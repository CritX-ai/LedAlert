//! Passive desktop metadata collection. Notification text is never retained.

use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;
#[cfg(any(windows, test))]
mod windows_state;

const LOCK_FRESHNESS: Duration = Duration::from_secs(1);
const MEDIA_FRESHNESS: Duration = Duration::from_secs(3);
const NOTIFICATION_CAPACITY: usize = 64;
type Shared = Arc<Mutex<SharedState>>;

pub struct Notification {
    pub id: u32,
    pub generation: u64,
    pub application: String,
    pub urgency: u8,
    pub received: Instant,
}

pub enum NotificationEvent {
    Raised(Notification),
    Closed {
        generation: u64,
        id: u32,
        reason: u32,
    },
}

#[derive(Clone)]
pub struct DesktopState {
    pub notification_status: String,
    pub notification_generation: u64,
    pub media_status: String,
    pub lock_status: String,
    pub locked: Option<bool>,
    pub playing: Vec<String>,
}

struct SharedState {
    desktop: DesktopState,
    lock_checked: Option<Instant>,
    media_checked: Option<Instant>,
    #[cfg(windows)]
    permission_pending: bool,
}

impl SharedState {
    fn new() -> Self {
        Self {
            desktop: DesktopState {
                notification_status: "Connecting to notification monitor".into(),
                notification_generation: 0,
                media_status: "Connecting to media services".into(),
                lock_status: "Lock state unknown; lighting inhibited".into(),
                locked: None,
                playing: Vec::new(),
            },
            lock_checked: None,
            media_checked: None,
            #[cfg(windows)]
            permission_pending: false,
        }
    }

    fn snapshot(&self, now: Instant) -> DesktopState {
        let mut desktop = self.desktop.clone();
        // Fail closed even if the worker dies or is starved rather than reporting a stale unlock.
        if self
            .lock_checked
            .is_none_or(|checked| now.saturating_duration_since(checked) > LOCK_FRESHNESS)
        {
            desktop.locked = None;
            desktop.lock_status = "Lock state unknown or stale; lighting inhibited".into();
        }
        if self
            .media_checked
            .is_some_and(|checked| now.saturating_duration_since(checked) > MEDIA_FRESHNESS)
        {
            desktop.playing.clear();
            desktop.media_status = "Media state stale; waiting for desktop services".into();
        }
        desktop
    }
}

pub struct DesktopMonitor {
    shared: Shared,
    notifications: mpsc::Receiver<NotificationEvent>,
    shutdown: Option<oneshot::Sender<()>>,
    finished: mpsc::Receiver<()>,
    worker: Option<thread::JoinHandle<()>>,
}

/// Foreground GUI-owned Windows permission request. Keep this outside shared runtime state:
/// its COM apartment must be initialized, polled by Windows, and released on the GUI thread.
#[cfg(windows)]
#[derive(Default)]
pub struct NotificationPermission {
    pending: Option<windows::PermissionRequest>,
}

#[cfg(windows)]
impl NotificationPermission {
    /// Call directly from the foreground GUI's button handler. Returns without waiting for
    /// consent; the monitor's snapshot reports the outcome. Never called during startup.
    pub fn request_access(&mut self, monitor: &DesktopMonitor) {
        windows::request_access(&monitor.shared, &mut self.pending);
    }
}

impl DesktopMonitor {
    /// Starts one worker thread. Desktop discovery never blocks the GUI thread or prompts for access.
    pub fn start() -> anyhow::Result<Self> {
        let shared = Arc::new(Mutex::new(SharedState::new()));
        // Queue loss advances the generation: persistent lights cannot survive a dropped close.
        let (notification_tx, notifications) = mpsc::sync_channel(NOTIFICATION_CAPACITY);
        let (shutdown, stop) = oneshot::channel();
        let (done, finished) = mpsc::sync_channel(1);
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("ledalert-desktop".into())
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => {
                        runtime.block_on(async {
                            tokio::select! {
                                biased;
                                _ = stop => {},
                                _ = run(&worker_shared, &notification_tx) => {},
                            }
                        });
                        runtime.shutdown_timeout(Duration::from_millis(100));
                    }
                    Err(_) => {
                        notification_status(&worker_shared, "Notification worker unavailable");
                        session_unavailable(&worker_shared, "Desktop worker unavailable");
                    }
                }
                let _ = done.try_send(());
            })?;
        Ok(Self {
            shared,
            notifications,
            shutdown: Some(shutdown),
            finished,
            worker: Some(worker),
        })
    }

    pub fn snapshot(&self) -> DesktopState {
        self.shared
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .snapshot(Instant::now())
    }

    pub fn notification_generation(&self) -> u64 {
        self.shared
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .desktop
            .notification_generation
    }

    pub fn try_notification(&self) -> Option<NotificationEvent> {
        self.notifications.try_recv().ok()
    }
}

impl Drop for DesktopMonitor {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        // Cancellation drops in-flight operations. Never wait indefinitely for desktop services.
        let _ = self.finished.recv_timeout(Duration::from_millis(500));
        if let Some(worker) = self.worker.take()
            && worker.is_finished()
        {
            let _ = worker.join();
        }
    }
}

async fn run(shared: &Shared, notifications: &mpsc::SyncSender<NotificationEvent>) {
    #[cfg(target_os = "linux")]
    linux::run(shared, notifications).await;
    #[cfg(windows)]
    windows::run(shared, notifications).await;
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = notifications;
        notification_status(shared, "Desktop notifications unsupported on this platform");
        session_unavailable(shared, "Desktop integration unsupported on this platform");
    }
}

fn notification_status(shared: &Shared, status: &str) {
    let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
    if state.desktop.notification_status != status {
        state.desktop.notification_status = status.into();
    }
}

fn notification_discontinuity(shared: &Shared, status: &str) -> u64 {
    let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
    state.desktop.notification_generation = state.desktop.notification_generation.wrapping_add(1);
    state.desktop.notification_status = status.into();
    state.desktop.notification_generation
}

fn session_unavailable(shared: &Shared, status: &str) {
    let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
    state.desktop.locked = None;
    state.desktop.playing.clear();
    state.desktop.lock_status = format!("{status}; lighting inhibited");
    state.desktop.media_status = status.into();
    state.lock_checked = None;
    state.media_checked = None;
}
