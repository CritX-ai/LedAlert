//! Bounded native worker queries. The foreground GUI only opens the permission pane on request.
#![allow(unsafe_code)]
use super::{
    macos_state::{self, MAX_NOTIFICATIONS, Metadata, Notifications},
    *,
};
use std::{
    ffi::{c_char, c_void},
    ptr, slice, str,
    time::SystemTime,
};
use tokio::time::{sleep, timeout};

const POLL: Duration = Duration::from_millis(250);
const QUERY_TIMEOUT: Duration = Duration::from_millis(500);
const MAX_AUDIO_APPLICATIONS: usize = 32;
const NEED_ACCESS: &str = "macOS notifications unavailable: grant LedAlert Full Disk Access in Privacy & Security, then relaunch; lighting permission is separate";
const BAD_SCHEMA: &str =
    "macOS notification schema unavailable or changed; notification lighting is paused";

type NotificationSink = unsafe extern "C" fn(i64, *const u8, usize, f64, *mut c_void) -> i32;
type ApplicationSink = unsafe extern "C" fn(*const u8, usize, *mut c_void) -> i32;
unsafe extern "C" {
    fn ledalert_macos_notifications(
        path: *const c_char,
        sink: NotificationSink,
        context: *mut c_void,
    ) -> i32;
    fn ledalert_macos_lock() -> i32;
    fn ledalert_macos_audio(sink: ApplicationSink, context: *mut c_void) -> i32;
    fn ledalert_macos_open_notification_settings() -> i32;
}

pub(super) fn open_notification_settings() -> anyhow::Result<()> {
    // SAFETY: no borrowed values; the bridge requires the foreground main thread.
    anyhow::ensure!(
        unsafe { ledalert_macos_open_notification_settings() } == 0,
        "Could not open Privacy & Security; open Full Disk Access in System Settings manually"
    );
    Ok(())
}

pub(super) async fn run(shared: &Shared, output: &mpsc::SyncSender<NotificationEvent>) {
    tokio::join!(
        notification_loop(shared, output),
        lock_loop(shared),
        audio_loop(shared)
    );
}

// At most one native operation per collector may be in flight. A timeout clears its state
// immediately, then waits for that same job rather than leaking repeated blocked threads.
async fn query<T: Send + 'static>(
    operation: impl FnOnce() -> T + Send + 'static,
    unavailable: impl FnOnce(),
) -> Option<T> {
    let mut job = tokio::task::spawn_blocking(operation);
    match timeout(QUERY_TIMEOUT, &mut job).await {
        Ok(Ok(value)) => Some(value),
        Ok(Err(_)) => None,
        Err(_) => {
            unavailable();
            let _ = job.await;
            None
        }
    }
}

async fn notification_loop(shared: &Shared, output: &mpsc::SyncSender<NotificationEvent>) {
    let mut reducer = Notifications::default();
    loop {
        let result = query(notification_snapshot, || {
            reducer.disconnect(
                shared,
                "macOS notification query timed out; lifecycle reset",
            );
        })
        .await;
        match result {
            Some(Ok(records)) => {
                reducer.reconcile(records, shared, output, Instant::now(), SystemTime::now())
            }
            Some(Err(status)) => reducer.disconnect(shared, status),
            None => reducer.disconnect(
                shared,
                "macOS notification worker unavailable; lifecycle reset",
            ),
        }
        sleep(POLL).await;
    }
}

fn notification_snapshot() -> Result<Vec<Metadata>, &'static str> {
    snapshot_at(ptr::null())
}

fn snapshot_at(path: *const c_char) -> Result<Vec<Metadata>, &'static str> {
    let mut records: Vec<Metadata> = Vec::new();
    // SAFETY: the native callback is synchronous, borrows a SQLite row only for this call,
    // and is passed the live, exclusive Vec context. It never returns payload columns.
    let status = unsafe {
        ledalert_macos_notifications(
            path,
            notification_record,
            (&mut records as *mut Vec<Metadata>).cast(),
        )
    };
    match status {
        0 => Ok(records),
        1 => Err(NEED_ACCESS),
        2 => Err(BAD_SCHEMA),
        _ => Err("macOS notification snapshot incomplete or invalid; lifecycle reset"),
    }
}

