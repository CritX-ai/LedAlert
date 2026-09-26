//! Portable reconciliation of macOS delivery membership, never notification contents.
use super::*;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) const MAX_NOTIFICATIONS: usize = 4096;
const APPLE_EPOCH_SECONDS: u64 = 978_307_200;
const MONITORING: &str = "Monitoring macOS notification identities (read-only private schema)";

#[derive(PartialEq)]
pub(super) struct Metadata {
    pub id: i64,
    pub application: String,
    pub created: f64,
}

struct Tracked {
    metadata: Metadata,
    event_id: u32,
}

#[derive(Default)]
pub(super) struct Notifications {
    previous: HashMap<i64, Tracked>,
    connected: bool,
    next_id: u32,
}

impl Notifications {
    pub fn disconnect(&mut self, shared: &Shared, status: &str) {
        if self.connected {
            notification_discontinuity(shared, status);
        } else {
            notification_status(shared, status);
        }
        self.previous.clear();
        self.connected = false;
        self.next_id = 0;
    }

    pub fn reconcile(
        &mut self,
        records: Vec<Metadata>,
        shared: &Shared,
        output: &mpsc::SyncSender<NotificationEvent>,
        now: Instant,
        wall: SystemTime,
    ) {
        if records.len() > MAX_NOTIFICATIONS {
            self.disconnect(
                shared,
                "macOS notification snapshot exceeds its limit; lifecycle reset",
            );
            return;
        }
        let mut current = HashMap::with_capacity(records.len());
        for metadata in records {
            if metadata.id < 0
                || !valid_application(&metadata.application)
                || !metadata.created.is_finite()
                || metadata.created < 0.0
                || current.contains_key(&metadata.id)
            {
                self.disconnect(
                    shared,
                    "macOS notification metadata is ambiguous; lifecycle reset",
                );
                return;
            }
            let event_id = if let Some(previous) = self.previous.get(&metadata.id) {
                previous.event_id
            } else {
                let Some(next) = self.next_id.checked_add(1) else {
                    self.disconnect(
                        shared,
                        "macOS notification identifiers exhausted; lifecycle reset",
                    );
                    return;
                };
                self.next_id = next;
                next
            };
            current.insert(metadata.id, Tracked { metadata, event_id });
        }
        if !self.connected {
            self.previous = current;
            self.connected = true;
            notification_status(shared, MONITORING);
            return;
        }
        let generation = shared
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .desktop
            .notification_generation;
        for (&id, previous) in &self.previous {
            if current
                .get(&id)
                .is_none_or(|value| value.metadata != previous.metadata)
                && output
                    .try_send(NotificationEvent::Closed {
                        generation,
                        id: previous.event_id,
                        reason: 3,
                    })
                    .is_err()
            {
                self.disconnect(
                    shared,
                    "macOS notification queue lost events; lifecycle reset",
                );
                return;
            }
        }
        for (&id, value) in &current {
            if self
                .previous
                .get(&id)
                .is_some_and(|previous| previous.metadata == value.metadata)
            {
                continue;
            }
            let Some(received) = notification_received(value.metadata.created, wall, now) else {
                continue;
            };
            if output
                .try_send(NotificationEvent::Raised(Notification {
                    generation,
                    id: value.event_id,
                    application: value.metadata.application.clone(),
                    urgency: 1,
                    received,
                }))
                .is_err()
            {
                self.disconnect(
                    shared,
                    "macOS notification queue lost events; lifecycle reset",
                );
                return;
            }
        }
        self.previous = current;
        notification_status(shared, MONITORING);
    }
}

