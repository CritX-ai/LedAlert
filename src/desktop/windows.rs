//! Windows 11 metadata adapters. This is the only desktop module permitted to call unsafe APIs.
#![allow(unsafe_code)]

use super::windows_state::{self, MAX_MEDIA_SESSIONS, MAX_NOTIFICATIONS, Metadata, Notifications};
use super::*;
use crate::taskbar::windows::canonical_app_id;
use ::windows::{
    ApplicationModel::Package,
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager as MediaManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    },
    UI::Notifications::{
        Management::{
            UserNotificationListener as Listener, UserNotificationListenerAccessStatus as Access,
        },
        NotificationKinds,
    },
    Win32::System::{
        RemoteDesktop::{
            WTS_CURRENT_SESSION, WTSActive, WTSFreeMemory, WTSINFOEXW, WTSQuerySessionInformationW,
            WTSSessionInfoEx,
        },
        WinRT::{
            RO_INIT_MULTITHREADED, RO_INIT_SINGLETHREADED, RO_INIT_TYPE, RoInitialize,
            RoUninitialize,
        },
    },
    core::{PWSTR, RuntimeType},
};
use std::{future::IntoFuture, marker::PhantomData, rc::Rc, time::SystemTime};
use tokio::{
    sync::Notify,
    time::{sleep, timeout},
};
use windows_future::{AsyncOperationCompletedHandler, AsyncStatus, IAsyncOperation};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const MEDIA_INTERVAL: Duration = Duration::from_millis(750);
const RETRY_INTERVAL: Duration = Duration::from_secs(2);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(2);
const UNPACKAGED: &str = "Windows notifications require the installed MSIX edition; portable builds support media and lock detection only";
const NEED_PERMISSION: &str =
    "Windows notification permission not granted; use Allow Windows notifications";
const DENIED: &str = "Windows notification access denied or revoked; enable notification access for LedAlert in Windows Settings";

// Apartment ownership is thread-affine. Rc makes this guard !Send and !Sync; the current-thread
// worker and GUI keep initialization and uninitialization on the same thread.
struct Apartment(PhantomData<Rc<()>>);
impl Apartment {
    fn initialize(kind: RO_INIT_TYPE) -> ::windows::core::Result<Self> {
        // SAFETY: Only initializes the calling thread. A successful call is balanced by Drop.
        unsafe {
            RoInitialize(kind)?;
        }
        Ok(Self(PhantomData))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: This guard cannot move to another thread and exists only after successful init.
        unsafe {
            RoUninitialize();
        }
    }
}

struct Operation<T: RuntimeType + 'static> {
    operation: IAsyncOperation<T>,
    completed: bool,
}
impl<T: RuntimeType + 'static> Drop for Operation<T> {
    fn drop(&mut self) {
        if !self.completed {
            let _ = self.operation.Cancel();
        }
    }
}
async fn complete<T: RuntimeType + 'static>(operation: IAsyncOperation<T>) -> anyhow::Result<T> {
    let mut pending = Operation {
        operation,
        completed: false,
    };
    let result = timeout(OPERATION_TIMEOUT, pending.operation.clone().into_future()).await?;
    pending.completed = true;
    Ok(result?)
}

pub(super) struct PermissionRequest {
    operation: IAsyncOperation<Access>,
    // Declared after operation, so the COM object is released before apartment teardown.
    _apartment: Apartment,
}
impl Drop for PermissionRequest {
    fn drop(&mut self) {
        let _ = self.operation.Cancel();
    }
}