unsafe extern "C" fn notification_record(
    id: i64,
    data: *const u8,
    length: usize,
    created: f64,
    context: *mut c_void,
) -> i32 {
    if data.is_null() || length == 0 || length > 512 || context.is_null() {
        return 0;
    }
    // SAFETY: native row bytes and exclusive callback context remain live until return.
    let records = unsafe { &mut *context.cast::<Vec<Metadata>>() };
    let Ok(application) = str::from_utf8(unsafe { slice::from_raw_parts(data, length) }) else {
        return 0;
    };
    if records.len() >= MAX_NOTIFICATIONS
        || !macos_state::valid_application(application)
        || id < 0
        || !created.is_finite()
        || created < 0.0
    {
        return 0;
    }
    records.push(Metadata {
        id,
        created,
        application: application.to_owned(),
    });
    1
}

async fn lock_loop(shared: &Shared) {
    loop {
        let observed = Instant::now();
        let result = query(
            || {
                // SAFETY: no arguments or retained native objects.
                match unsafe { ledalert_macos_lock() } {
                    0 => Some(false),
                    1 => Some(true),
                    _ => None,
                }
            },
            || macos_state::update_lock(shared, None, observed),
        )
        .await
        .flatten();
        macos_state::update_lock(shared, result, observed);
        sleep(POLL).await;
    }
}

async fn audio_loop(shared: &Shared) {
    loop {
        let observed = Instant::now();
        let playing = query(audio_snapshot, || {
            macos_state::update_media(shared, None, observed)
        })
        .await
        .flatten();
        macos_state::update_media(shared, playing, observed);
        sleep(Duration::from_millis(750)).await;
    }
}

fn audio_snapshot() -> Option<Vec<String>> {
    let mut playing: Vec<String> = Vec::new();
    // SAFETY: callbacks borrow valid bounded strings and the exclusive local Vec synchronously.
    let status = unsafe {
        ledalert_macos_audio(audio_application, (&mut playing as *mut Vec<String>).cast())
    };
    if status != 0 {
        return None;
    }
    playing.sort_unstable();
    Some(playing)
}

