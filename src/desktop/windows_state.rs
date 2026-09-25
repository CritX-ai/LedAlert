//! Platform-independent reconciliation for Windows snapshots. Stores identifiers, never toast text.
use super::*;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) const MAX_NOTIFICATIONS: usize = 4096;
#[cfg(windows)]
pub(super) const MAX_MEDIA_SESSIONS: usize = 32;

#[derive(PartialEq, Eq)]
pub(super) struct Metadata {
    pub id: u32,
    pub application: String,
    pub created: i64,
}

#[derive(Default)]
pub(super) struct Notifications {
    previous: HashMap<u32, Metadata>,
    connected: bool,
}

impl Notifications {
    pub fn disconnect(&mut self, shared: &Shared, status: &str) {
        if self.connected {
            notification_discontinuity(shared, status);
        } else {
            notification_status(shared, status);
        }
        self.connected = false;
        self.previous.clear();
    }

    pub fn reconcile(
        &mut self,
        records: Vec<Metadata>,
        shared: &Shared,
        output: &mpsc::SyncSender<NotificationEvent>,
        now: Instant,
        wall_now: SystemTime,
    ) {
        if records.len() > MAX_NOTIFICATIONS {
            self.disconnect(
                shared,
                "Notification snapshot too large; lighting lifecycle reset",
            );
            return;
        }
        let mut current = HashMap::with_capacity(records.len());
        for record in records {
            if current.insert(record.id, record).is_some() {
                self.disconnect(
                    shared,
                    "Notification identifiers ambiguous; lighting lifecycle reset",
                );
                return;
            }
        }
        if !self.connected {
            // First successful snapshot after startup, denied permission or a lost connection is
            // a baseline, not a batch of new toasts. Never replay the user's notification history.
            self.previous = current;
            self.connected = true;
            notification_status(shared, "Monitoring Windows notifications (metadata only)");
            return;
        }
        let generation = shared
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .desktop
            .notification_generation;
        for (&id, old) in &self.previous {
            if current.get(&id) != Some(old)
                && output
                    .try_send(NotificationEvent::Closed {
                        generation,
                        id,
                        reason: 3,
                    })
                    .is_err()
            {
                self.queue_lost(shared);
                return;
            }
        }
        for (&id, record) in &current {
            if self.previous.get(&id) == Some(record) {
                continue;
            }
            // Delayed snapshots must preserve creation age: using receipt time would replay a
            // toast created while locked/quiet as soon as the UI gate is enabled again.
            let Some(received) = notification_received(record.created, wall_now, now) else {
                continue;
            };
            if output
                .try_send(NotificationEvent::Raised(Notification {
                    generation,
                    id,
                    application: record.application.clone(),
                    urgency: 1,
                    received,
                }))
                .is_err()
            {
                self.queue_lost(shared);
                return;
            }
        }
        self.previous = current;
        notification_status(shared, "Monitoring Windows notifications (metadata only)");
    }

    fn queue_lost(&mut self, shared: &Shared) {
        // Even events enqueued before the failed send are invalidated by this generation change.
        self.disconnect(
            shared,
            "Windows notification queue overflow; lighting lifecycle reset",
        );
    }
}

const WINDOWS_EPOCH_TICKS: u64 = 116_444_736_000_000_000;

fn notification_received(created: i64, wall_now: SystemTime, now: Instant) -> Option<Instant> {
    let ticks = u64::try_from(created)
        .ok()?
        .checked_sub(WINDOWS_EPOCH_TICKS)?;
    let created_since_epoch = Duration::new(ticks / 10_000_000, (ticks % 10_000_000) as u32 * 100);
    let age = wall_now
        .duration_since(UNIX_EPOCH)
        .ok()?
        .checked_sub(created_since_epoch)?;
    // Reject future, invalid, and unrepresentably old timestamps rather than manufacturing a
    // fresh event. Clock adjustments therefore fail closed instead of bypassing the UI gate.
    now.checked_sub(age)
}

// WTSActive = 0; other connection states must not leave lighting active in a disconnected session.
// Windows 11 WTS_SESSIONSTATE_LOCK = 0, UNLOCK = 1, UNKNOWN = -1.
pub(super) fn session_locked(active: bool, flags: i32) -> Option<bool> {
    if !active {
        Some(true)
    } else {
        match flags {
            0 => Some(true),
            1 => Some(false),
            _ => None,
        }
    }
}

pub(super) fn update_lock(shared: &Shared, locked: Option<bool>, now: Instant) {
    let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
    state.desktop.locked = locked;
    state.desktop.lock_status = match locked {
        Some(false) => "Windows session unlocked",
        Some(true) => "Windows session locked or disconnected; lighting inhibited",
        None => "Windows lock state unavailable; lighting inhibited",
    }
    .into();
    state.lock_checked = Some(now);
}

