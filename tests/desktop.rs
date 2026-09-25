//! Every scenario executes in a child test process on an explicitly configured private bus.
//! No test changes the parent environment or connects to the user's desktop session.

#![cfg(target_os = "linux")]

use std::{
    collections::HashMap,
    num::NonZeroU32,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use ledalert::desktop::{DesktopMonitor, Notification, NotificationEvent};
use tokio::time::{sleep, timeout};
use zbus::{
    Connection, Message, MessageStream, Proxy,
    zvariant::{OwnedValue, Value},
};

const CHILD_ROOT: &str = "LEDALERT_PRIVATE_DESKTOP_TEST_ROOT";
const WAIT: Duration = Duration::from_secs(8);

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_bus(root: &Path) -> Daemon {
    let mut daemon = Daemon(
        Command::new("dbus-daemon")
            .arg("--nofork")
            .arg(format!("--config-file={}", root.join("bus.conf").display()))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("isolated desktop tests require dbus-daemon"),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join("bus").exists() {
        assert!(
            daemon.0.try_wait().unwrap().is_none(),
            "private D-Bus daemon exited"
        );
        assert!(
            Instant::now() < deadline,
            "private D-Bus daemon did not create its socket"
        );
        thread::sleep(Duration::from_millis(10));
    }
    daemon
}

// Returns a validated private address in the child; runs and supervises the child in the parent.
fn private_scenario(name: &str, deny_monitor: bool) -> Option<PathBuf> {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        assert!(root.join("bus.conf").is_file());
        assert_eq!(
            std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(),
            format!("unix:path={}", root.join("bus").display())
        );
        return Some(root);
    }
    let root = tempfile::Builder::new()
        .prefix("ledalert-desktop-")
        .tempdir()
        .unwrap();
    let deny = if deny_monitor {
        "<deny send_interface=\"org.freedesktop.DBus.Monitoring\" send_member=\"BecomeMonitor\"/>"
    } else {
        ""
    };
    let configuration = format!(
        "<busconfig><type>session</type><listen>unix:path={}</listen>\
        <auth>EXTERNAL</auth><policy context=\"default\"><allow user=\"*\"/>\
        <allow own=\"*\"/><allow send_destination=\"*\" eavesdrop=\"true\"/>\
        <allow receive_sender=\"*\" eavesdrop=\"true\"/>{deny}</policy></busconfig>",
        root.path().join("bus").display()
    );
    std::fs::write(root.path().join("bus.conf"), configuration).unwrap();
    let mut daemon = Some(start_bus(root.path()));
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env(CHILD_ROOT, root.path())
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", root.path().join("bus").display()),
        )
        .env_remove("DBUS_STARTER_ADDRESS")
        .env_remove("DBUS_STARTER_BUS_TYPE")
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "isolated desktop scenario failed: {name}");
            return None;
        }
        if root.path().join("stop-bus").exists() && daemon.is_some() {
            drop(daemon.take());
            // A killed daemon can leave a stale pathname behind.
            let _ = std::fs::remove_file(root.path().join("bus"));
            std::fs::remove_file(root.path().join("stop-bus")).unwrap();
            std::fs::write(root.path().join("bus-stopped"), "").unwrap();
        }
        if root.path().join("start-bus").exists() && daemon.is_none() {
            daemon = Some(start_bus(root.path()));
            std::fs::remove_file(root.path().join("start-bus")).unwrap();
            std::fs::write(root.path().join("bus-started"), "").unwrap();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("isolated desktop scenario timed out: {name}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

async fn eventually(mut predicate: impl FnMut() -> bool) {
    timeout(WAIT, async {
        while !predicate() {
            sleep(Duration::from_millis(15)).await;
        }
    })
    .await
    .expect("desktop behavior did not converge on the private bus");
}

async fn next_event(monitor: &DesktopMonitor) -> NotificationEvent {
    timeout(WAIT, async {
        loop {
            if let Some(event) = monitor.try_notification() {
                return event;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("synthetic notification lifecycle was not observed")
}

async fn next_notification(monitor: &DesktopMonitor) -> Notification {
    match next_event(monitor).await {
        NotificationEvent::Raised(notification) => notification,
        NotificationEvent::Closed { .. } => panic!("expected a raised notification, got a close"),
    }
}

async fn assert_closed(monitor: &DesktopMonitor, generation: u64, id: u32, reason: u32) {
    match next_event(monitor).await {
        NotificationEvent::Closed {
            generation: actual_generation,
            id: actual_id,
            reason: actual_reason,
        } => {
            assert_eq!(
                (actual_generation, actual_id, actual_reason),
                (generation, id, reason)
            );
        }
        NotificationEvent::Raised(_) => panic!("expected a close, got a raised notification"),
    }
}

async fn closed(connection: &Connection, id: u32, reason: u32) {
    connection
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            "NotificationClosed",
            &(id, reason),
        )
        .await
        .unwrap();
}

async fn raw_daemon() -> (Connection, MessageStream) {
    let connection = Connection::session().await.unwrap();
    let stream = MessageStream::from(&connection);
    connection
        .request_name("org.freedesktop.Notifications")
        .await
        .unwrap();
    (connection, stream)
}

async fn raw_notify(
    sender: &Connection,
    daemon: &mut MessageStream,
    application: &str,
    replaces: u32,
    serial: u32,
) -> Message {
    let call = Message::method_call("/org/freedesktop/Notifications", "Notify")
        .unwrap()
        .destination("org.freedesktop.Notifications")
        .unwrap()
        .interface("org.freedesktop.Notifications")
        .unwrap()
        .serial(NonZeroU32::new(serial).unwrap())
        .build(&(
            application,
            replaces,
            "",
            "",
            "",
            Vec::<&str>::new(),
            HashMap::<&str, Value<'_>>::new(),
            -1_i32,
        ))
        .unwrap();
    sender.send(&call).await.unwrap();
    timeout(WAIT, async {
        loop {
            let message = daemon.next().await.unwrap().unwrap();
            if message.message_type() == zbus::message::Type::MethodCall
                && message
                    .header()
                    .member()
                    .is_some_and(|member| member.as_str() == "Notify")
            {
                return message;
            }
        }
    })
    .await
    .expect("raw notification did not reach the daemon")
}

async fn monitoring(monitor: &DesktopMonitor) {
    eventually(|| {
        monitor
            .snapshot()
            .notification_status
            .contains("metadata only")
    })
    .await;
}

#[derive(Clone, Default)]
struct ExistingNotifications {
    calls: Arc<AtomicUsize>,
    replacement: Arc<AtomicU32>,
    payload_intact: Arc<AtomicBool>,
    next_id: Arc<AtomicU32>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl ExistingNotifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        _application: &str,
        replaces_id: u32,
        icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        _hints: HashMap<String, OwnedValue>,
        _expiry: i32,
    ) -> u32 {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.replacement.store(replaces_id, Ordering::SeqCst);
        self.payload_intact.store(
            icon == "synthetic-icon"
                && summary == "SYNTHETIC_SUMMARY"
                && body == "SYNTHETIC_PRIVATE_BODY"
                && actions == ["open", "SYNTHETIC_ACTION"],
            Ordering::SeqCst,
        );
        if replaces_id == 0 {
            41 + self.next_id.fetch_add(1, Ordering::SeqCst)
        } else {
            replaces_id
        }
    }
}

async fn notification_daemon(daemon: ExistingNotifications) -> Connection {
    zbus::connection::Builder::session()
        .unwrap()
        .name("org.freedesktop.Notifications")
        .unwrap()
        .serve_at("/org/freedesktop/Notifications", daemon)
        .unwrap()
        .build()
        .await
        .unwrap()
}

async fn notify(
    connection: &Connection,
    application: &str,
    replaces: u32,
    body: &str,
    hints: HashMap<&str, Value<'_>>,
) -> u32 {
    timeout(WAIT, async {
        Proxy::new(
            connection,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )
        .await
        .unwrap()
        .call(
            "Notify",
            &(
                application,
                replaces,
                "synthetic-icon",
                "SYNTHETIC_SUMMARY",
                body,
                vec!["open", "SYNTHETIC_ACTION"],
                hints,
                -1_i32,
            ),
        )
        .await
        .unwrap()
    })
    .await
    .expect("the existing daemon was blocked by the observer")
}

fn synthetic_hints() -> HashMap<&'static str, Value<'static>> {
    HashMap::from([
        (
            "desktop-entry",
            Value::from("org.example.Synthetic.desktop"),
        ),
        ("urgency", Value::from(2_u8)),
        (
            "unrelated-private-hint",
            Value::from("SYNTHETIC_HINT_PRIVATE_BODY"),
        ),
        ("image-data", Value::from(vec![7_u8; 128])),
    ])
}

#[test]
fn passive_notification_metadata_and_boundaries() {
    let Some(_root) = private_scenario("passive_notification_metadata_and_boundaries", false)
    else {
        return;
    };
    runtime().block_on(async {
        let existing = ExistingNotifications::default();
        let daemon = notification_daemon(existing.clone()).await;
        let sender = Connection::session().await.unwrap();
        let monitor = DesktopMonitor::start().unwrap();
        eventually(|| {
            monitor
                .snapshot()
                .notification_status
                .contains("metadata only")
        })
        .await;

        let before = Instant::now();
        let id = notify(
            &sender,
            "Synthetic Display Name",
            0,
            "SYNTHETIC_PRIVATE_BODY",
            synthetic_hints(),
        )
        .await;
        assert_eq!(id, 41);
        assert!(existing.payload_intact.load(Ordering::SeqCst));
        let event = next_notification(&monitor).await;
        // The hint is already a desktop-file prefix. Its own ".desktop" ending
        // belongs to the application ID, as with org.telegram.desktop.
        assert_eq!(event.application, "org.example.Synthetic.desktop");
        assert_eq!(event.urgency, 2);
        assert_eq!(event.id, id);
        assert_eq!(event.generation, monitor.snapshot().notification_generation);
        assert!(event.received >= before && event.received <= Instant::now());
        let pin = ledalert::taskbar::PinnedApp {
            id: "org.example.Synthetic.desktop".into(),
            name: "Synthetic".into(),
            kind: ledalert::taskbar::AppKind::Communication,
            suggested: true,
            icon: None,
        };
        let base = ledalert::config::Config::default();
        let rule =
            ledalert::taskbar::suggested_rule(&pin, base.room.screens[0].id, base.room.width);
        let config = ledalert::config::Config {
            rules: vec![rule],
            ..base
        };
        let mut renderer = ledalert::engine::Engine::new(&config).unwrap();
        assert!(renderer.notify(
            &event.application,
            event.urgency,
            event.received,
            event.received,
            Some((event.generation, event.id))
        ));

        // Replacements still reach the original daemon unchanged and produce fresh metadata.
        assert_eq!(
            notify(
                &sender,
                "Ignored Display Name",
                id,
                "SYNTHETIC_PRIVATE_BODY",
                synthetic_hints()
            )
            .await,
            id
        );
        assert_eq!(existing.replacement.load(Ordering::SeqCst), id);
        let replacement = next_notification(&monitor).await;
        assert_eq!(replacement.application, event.application);
        assert_eq!(
            (replacement.generation, replacement.id),
            (event.generation, id)
        );
        assert!(replacement.received >= event.received);
        let bus = zbus::fdo::DBusProxy::new(&sender).await.unwrap();
        assert_eq!(
            bus.get_name_owner("org.freedesktop.Notifications".try_into().unwrap())
                .await
                .unwrap()
                .as_str(),
            daemon.unique_name().unwrap().as_str()
        );

        // Discard private body/actions/unknown hints and strip presentation control characters.
        notify(
            &sender,
            "\u{202e}Synthetic\n Safe\u{0007}",
            0,
            "SYNTHETIC_PRIVATE_BODY",
            HashMap::from([("urgency", Value::from(250_u8))]),
        )
        .await;
        let sanitized = next_notification(&monitor).await;
        assert_eq!(sanitized.application, "Synthetic Safe");
        assert_eq!(sanitized.urgency, 1);
        let state = monitor.snapshot();
        for visible in [
            &state.notification_status,
            &state.media_status,
            &state.lock_status,
        ] {
            assert!(
                !visible.contains("SYNTHETIC_PRIVATE") && !visible.contains("SYNTHETIC_SUMMARY")
            );
        }

        // A too-large body or app identifier is dropped, not truncated into a valid event.
        notify(
            &sender,
            "OversizedBody",
            0,
            &"x".repeat(70 * 1024),
            HashMap::new(),
        )
        .await;
        notify(&sender, &"A".repeat(2048), 0, "", HashMap::new()).await;
        let oversized_entry = "A".repeat(2048);
        notify(
            &sender,
            "OversizedDesktopEntry",
            0,
            "",
            HashMap::from([("desktop-entry", Value::from(oversized_entry.as_str()))]),
        )
        .await;
        notify(&sender, "AfterOversized", 0, "", HashMap::new()).await;
        assert_eq!(
            next_notification(&monitor).await.application,
            "AfterOversized"
        );
        assert!(monitor.try_notification().is_none());

        // Saturating the consumer cannot delay the daemon or hide a close indefinitely.
        let generation = monitor.snapshot().notification_generation;
        let count_before = existing.calls.load(Ordering::SeqCst);
        let mut first_id = 0;
        for index in 0..64 {
            let id = notify(&sender, &format!("Burst{index:02}"), 0, "", HashMap::new()).await;
            if index == 0 {
                first_id = id;
            }
        }
        closed(&daemon, first_id, 2).await;
        eventually(|| monitor.snapshot().notification_generation != generation).await;
        assert_eq!(existing.calls.load(Ordering::SeqCst), count_before + 64);
        // Every queued raise belongs to the invalidated generation. A consumer can clear its
        // persistent lights from the snapshot even though the close itself did not fit.
        let current_generation = monitor.snapshot().notification_generation;
        let mut received = Vec::new();
        while let Some(event) = monitor.try_notification() {
            match event {
                NotificationEvent::Raised(notification) => {
                    assert_eq!(notification.generation, generation);
                    assert_ne!(notification.generation, current_generation);
                    received.push(notification.application);
                }
                NotificationEvent::Closed { .. } => panic!("full queue accepted a close"),
            }
        }
        assert_eq!(received.len(), 64);
        assert_eq!(received.first().unwrap(), "Burst00");
        assert_eq!(received.last().unwrap(), "Burst63");
        let id = notify(&sender, "AfterOverflow", 0, "", HashMap::new()).await;
        let recovered = next_notification(&monitor).await;
        assert_eq!(
            (recovered.generation, recovered.id),
            (current_generation, id)
        );
        closed(&daemon, id, 3).await;
        assert_closed(&monitor, current_generation, id, 3).await;
        drop(monitor);
    });
}

#[test]
fn daemon_reply_correlation_and_independent_closes() {
    let Some(_root) = private_scenario("daemon_reply_correlation_and_independent_closes", false)
    else {
        return;
    };
    runtime().block_on(async {
        let (daemon, mut calls) = raw_daemon().await;
        let first_sender = Connection::session().await.unwrap();
        let second_sender = Connection::session().await.unwrap();
        let untrusted = Connection::session().await.unwrap();
        let monitor = DesktopMonitor::start().unwrap();
        monitoring(&monitor).await;
        let generation = monitor.snapshot().notification_generation;

        // Different callers may use the same serial, and daemons may return in a different order.
        let before = Instant::now();
        let first = raw_notify(&first_sender, &mut calls, "First", 0, 8001).await;
        let second = raw_notify(&second_sender, &mut calls, "Second", 0, 8001).await;
        untrusted.reply(&first.header(), &777_u32).await.unwrap();
        closed(&untrusted, 501, 2).await;
        untrusted
            .emit_signal(
                None::<&str>,
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "NameOwnerChanged",
                &(
                    "org.freedesktop.Notifications",
                    daemon.unique_name().unwrap().as_str(),
                    "",
                ),
            )
            .await
            .unwrap();
        // Merely using the Notify interface/path does not make an unrelated destination a daemon.
        let unrelated = Message::method_call("/org/freedesktop/Notifications", "Notify")
            .unwrap()
            .destination(first_sender.unique_name().unwrap().as_str())
            .unwrap()
            .interface("org.freedesktop.Notifications")
            .unwrap()
            .build(&())
            .unwrap();
        untrusted.send(&unrelated).await.unwrap();
        // A round trip fences the untrusted socket before the genuine daemon's sentinel signal.
        zbus::fdo::DBusProxy::new(&untrusted)
            .await
            .unwrap()
            .get_id()
            .await
            .unwrap();
        closed(&daemon, 999, 2).await;
        assert_closed(&monitor, generation, 999, 2).await;
        assert_eq!(monitor.snapshot().notification_generation, generation);
        let replying = Instant::now();
        daemon.reply(&second.header(), &502_u32).await.unwrap();
        let second_event = next_notification(&monitor).await;
        assert_eq!(
            (second_event.application.as_str(), second_event.id),
            ("Second", 502)
        );
        daemon.reply(&first.header(), &501_u32).await.unwrap();
        let first_event = next_notification(&monitor).await;
        assert_eq!(
            (first_event.application.as_str(), first_event.id),
            ("First", 501)
        );
        assert!(first_event.received >= before && first_event.received <= replying);
        assert!(second_event.received >= before && second_event.received <= replying);

        // A daemon may reject a replacement ID and allocate another. Never guess from replaces_id.
        let replacement = raw_notify(&first_sender, &mut calls, "Replacement", 501, 8002).await;
        daemon.reply(&replacement.header(), &901_u32).await.unwrap();
        let replacement = next_notification(&monitor).await;
        assert_eq!((replacement.id, replacement.generation), (901, generation));
        closed(&daemon, 502, 1).await;
        closed(&daemon, 501, 2).await;
        closed(&daemon, 901, 77).await;
        assert_closed(&monitor, generation, 502, 1).await;
        assert_closed(&monitor, generation, 501, 2).await;
        assert_closed(&monitor, generation, 901, 77).await;

        let rejected = raw_notify(&first_sender, &mut calls, "Rejected", 0, 8003).await;
        daemon
            .reply_error(
                &rejected.header(),
                "org.freedesktop.DBus.Error.Failed",
                &"Synthetic rejection",
            )
            .await
            .unwrap();
        closed(&daemon, 999, 3).await;
        assert_closed(&monitor, generation, 999, 3).await;
        assert!(monitor.try_notification().is_none());
    });
}

#[test]
fn close_before_reply_does_not_resurrect_or_poison_reused_id() {
    let Some(_root) = private_scenario(
        "close_before_reply_does_not_resurrect_or_poison_reused_id",
        false,
    ) else {
        return;
    };
    runtime().block_on(async {
        let (daemon, mut calls) = raw_daemon().await;
        let sender = Connection::session().await.unwrap();
        let monitor = DesktopMonitor::start().unwrap();
        monitoring(&monitor).await;
        let generation = monitor.snapshot().notification_generation;
        let call = raw_notify(&sender, &mut calls, "ShortLived", 0, 8101).await;
        closed(&daemon, 610, 1).await;
        assert_closed(&monitor, generation, 610, 1).await;
        daemon.reply(&call.header(), &610_u32).await.unwrap();
        let event = next_notification(&monitor).await;
        assert_eq!((event.generation, event.id), (generation, 610));
        // Consumers get the close again after the correlated raise; expiry policy belongs to Main.
        assert_closed(&monitor, generation, 610, 1).await;

        let call = raw_notify(&sender, &mut calls, "Reused", 0, 8102).await;
        daemon.reply(&call.header(), &610_u32).await.unwrap();
        let event = next_notification(&monitor).await;
        assert_eq!((event.application.as_str(), event.id), ("Reused", 610));
        closed(&daemon, 999, 2).await;
        assert_closed(&monitor, generation, 999, 2).await;
        assert!(monitor.try_notification().is_none());
    });
}

#[test]
fn daemon_replacement_invalidates_active_queued_and_inflight_ids() {
    let Some(_root) = private_scenario(
        "daemon_replacement_invalidates_active_queued_and_inflight_ids",
        false,
    ) else {
        return;
    };
    runtime().block_on(async {
        let (old_daemon, mut old_calls) = raw_daemon().await;
        let sender = Connection::session().await.unwrap();
        let monitor = DesktopMonitor::start().unwrap();
        monitoring(&monitor).await;
        let generation = monitor.snapshot().notification_generation;
        let call = raw_notify(&sender, &mut old_calls, "BeforeReset", 0, 8201).await;
        old_daemon.reply(&call.header(), &41_u32).await.unwrap();
        assert_eq!(next_notification(&monitor).await.id, 41);
        let queued = raw_notify(&sender, &mut old_calls, "QueuedBeforeReset", 0, 8202).await;
        old_daemon.reply(&queued.header(), &42_u32).await.unwrap();
        let pending = raw_notify(&sender, &mut old_calls, "PendingBeforeReset", 0, 8203).await;
        old_daemon
            .release_name("org.freedesktop.Notifications")
            .await
            .unwrap();
        let (new_daemon, mut new_calls) = raw_daemon().await;
        eventually(|| monitor.snapshot().notification_generation != generation).await;
        // The old connection remains alive, but is no longer entitled to reply or close.
        old_daemon.reply(&pending.header(), &41_u32).await.unwrap();
        closed(&old_daemon, 41, 2).await;
        zbus::fdo::DBusProxy::new(&old_daemon)
            .await
            .unwrap()
            .get_id()
            .await
            .unwrap();
        closed(&new_daemon, 999, 2).await;
        let queued = next_notification(&monitor).await;
        assert_eq!((queued.generation, queued.id), (generation, 42));
        match next_event(&monitor).await {
            NotificationEvent::Closed {
                generation: current,
                id: 999,
                reason: 2,
            } => {
                assert_ne!(current, generation);
                assert_eq!(current, monitor.snapshot().notification_generation);
            }
            _ => panic!("an old daemon event survived ownership replacement"),
        }

        // Even the same caller serial and daemon ID are fresh only in the new generation.
        let call = raw_notify(&sender, &mut new_calls, "AfterReset", 0, 8203).await;
        new_daemon.reply(&call.header(), &41_u32).await.unwrap();
        let event = next_notification(&monitor).await;
        assert_ne!(event.generation, generation);
        assert_eq!((event.application.as_str(), event.id), ("AfterReset", 41));
        closed(&new_daemon, 41, 3).await;
        assert_closed(&monitor, event.generation, 41, 3).await;
        assert!(monitor.try_notification().is_none());
    });
}

#[test]
fn inflight_overflow_and_late_reply_cannot_restore_uncertain_lights() {
    let Some(_root) = private_scenario(
        "inflight_overflow_and_late_reply_cannot_restore_uncertain_lights",
        false,
    ) else {
        return;
    };
    runtime().block_on(async {
        let (daemon, mut calls) = raw_daemon().await;
        let sender = Connection::session().await.unwrap();
        let monitor = DesktopMonitor::start().unwrap();
        monitoring(&monitor).await;
        let generation = monitor.snapshot().notification_generation;
        let mut pending = Vec::new();
        for index in 0..65 {
            pending.push(raw_notify(&sender, &mut calls, "Pending", 0, 9000 + index).await);
        }
        closed(&daemon, 999, 2).await;
        let current = match next_event(&monitor).await {
            NotificationEvent::Closed {
                generation: current,
                id: 999,
                reason: 2,
            } => current,
            _ => panic!("unanswered Notify produced an event"),
        };
        assert_ne!(current, generation);
        for call in &pending[..64] {
            daemon.reply(&call.header(), &41_u32).await.unwrap();
        }
        daemon.reply(&pending[64].header(), &777_u32).await.unwrap();
        let event = next_notification(&monitor).await;
        assert_eq!((event.generation, event.id), (current, 777));
        assert!(monitor.try_notification().is_none());

        let expired = raw_notify(&sender, &mut calls, "ReplyNeverArrived", 0, 9100).await;
        eventually(|| monitor.snapshot().notification_generation != current).await;
        let after_timeout = monitor.snapshot().notification_generation;
        daemon.reply(&expired.header(), &777_u32).await.unwrap();
        closed(&daemon, 999, 2).await;
        assert_closed(&monitor, after_timeout, 999, 2).await;
        let call = raw_notify(&sender, &mut calls, "AfterTimeout", 0, 9101).await;
        daemon.reply(&call.header(), &778_u32).await.unwrap();
        let event = next_notification(&monitor).await;
        assert_eq!((event.generation, event.id), (after_timeout, 778));

        let invalid = raw_notify(&sender, &mut calls, "InvalidDaemonId", 778, 9102).await;
        daemon.reply(&invalid.header(), &0_u32).await.unwrap();
        closed(&daemon, 999, 2).await;
        match next_event(&monitor).await {
            NotificationEvent::Closed {
                generation,
                id: 999,
                reason: 2,
            } => {
                assert_ne!(generation, after_timeout);
                assert_eq!(generation, monitor.notification_generation());
            }
            _ => panic!("an invalid daemon ID produced a raised notification"),
        }
    });
}

#[derive(Clone)]
struct LockService(Arc<AtomicU8>);

#[zbus::interface(name = "org.freedesktop.ScreenSaver")]
impl LockService {
    async fn get_active(&self) -> zbus::fdo::Result<bool> {
        match self.0.load(Ordering::SeqCst) {
            0 => Ok(false),
            1 => Ok(true),
            2 => Err(zbus::fdo::Error::Failed("synthetic service failure".into())),
            _ => std::future::pending().await,
        }
    }
}

async fn lock_service(mode: Arc<AtomicU8>) -> Connection {
    zbus::connection::Builder::session()
        .unwrap()
        .name("org.freedesktop.ScreenSaver")
        .unwrap()
        .serve_at("/ScreenSaver", LockService(mode))
        .unwrap()
        .build()
        .await
        .unwrap()
}

#[derive(Clone)]
struct MediaService {
    playback: Arc<AtomicU8>,
    missing_entry: Arc<AtomicBool>,
    metadata_reads: Arc<AtomicUsize>,
    entry: &'static str,
}

struct MediaRoot(MediaService);
struct MediaPlayer(MediaService);

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl MediaRoot {
    #[zbus(property)]
    fn desktop_entry(&self) -> zbus::fdo::Result<&str> {
        if self.0.missing_entry.load(Ordering::SeqCst) {
            Err(zbus::fdo::Error::UnknownProperty(
                "synthetic missing entry".into(),
            ))
        } else {
            Ok(self.0.entry)
        }
    }
    #[zbus(property)]
    fn identity(&self) -> &str {
        "Synthetic Media Identity"
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl MediaPlayer {
    #[zbus(property)]
    fn playback_status(&self) -> &str {
        match self.0.playback.load(Ordering::SeqCst) {
            1 => "Playing",
            2 => "Paused",
            _ => "Stopped",
        }
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        self.0.metadata_reads.fetch_add(1, Ordering::SeqCst);
        HashMap::new()
    }
}

async fn media_service(service: MediaService) -> Connection {
    zbus::connection::Builder::session()
        .unwrap()
        .name("org.mpris.MediaPlayer2.synthetic")
        .unwrap()
        .serve_at("/org/mpris/MediaPlayer2", MediaRoot(service.clone()))
        .unwrap()
        .serve_at("/org/mpris/MediaPlayer2", MediaPlayer(service))
        .unwrap()
        .build()
        .await
        .unwrap()
}

#[test]
fn lock_media_lifecycle_and_bus_reconnection() {
    let Some(root) = private_scenario("lock_media_lifecycle_and_bus_reconnection", false) else {
        return;
    };
    runtime().block_on(async {
        let monitor = DesktopMonitor::start().unwrap();
        assert_eq!(monitor.snapshot().locked, None);
        eventually(|| monitor.snapshot().lock_status.contains("unavailable")).await;
        assert_eq!(monitor.snapshot().locked, None);

        let lock_mode = Arc::new(AtomicU8::new(0));
        let mut lock = lock_service(Arc::clone(&lock_mode)).await;
        eventually(|| monitor.snapshot().locked == Some(false)).await;
        lock_mode.store(1, Ordering::SeqCst);
        eventually(|| monitor.snapshot().locked == Some(true)).await;
        lock_mode.store(2, Ordering::SeqCst);
        eventually(|| monitor.snapshot().locked.is_none()).await;
        lock_mode.store(0, Ordering::SeqCst);
        eventually(|| monitor.snapshot().locked == Some(false)).await;
        lock_mode.store(3, Ordering::SeqCst);
        eventually(|| monitor.snapshot().locked.is_none()).await;
        lock_mode.store(0, Ordering::SeqCst);
        eventually(|| monitor.snapshot().locked == Some(false)).await;

        let media = MediaService {
            playback: Arc::new(AtomicU8::new(0)),
            missing_entry: Arc::new(AtomicBool::new(false)),
            metadata_reads: Arc::new(AtomicUsize::new(0)),
            entry: "org.example.Media.desktop",
        };
        let player = media_service(media.clone()).await;
        assert!(monitor.snapshot().playing.is_empty());
        media.playback.store(1, Ordering::SeqCst);
        eventually(|| monitor.snapshot().playing == ["org.example.Media.desktop"]).await;
        media.playback.store(0, Ordering::SeqCst);
        eventually(|| monitor.snapshot().playing.is_empty()).await;
        media.playback.store(1, Ordering::SeqCst);
        eventually(|| monitor.snapshot().playing == ["org.example.Media.desktop"]).await;
        media.playback.store(2, Ordering::SeqCst);
        eventually(|| monitor.snapshot().playing.is_empty()).await;
        media.playback.store(1, Ordering::SeqCst);
        media.missing_entry.store(true, Ordering::SeqCst);
        eventually(|| monitor.snapshot().playing == ["Synthetic Media Identity"]).await;
        assert_eq!(
            media.metadata_reads.load(Ordering::SeqCst),
            0,
            "song metadata must never be read"
        );
        player.close().await.unwrap();
        eventually(|| monitor.snapshot().playing.is_empty()).await;

        let replacement = MediaService {
            entry: "org.example.Replacement",
            ..media.clone()
        };
        replacement.missing_entry.store(false, Ordering::SeqCst);
        let player = media_service(replacement.clone()).await;
        eventually(|| monitor.snapshot().playing == ["org.example.Replacement"]).await;
        lock.close().await.unwrap();
        eventually(|| monitor.snapshot().locked.is_none()).await;
        lock = lock_service(Arc::clone(&lock_mode)).await;
        eventually(|| monitor.snapshot().locked == Some(false)).await;

        // Kill/restart only the private bus at the same address. No stale unlock or player may
        // survive a disconnect, and both connections must reattach without restarting the GUI.
        let notification_generation = monitor.snapshot().notification_generation;
        std::fs::write(root.join("stop-bus"), "").unwrap();
        eventually(|| root.join("bus-stopped").exists()).await;
        eventually(|| {
            let state = monitor.snapshot();
            state.locked.is_none()
                && state.playing.is_empty()
                && state.notification_status.contains("reconnect")
        })
        .await;
        assert_ne!(
            monitor.snapshot().notification_generation,
            notification_generation
        );
        drop(lock);
        drop(player);
        std::fs::write(root.join("start-bus"), "").unwrap();
        eventually(|| root.join("bus-started").exists()).await;
        let lock = lock_service(Arc::clone(&lock_mode)).await;
        let _player = media_service(replacement.clone()).await;
        eventually(|| monitor.snapshot().locked == Some(false)).await;
        eventually(|| monitor.snapshot().playing == ["org.example.Replacement"]).await;
        let _notifications = notification_daemon(ExistingNotifications::default()).await;
        let sender = Connection::session().await.unwrap();
        eventually(|| {
            monitor
                .snapshot()
                .notification_status
                .contains("metadata only")
        })
        .await;
        notify(&sender, "AfterBusRestart", 0, "", HashMap::new()).await;
        assert_eq!(
            next_notification(&monitor).await.application,
            "AfterBusRestart"
        );
        assert_eq!(media.metadata_reads.load(Ordering::SeqCst), 0);

        lock_mode.store(3, Ordering::SeqCst);
        eventually(|| monitor.snapshot().locked.is_none()).await;
        let before = Instant::now();
        drop(monitor);
        assert!(
            before.elapsed() < Duration::from_secs(1),
            "drop hung on an unresponsive lock RPC"
        );
        drop(lock);
    });
}

#[test]
fn denied_passive_monitor_preserves_existing_daemon() {
    let Some(_root) = private_scenario("denied_passive_monitor_preserves_existing_daemon", true)
    else {
        return;
    };
    runtime().block_on(async {
        let existing = ExistingNotifications::default();
        let _daemon = notification_daemon(existing.clone()).await;
        let sender = Connection::session().await.unwrap();
        let monitor = DesktopMonitor::start().unwrap();
        eventually(|| monitor.snapshot().notification_status.contains("denied")).await;
        assert_eq!(
            notify(
                &sender,
                "Synthetic Denied",
                0,
                "SYNTHETIC_PRIVATE_BODY",
                synthetic_hints()
            )
            .await,
            41
        );
        assert_eq!(existing.calls.load(Ordering::SeqCst), 1);
        assert!(existing.payload_intact.load(Ordering::SeqCst));
        assert!(monitor.try_notification().is_none());
        assert_eq!(monitor.snapshot().locked, None);
        let before = Instant::now();
        drop(monitor);
        assert!(before.elapsed() < Duration::from_secs(1));
    });
}