unsafe extern "C" fn audio_application(
    data: *const u8,
    length: usize,
    context: *mut c_void,
) -> i32 {
    if data.is_null() || length == 0 || length > 512 || context.is_null() {
        return 0;
    }
    // SAFETY: this synchronous callback owns the Vec access; the bridge bounds the string.
    let playing = unsafe { &mut *context.cast::<Vec<String>>() };
    let Ok(application) = str::from_utf8(unsafe { slice::from_raw_parts(data, length) }) else {
        return 0;
    };
    if !macos_state::valid_application(application) {
        return 0;
    }
    if playing.iter().any(|value| value == application) {
        return 1;
    }
    if playing.len() >= MAX_AUDIO_APPLICATIONS {
        return 0;
    }
    playing.push(application.to_owned());
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use tempfile::TempDir;

    unsafe extern "C" {
        fn sqlite3_open(path: *const c_char, database: *mut *mut c_void) -> i32;
        fn sqlite3_exec(
            database: *mut c_void,
            sql: *const c_char,
            callback: *const c_void,
            context: *mut c_void,
            error: *mut *mut c_char,
        ) -> i32;
        fn sqlite3_close(database: *mut c_void) -> i32;
    }

    struct Fixture {
        _directory: TempDir,
        path: CString,
    }
    impl Fixture {
        fn new(sql: &str) -> Self {
            let directory = TempDir::new().unwrap();
            let path = CString::new(
                directory
                    .path()
                    .canonicalize()
                    .unwrap()
                    .join("synthetic.db")
                    .to_str()
                    .unwrap(),
            )
            .unwrap();
            let value = Self {
                _directory: directory,
                path,
            };
            value.execute(sql);
            value
        }
        fn execute(&self, sql: &str) {
            let sql = CString::new(sql).unwrap();
            let mut db = ptr::null_mut();
            // SAFETY: isolated test-owned database, constant synthetic SQL, balanced connection.
            unsafe {
                assert_eq!(sqlite3_open(self.path.as_ptr(), &mut db), 0);
                let result = sqlite3_exec(
                    db,
                    sql.as_ptr(),
                    ptr::null(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                );
                assert_eq!(sqlite3_close(db), 0);
                assert_eq!(result, 0);
            }
        }
        fn snapshot(&self) -> Result<Vec<Metadata>, &'static str> {
            snapshot_at(self.path.as_ptr())
        }
    }

    // Installed macOS 27 schema: delivered.list is a set of packed UUIDs, not record IDs.
    const SCHEMA: &str = "CREATE TABLE app(app_id INTEGER PRIMARY KEY,identifier TEXT);
        CREATE TABLE record(rec_id INTEGER PRIMARY KEY,app_id INTEGER,uuid BLOB,data BLOB,
            request_date REAL,request_last_date REAL,delivered_date REAL,presented BOOL,
            style INTEGER,snooze_fire_date REAL);
        CREATE TABLE delivered(app_id INTEGER PRIMARY KEY,list BLOB);
        INSERT INTO app VALUES(1,'org.example.Synthetic');
        INSERT INTO record(rec_id,app_id,uuid,delivered_date,data)
            VALUES(4294967297,1,X'00112233445566778899AABBCCDDEEFF',99.5,X'00FF');
        INSERT INTO record(rec_id,app_id,uuid,delivered_date,data)
            VALUES(2,1,X'FFEEDDCCBBAA99887766554433221100',50.0,X'FF00');
        INSERT INTO delivered VALUES(1,X'00112233445566778899AABBCCDDEEFF');";

    #[test]
    fn native_projection_tracks_delivery_membership_not_retained_payload_history() {
        let fixture = Fixture::new(SCHEMA);
        let first = fixture.snapshot().unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id, 4294967297);
        assert_eq!(first[0].application, "org.example.Synthetic");
        assert_eq!(first[0].created, 99.5);
        fixture.execute("DELETE FROM delivered WHERE app_id=1;");
        assert!(
            fixture.snapshot().unwrap().is_empty(),
            "dismissal leaves historical record but no active delivery"
        );
        fixture.execute("INSERT INTO delivered VALUES(1,X'FFEEDDCCBBAA99887766554433221100');");
        assert_eq!(fixture.snapshot().unwrap()[0].id, 2);
    }

    #[test]
    fn native_projection_rejects_payload_redirection_and_unknown_schema() {
        let fixture = Fixture::new(SCHEMA);
        fixture.execute("DROP TABLE delivered;");
        assert!(fixture.snapshot().is_err());
        fixture.execute("CREATE VIEW delivered AS SELECT app_id,data AS list FROM record;");
        assert!(
            fixture.snapshot().is_err(),
            "authorizer must reject any data-column read"
        );
    }

    #[test]
    fn native_projection_does_not_create_missing_files_and_rejects_oversize_identity() {
        let directory = TempDir::new().unwrap();
        let missing = directory.path().canonicalize().unwrap().join("missing.db");
        let path = CString::new(missing.to_str().unwrap()).unwrap();
        assert!(snapshot_at(path.as_ptr()).is_err());
        assert!(!missing.exists());
        let fixture = Fixture::new(SCHEMA);
        fixture.execute(&format!("UPDATE app SET identifier='{}';", "x".repeat(513)));
        assert!(fixture.snapshot().is_err());
    }

    #[test]
    fn native_projection_rejects_an_orphaned_delivery_instead_of_a_partial_snapshot() {
        let fixture = Fixture::new(SCHEMA);
        fixture.execute(
            "UPDATE delivered SET list=CAST(list||X'99999999999999999999999999999999' AS BLOB);",
        );
        assert!(fixture.snapshot().is_err());
    }

    #[test]
    fn native_projection_keeps_delivery_membership_scoped_to_its_application() {
        let fixture = Fixture::new(SCHEMA);
        fixture.execute(
            "INSERT INTO app VALUES(2,'org.example.Other');
             INSERT INTO record(rec_id,app_id,uuid,delivered_date)
                 VALUES(3,2,X'00112233445566778899AABBCCDDEEFF',100.0);",
        );
        let records = fixture.snapshot().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, 4294967297);
        assert_eq!(records[0].application, "org.example.Synthetic");
        fixture.execute("UPDATE record SET app_id=2 WHERE rec_id=4294967297;");
        assert!(
            fixture.snapshot().is_err(),
            "another app cannot satisfy this delivery"
        );
    }

    #[test]
    fn native_projection_rejects_malformed_duplicate_and_oversize_uuid_membership() {
        for sql in [
            "UPDATE delivered SET list=X'00';",
            "UPDATE delivered SET list=CAST(list||list AS BLOB);",
            "UPDATE delivered SET list=zeroblob(16*4097);",
            "INSERT INTO record(rec_id,app_id,uuid,delivered_date)
                VALUES(3,1,X'00112233445566778899AABBCCDDEEFF',100.0);",
        ] {
            let fixture = Fixture::new(SCHEMA);
            fixture.execute(sql);
            assert!(fixture.snapshot().is_err());
        }
    }

    #[test]
    fn native_projection_accepts_null_and_zero_byte_empty_delivery_sets() {
        let fixture = Fixture::new(SCHEMA);
        fixture.execute("UPDATE delivered SET list=NULL;");
        assert!(fixture.snapshot().unwrap().is_empty());
        fixture.execute("UPDATE delivered SET list=X'';");
        assert!(fixture.snapshot().unwrap().is_empty());
    }
}