pub(super) fn update_media(shared: &Shared, playing: Option<Vec<String>>, now: Instant) {
    let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
    match playing {
        Some(mut applications) => {
            applications.sort_unstable();
            applications.dedup();
            state.desktop.playing = applications;
            state.desktop.media_status = "Monitoring Windows media playback".into();
        }
        None => {
            state.desktop.playing.clear();
            state.desktop.media_status = "Windows media unavailable; retrying".into();
        }
    }
    state.media_checked = Some(now);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> Shared {
        Arc::new(Mutex::new(SharedState::new()))
    }
    fn test_wall() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(100)
    }
    fn metadata(id: u32, created: i64) -> Metadata {
        Metadata {
            id,
            created: WINDOWS_EPOCH_TICKS as i64 + 990_000_000 + created * 10_000,
            application: "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".into(),
        }
    }
    fn generation(shared: &Shared) -> u64 {
        shared
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .desktop
            .notification_generation
    }

    #[test]
    fn baselines_history_then_emits_addition_and_removal() {
        let shared = shared();
        let (tx, rx) = mpsc::sync_channel(8);
        let mut state = Notifications::default();
        let now = Instant::now();
        state.reconcile(vec![metadata(1, 10)], &shared, &tx, now, test_wall());
        assert!(rx.try_recv().is_err(), "history must never be raised");
        state.reconcile(
            vec![metadata(1, 10), metadata(2, 20)],
            &shared,
            &tx,
            now,
            test_wall(),
        );
        let NotificationEvent::Raised(event) = rx.try_recv().unwrap() else {
            panic!("expected addition")
        };
        assert_eq!((event.id, event.generation, event.urgency), (2, 0, 1));
        assert_eq!(
            event.application,
            "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"
        );
        assert_eq!(event.received, now - Duration::from_millis(980));
        state.reconcile(vec![metadata(1, 10)], &shared, &tx, now, test_wall());
        assert!(matches!(
            rx.try_recv().unwrap(),
            NotificationEvent::Closed {
                id: 2,
                generation: 0,
                reason: 3
            }
        ));
        state.reconcile(vec![metadata(1, 10)], &shared, &tx, now, test_wall());
        assert!(
            rx.try_recv().is_err(),
            "unchanged snapshots are not new notifications"
        );
    }

    #[test]
    fn reused_id_closes_old_lifecycle_before_raising_replacement() {
        let shared = shared();
        let (tx, rx) = mpsc::sync_channel(8);
        let mut state = Notifications::default();
        let now = Instant::now();
        state.reconcile(vec![metadata(1, 10)], &shared, &tx, now, test_wall());
        state.reconcile(vec![metadata(1, 20)], &shared, &tx, now, test_wall());
        assert!(matches!(
            rx.try_recv().unwrap(),
            NotificationEvent::Closed { id: 1, .. }
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            NotificationEvent::Raised(Notification {
                id: 1,
                generation: 0,
                ..
            })
        ));
    }

    #[test]
    fn revocation_and_reconnect_invalidate_lights_without_replaying_history() {
        let shared = shared();
        let (tx, rx) = mpsc::sync_channel(8);
        let mut state = Notifications::default();
        let now = Instant::now();
        state.reconcile(vec![], &shared, &tx, now, test_wall());
        state.reconcile(vec![metadata(1, 10)], &shared, &tx, now, test_wall());
        let old = generation(&shared);
        state.disconnect(&shared, "Permission revoked");
        let new = generation(&shared);
        assert_ne!(old, new);
        state.disconnect(&shared, "Permission denied");
        assert_eq!(generation(&shared), new);
        assert!(
            matches!(rx.try_recv().unwrap(), NotificationEvent::Raised(Notification { generation, .. }) if generation == old)
        );
        state.reconcile(
            vec![metadata(1, 10), metadata(2, 20)],
            &shared,
            &tx,
            now,
            test_wall(),
        );
        assert!(
            rx.try_recv().is_err(),
            "reconnected history must be suppressed"
        );
        state.reconcile(
            vec![metadata(1, 10), metadata(2, 20), metadata(3, 30)],
            &shared,
            &tx,
            now,
            test_wall(),
        );
        assert!(
            matches!(rx.try_recv().unwrap(), NotificationEvent::Raised(Notification { id: 3, generation, .. }) if generation == new)
        );
    }

    #[test]
    fn dropped_close_invalidates_queued_events_and_rebaselines() {
        let shared = shared();
        let (tx, rx) = mpsc::sync_channel(1);
        let mut state = Notifications::default();
        let now = Instant::now();
        state.reconcile(vec![], &shared, &tx, now, test_wall());
        state.reconcile(vec![metadata(1, 10)], &shared, &tx, now, test_wall());
        state.reconcile(vec![], &shared, &tx, now, test_wall()); // Close cannot fit.
        assert_eq!(generation(&shared), 1);
        assert!(matches!(
            rx.try_recv().unwrap(),
            NotificationEvent::Raised(Notification { generation: 0, .. })
        ));
        state.reconcile(vec![metadata(2, 20)], &shared, &tx, now, test_wall());
        assert!(rx.try_recv().is_err());
        state.reconcile(
            vec![metadata(2, 20), metadata(3, 30)],
            &shared,
            &tx,
            now,
            test_wall(),
        );
        assert!(matches!(
            rx.try_recv().unwrap(),
            NotificationEvent::Raised(Notification {
                id: 3,
                generation: 1,
                ..
            })
        ));
    }

    #[test]
    fn ambiguous_or_oversized_snapshot_cannot_leave_lights_alive() {
        let shared = shared();
        let (tx, rx) = mpsc::sync_channel(8);
        let mut state = Notifications::default();
        let now = Instant::now();
        state.reconcile(vec![], &shared, &tx, now, test_wall());
        state.reconcile(
            vec![metadata(1, 10), metadata(1, 20)],
            &shared,
            &tx,
            now,
            test_wall(),
        );
        assert_eq!(generation(&shared), 1);
        assert!(rx.try_recv().is_err());
        state.reconcile(vec![], &shared, &tx, now, test_wall());
        state.reconcile(
            (0..=MAX_NOTIFICATIONS as u32)
                .map(|id| metadata(id, 10))
                .collect(),
            &shared,
            &tx,
            now,
            test_wall(),
        );
        assert_eq!(generation(&shared), 2);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn lock_is_fail_closed_for_unknown_disconnected_and_stale_sessions() {
        assert_eq!(session_locked(true, 1), Some(false));
        assert_eq!(session_locked(true, 0), Some(true));
        assert_eq!(session_locked(true, -1), None);
        assert_eq!(session_locked(false, 1), Some(true));
        let shared = shared();
        let now = Instant::now();
        update_lock(&shared, Some(false), now);
        assert_eq!(
            shared
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .snapshot(now + LOCK_FRESHNESS)
                .locked,
            Some(false)
        );
        assert_eq!(
            shared
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .snapshot(now + LOCK_FRESHNESS + Duration::from_millis(1))
                .locked,
            None
        );
        update_lock(&shared, None, now);
        assert_eq!(
            shared
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .snapshot(now)
                .locked,
            None
        );
    }

    #[test]
    fn media_is_deduplicated_and_clears_on_failure_or_expiry() {
        let shared = shared();
        let now = Instant::now();
        update_media(
            &shared,
            Some(vec!["Spotify.exe".into(), "Spotify.exe".into()]),
            now,
        );
        assert_eq!(
            shared
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .snapshot(now)
                .playing,
            ["Spotify.exe"]
        );
        assert!(
            shared
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .snapshot(now + MEDIA_FRESHNESS + Duration::from_millis(1))
                .playing
                .is_empty()
        );
        update_media(&shared, None, now);
        assert!(
            shared
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .snapshot(now)
                .playing
                .is_empty()
        );
        update_media(&shared, Some(vec![]), now);
        assert!(
            shared
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .snapshot(now)
                .playing
                .is_empty()
        );
    }

    #[test]
    fn delayed_snapshot_preserves_creation_time_across_unlock_gate() {
        let shared = shared();
        let (tx, rx) = mpsc::sync_channel(8);
        let mut state = Notifications::default();
        let now = Instant::now();
        let gate = now - Duration::from_millis(500);
        state.reconcile(vec![], &shared, &tx, now, test_wall());
        // All three become visible in one delayed snapshot after unlock/Quiet off.
        state.reconcile(
            vec![metadata(1, 250), metadata(2, 500), metadata(3, 750)],
            &shared,
            &tx,
            now,
            test_wall(),
        );
        let received: HashMap<_, _> = rx
            .try_iter()
            .map(|event| {
                let NotificationEvent::Raised(event) = event else {
                    panic!("expected addition")
                };
                (event.id, event.received)
            })
            .collect();
        assert_eq!(received[&1], now - Duration::from_millis(750));
        assert!(
            received[&1] < gate,
            "pre-unlock toast must remain rejected by the UI gate"
        );
        assert_eq!(received[&2], gate, "gate boundary is inclusive");
        assert_eq!(received[&3], now - Duration::from_millis(250));
        assert!(received[&3] > gate, "post-unlock toast remains eligible");
        state.reconcile(
            vec![metadata(1, 250), metadata(2, 500), metadata(3, 750)],
            &shared,
            &tx,
            now + Duration::from_secs(1),
            test_wall() + Duration::from_secs(1),
        );
        assert!(
            rx.try_recv().is_err(),
            "moving clock anchors must not create replacement events"
        );
    }

    #[test]
    fn invalid_and_future_creation_times_never_become_fresh_events() {
        let now = Instant::now();
        assert_eq!(notification_received(-1, test_wall(), now), None);
        assert_eq!(
            notification_received(WINDOWS_EPOCH_TICKS as i64 - 1, test_wall(), now),
            None
        );
        assert_eq!(
            notification_received(metadata(1, 1001).created, test_wall(), now),
            None
        );
        assert_eq!(
            notification_received(metadata(1, 1000).created, test_wall(), now),
            Some(now)
        );
        assert_eq!(
            notification_received(metadata(1, 1000).created - 1, test_wall(), now),
            Some(now - Duration::from_nanos(100)),
        );
    }
}