pub(super) fn request_access(shared: &Shared, pending: &mut Option<PermissionRequest>) {
    if pending
        .as_ref()
        .is_some_and(|request| request.operation.Status().ok() == Some(AsyncStatus::Started))
    {
        return;
    }
    // Release any previous initialization on this same UI thread before creating the next request.
    drop(pending.take());
    let apartment = match Apartment::initialize(RO_INIT_SINGLETHREADED) {
        Ok(apartment) => apartment,
        Err(_) => {
            notification_status(
                shared,
                "Notification permission must be requested from the foreground Windows GUI thread",
            );
            return;
        }
    };
    if Package::Current().is_err() {
        notification_status(shared, UNPACKAGED);
        return;
    }
    // Microsoft requires RequestAccessAsync itself (not just awaiting it) on the foreground UI
    // thread. This method is called only by the eframe consent button, never during startup.
    let operation = match Listener::Current().and_then(|listener| listener.RequestAccessAsync()) {
        Ok(operation) => operation,
        Err(_) => {
            notification_status(
                shared,
                "Windows notification permission request failed; verify MSIX capability and foreground window",
            );
            return;
        }
    };
    {
        let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
        state.permission_pending = true;
        state.desktop.notification_status = "Waiting for Windows notification permission".into();
    }
    let state = Arc::clone(shared);
    let handler = AsyncOperationCompletedHandler::new(move |operation, _| {
        let result = operation
            .as_ref()
            .and_then(|operation| operation.GetResults().ok());
        let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
        state.permission_pending = false;
        state.desktop.notification_status = match result {
            Some(Access::Allowed) => {
                "Windows notification permission granted; synchronizing without replay"
            }
            Some(Access::Denied) => DENIED,
            Some(Access::Unspecified) => NEED_PERMISSION,
            _ => "Windows notification permission request failed or cancelled",
        }
        .into();
        Ok(())
    });
    if operation.SetCompleted(&handler).is_err() {
        let _ = operation.Cancel();
        let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
        state.permission_pending = false;
        state.desktop.notification_status =
            "Windows notification permission result unavailable; retry from the GUI".into();
        return;
    }
    *pending = Some(PermissionRequest {
        operation,
        _apartment: apartment,
    });
}

pub(super) async fn run(shared: &Shared, notifications: &mpsc::SyncSender<NotificationEvent>) {
    let Ok(_apartment) = Apartment::initialize(RO_INIT_MULTITHREADED) else {
        notification_status(shared, "Windows notification runtime unavailable");
        session_unavailable(shared, "Windows desktop runtime unavailable");
        return;
    };
    tokio::join!(
        notification_loop(shared, notifications),
        lock_loop(shared),
        media_loop(shared)
    );
}

struct Subscription {
    listener: Listener,
    token: i64,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        let _ = self.listener.RemoveNotificationChanged(self.token);
    }
}

async fn notification_loop(shared: &Shared, output: &mpsc::SyncSender<NotificationEvent>) {
    if Package::Current().is_err() {
        notification_status(shared, UNPACKAGED);
        return;
    }
    let mut state = Notifications::default();
    loop {
        let Ok(listener) = Listener::Current() else {
            state.disconnect(
                shared,
                "Windows notification listener unavailable; verify MSIX capability; retrying",
            );
            sleep(RETRY_INTERVAL).await;
            continue;
        };
        let changed = Arc::new(Notify::new());
        let wake = Arc::clone(&changed);
        let handler = TypedEventHandler::new(move |_, _| {
            wake.notify_one();
            Ok(())
        });
        // Coalesced wakeups are not lifecycle events: every wake reconciles the authoritative
        // snapshot. Polling also detects silent permission revocation and missing callbacks.
        let _subscription = listener
            .NotificationChanged(&handler)
            .ok()
            .map(|token| Subscription {
                listener: listener.clone(),
                token,
            });
        loop {
            match listener.GetAccessStatus() {
                Ok(Access::Allowed) => {}
                Ok(access) => {
                    let pending = shared
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .permission_pending;
                    state.disconnect(
                        shared,
                        if pending {
                            "Waiting for Windows notification permission"
                        } else if access == Access::Denied {
                            DENIED
                        } else {
                            NEED_PERMISSION
                        },
                    );
                    sleep(POLL_INTERVAL).await;
                    continue;
                }
                Err(_) => {
                    state.disconnect(
                        shared,
                        "Windows notification access unavailable; reconnecting",
                    );
                    break;
                }
            }
            match notification_snapshot(&listener).await {
                Ok(records) => {
                    // Revocation can make GetNotificationsAsync silently return an empty list.
                    // Recheck after awaiting so such a result is not mistaken for normal removals.
                    if listener.GetAccessStatus().ok() != Some(Access::Allowed) {
                        state.disconnect(shared, DENIED);
                    } else {
                        let now = Instant::now();
                        let wall_now = SystemTime::now();
                        state.reconcile(records, shared, output, now, wall_now);
                    }
                }
                Err(_) => {
                    state.disconnect(
                        shared,
                        "Windows notification snapshot unavailable; reconnecting without replay",
                    );
                    break;
                }
            }
            tokio::select! { _ = changed.notified() => {}, _ = sleep(POLL_INTERVAL) => {} }
        }
        sleep(RETRY_INTERVAL).await;
    }
}