pub(super) fn valid_application(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn notification_received(created: f64, wall: SystemTime, now: Instant) -> Option<Instant> {
    // Preserve delivery age across unlock/Quiet boundaries, including fractional seconds.
    let date = Duration::try_from_secs_f64(created)
        .ok()?
        .checked_add(Duration::from_secs(APPLE_EPOCH_SECONDS))?;
    let age = wall.duration_since(UNIX_EPOCH).ok()?.checked_sub(date)?;
    now.checked_sub(age)
}

pub(super) fn update_lock(shared: &Shared, locked: Option<bool>, observed: Instant) {
    let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
    state.desktop.locked = locked;
    state.desktop.lock_status = match locked {
        Some(false) => "macOS console session unlocked",
        Some(true) => "macOS session locked or disconnected; lighting inhibited",
        None => "macOS lock state unavailable; lighting inhibited",
    }
    .into();
    state.lock_checked = Some(observed);
}

pub(super) fn update_media(shared: &Shared, playing: Option<Vec<String>>, observed: Instant) {
    let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
    state.desktop.media_status = if playing.is_some() {
        "Monitoring CoreAudio output activity (includes calls and silent streams)"
    } else {
        "CoreAudio output activity unavailable; retrying"
    }
    .into();
    state.desktop.playing = playing.unwrap_or_default();
    state.media_checked = Some(observed);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shared() -> Shared {
        Arc::new(Mutex::new(SharedState::new()))
    }
    fn state(shared: &Shared) -> std::sync::MutexGuard<'_, SharedState> {
        // Preserve the existing shared-runtime lock type and its poison recovery policy.
        shared.lock().unwrap_or_else(|error| error.into_inner())
    }
    fn wall() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(APPLE_EPOCH_SECONDS + 100)
    }
    fn record(id: i64, created: f64) -> Metadata {
        Metadata {
            id,
            created,
            application: "org.example.Synthetic".into(),
        }
    }

    #[test]
    fn history_is_silent_and_wide_ids_have_distinct_lifecycles() {
        let shared = shared();
        let (tx, rx) = mpsc::sync_channel(8);
        let mut reducer = Notifications::default();
        let now = Instant::now();
        reducer.reconcile(vec![record(1, 99.0)], &shared, &tx, now, wall());
        assert!(rx.try_recv().is_err());
        let wide = i64::from(u32::MAX) + 2;
        reducer.reconcile(
            vec![record(1, 99.0), record(wide, 99.5)],
            &shared,
            &tx,
            now,
            wall(),
        );
        let NotificationEvent::Raised(event) = rx.try_recv().unwrap() else {
            panic!("raise")
        };
        assert_eq!(event.received, now - Duration::from_millis(500));
        assert_eq!(event.application, "org.example.Synthetic");
        assert_ne!(event.id, 1);
        reducer.reconcile(vec![record(1, 99.0)], &shared, &tx, now, wall());
        assert!(
            matches!(rx.try_recv().unwrap(), NotificationEvent::Closed { id, generation: 0, .. } if id == event.id)
        );
    }

    #[test]
    fn replacements_close_first_and_permission_recovery_never_replays() {
        let shared = shared();
        let (tx, rx) = mpsc::sync_channel(8);
        let mut reducer = Notifications::default();
        let now = Instant::now();
        reducer.reconcile(vec![record(7, 98.0)], &shared, &tx, now, wall());
        reducer.reconcile(vec![record(7, 99.5)], &shared, &tx, now, wall());
        let NotificationEvent::Closed { id, .. } = rx.try_recv().unwrap() else {
            panic!("close")
        };
        assert!(
            matches!(rx.try_recv().unwrap(), NotificationEvent::Raised(event) if event.id == id)
        );
        reducer.disconnect(&shared, "permission unavailable");
        let generation = state(&shared).desktop.notification_generation;
        assert_eq!(generation, 1);
        reducer.disconnect(&shared, "permission still unavailable");
        assert_eq!(state(&shared).desktop.notification_generation, generation);
        reducer.reconcile(
            vec![record(7, 99.5), record(9, 99.8)],
            &shared,
            &tx,
            now,
            wall(),
        );
        assert!(rx.try_recv().is_err());
        reducer.reconcile(
            vec![record(7, 99.5), record(9, 99.8), record(10, 99.9)],
            &shared,
            &tx,
            now,
            wall(),
        );
        assert!(
            matches!(rx.try_recv().unwrap(), NotificationEvent::Raised(event) if event.generation == generation)
        );
    }

    #[test]
    fn queue_loss_invalidates_even_already_queued_events_and_rebaselines() {
        let shared = shared();
        let (tx, rx) = mpsc::sync_channel(1);
        let mut reducer = Notifications::default();
        let now = Instant::now();
        reducer.reconcile(vec![], &shared, &tx, now, wall());
        reducer.reconcile(
            vec![record(1, 99.5), record(2, 99.5)],
            &shared,
            &tx,
            now,
            wall(),
        );
        assert_eq!(state(&shared).desktop.notification_generation, 1);
        assert!(
            matches!(rx.try_recv().unwrap(), NotificationEvent::Raised(event) if event.generation == 0)
        );
        reducer.reconcile(
            vec![record(1, 99.5), record(2, 99.5)],
            &shared,
            &tx,
            now,
            wall(),
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn malformed_and_oversized_snapshots_clear_connected_lifecycles() {
        for records in [
            vec![record(1, 99.0), record(1, 99.0)],
            vec![record(1, f64::NAN)],
            vec![record(-1, 99.0)],
            (0..=MAX_NOTIFICATIONS)
                .map(|id| record(id as i64, 99.0))
                .collect(),
        ] {
            let shared = shared();
            let (tx, _) = mpsc::sync_channel(8);
            let mut reducer = Notifications::default();
            let now = Instant::now();
            reducer.reconcile(vec![], &shared, &tx, now, wall());
            reducer.reconcile(records, &shared, &tx, now, wall());
            assert_eq!(state(&shared).desktop.notification_generation, 1);
            assert!(!reducer.connected);
        }
    }

    #[test]
    fn timestamps_never_become_fresh_across_clock_or_gate_boundaries() {
        let now = Instant::now();
        assert_eq!(
            notification_received(99.25, wall(), now),
            Some(now - Duration::from_millis(750))
        );
        assert!(notification_received(100.01, wall(), now).is_none());
        assert!(notification_received(-1.0, wall(), now).is_none());
        assert!(notification_received(f64::INFINITY, wall(), now).is_none());
        assert!(notification_received(1.0, UNIX_EPOCH, now).is_none());
    }

    #[test]
    fn stale_lock_and_media_fail_closed_and_errors_clear_activity() {
        let shared = shared();
        let now = Instant::now();
        update_lock(&shared, Some(false), now);
        update_media(&shared, Some(vec!["org.example.Synthetic".into()]), now);
        let current = state(&shared);
        assert_eq!(current.snapshot(now).locked, Some(false));
        assert_eq!(
            current
                .snapshot(now + LOCK_FRESHNESS + Duration::from_nanos(1))
                .locked,
            None
        );
        assert!(
            current
                .snapshot(now + MEDIA_FRESHNESS + Duration::from_nanos(1))
                .playing
                .is_empty()
        );
        drop(current);
        update_media(&shared, None, now);
        update_lock(&shared, Some(true), now);
        assert!(state(&shared).desktop.playing.is_empty());
        assert_eq!(state(&shared).desktop.locked, Some(true));
    }
}