async fn notification_snapshot(listener: &Listener) -> anyhow::Result<Vec<Metadata>> {
    let snapshot = complete(listener.GetNotificationsAsync(NotificationKinds::Toast)?).await?;
    let count = snapshot.Size()? as usize;
    anyhow::ensure!(count <= MAX_NOTIFICATIONS, "notification snapshot limit");
    let mut records = Vec::with_capacity(count);
    for index in 0..count {
        let notification = snapshot.GetAt(index as u32)?;
        // Intentionally never call Notification(), Visual(), GetTextElements(), DisplayInfo(),
        // or any title/body/image API. Only stable application identity and lifecycle metadata.
        let application = String::from_utf16(&notification.AppInfo()?.AppUserModelId()?)?;
        if canonical_app_id(&application).is_none() {
            continue;
        }
        records.push(Metadata {
            id: notification.Id()?,
            created: notification.CreationTime()?.UniversalTime,
            application,
        });
    }
    Ok(records)
}

async fn lock_loop(shared: &Shared) {
    loop {
        windows_state::update_lock(shared, query_lock(), Instant::now());
        sleep(POLL_INTERVAL).await;
    }
}

fn query_lock() -> Option<bool> {
    let mut buffer = PWSTR::null();
    let mut size = 0;
    // SAFETY: Out-pointers refer to initialized local variables. WTS_CURRENT_SESSION queries the
    // process's own interactive/RDP session on the local server, not another user's console.
    unsafe {
        WTSQuerySessionInformationW(
            None,
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buffer,
            &mut size,
        )
        .ok()?;
    }
    struct Buffer(PWSTR);
    impl Drop for Buffer {
        fn drop(&mut self) {
            // SAFETY: WTSQuerySessionInformationW owns this allocation; release exactly once.
            unsafe {
                WTSFreeMemory(self.0.0.cast());
            }
        }
    }
    let buffer = Buffer(buffer);
    if buffer.0.is_null() || (size as usize) < std::mem::size_of::<WTSINFOEXW>() {
        return None;
    }
    // SAFETY: Query success and byte-size validation establish a complete WTSINFOEXW allocation;
    // read_unaligned does not assume stronger alignment than the API's PWSTR return type.
    let info = unsafe { buffer.0.0.cast::<WTSINFOEXW>().read_unaligned() };
    if info.Level != 1 {
        return None;
    }
    // SAFETY: Level == 1 identifies the initialized WTSInfoExLevel1 union member.
    let session = unsafe { info.Data.WTSInfoExLevel1 };
    windows_state::session_locked(session.SessionState == WTSActive, session.SessionFlags)
}

async fn media_loop(shared: &Shared) {
    loop {
        let manager = match MediaManager::RequestAsync() {
            Ok(operation) => complete(operation).await.ok(),
            Err(_) => None,
        };
        if let Some(manager) = manager {
            loop {
                match media_snapshot(&manager) {
                    Ok(playing) => {
                        windows_state::update_media(shared, Some(playing), Instant::now())
                    }
                    Err(_) => {
                        windows_state::update_media(shared, None, Instant::now());
                        break;
                    }
                }
                sleep(MEDIA_INTERVAL).await;
            }
        }
        windows_state::update_media(shared, None, Instant::now());
        sleep(RETRY_INTERVAL).await;
    }
}

fn media_snapshot(manager: &MediaManager) -> anyhow::Result<Vec<String>> {
    let sessions = manager.GetSessions()?;
    let count = sessions.Size()? as usize;
    anyhow::ensure!(count <= MAX_MEDIA_SESSIONS, "media session limit");
    let mut playing = Vec::with_capacity(count);
    for index in 0..count {
        let session = sessions.GetAt(index as u32)?;
        if session.GetPlaybackInfo()?.PlaybackStatus()? == PlaybackStatus::Playing {
            // Playback metadata, titles and timeline content are deliberately never requested.
            let application = String::from_utf16(&session.SourceAppUserModelId()?)?;
            if canonical_app_id(&application).is_some() {
                playing.push(application);
            }
        }
    }
    Ok(playing)
}
