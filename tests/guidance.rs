use ledalert::{
    config::DeviceConfig,
    guidance::{Guidance, LEASE_DURATION, StopReason},
    wled::OutputState,
};
use std::time::{Duration, Instant};

fn output(active: bool) -> OutputState {
    OutputState {
        status: String::new(),
        sent_frames: u64::from(active),
        active,
        error: None,
        failure_epoch: 0,
    }
}
fn device() -> DeviceConfig {
    DeviceConfig {
        led_count: 13,
        address: std::net::Ipv4Addr::new(127, 0, 0, 2),
    }
}

#[test]
fn consent_is_required_and_identification_overrides_only_the_selected_leds() {
    let now = Instant::now();
    let mut guide = Guidance::default();
    assert!(!guide.tick(device(), Some((3, 7)), true, false, &output(false), now));
    assert!(guide.begin(device(), false, &output(false), now).is_err());
    assert!(!guide.active());
    guide.begin(device(), true, &output(false), now).unwrap();
    assert!(guide.tick(device(), Some((3, 7)), true, false, &output(false), now));
    assert_eq!(
        guide.frame(),
        &[
            [0; 3], [0; 3], [0; 3], [25; 3], [25; 3], [25; 3], [25; 3], [25; 3], [0; 3], [0; 3],
            [0; 3], [0; 3], [0; 3]
        ]
    );
    assert!(guide.tick(
        device(),
        Some((12, 12)),
        true,
        false,
        &output(true),
        now + Duration::from_millis(33)
    ));
    assert!(guide.frame()[..12].iter().all(|pixel| *pixel == [0; 3]));
    assert_eq!(guide.frame()[12], [25; 3]);
}

#[test]
fn safety_failures_consume_permission_and_healthy_input_does_not_rearm() {
    let now = Instant::now();
    for (changed, selection, unlocked, quiet, reason) in [
        (
            DeviceConfig {
                led_count: 14,
                ..device()
            },
            Some((0, 1)),
            true,
            false,
            StopReason::TargetChanged,
        ),
        (
            DeviceConfig {
                address: std::net::Ipv4Addr::LOCALHOST,
                ..device()
            },
            Some((0, 1)),
            true,
            false,
            StopReason::TargetChanged,
        ),
        (device(), Some((0, 1)), false, false, StopReason::Locked),
        (device(), Some((0, 1)), true, true, StopReason::Quiet),
        (
            device(),
            Some((0, 13)),
            true,
            false,
            StopReason::InvalidSelection,
        ),
        (
            device(),
            Some((7, 2)),
            true,
            false,
            StopReason::InvalidSelection,
        ),
        (device(), None, true, false, StopReason::InvalidSelection),
    ] {
        let mut guide = Guidance::default();
        guide.begin(device(), true, &output(false), now).unwrap();
        assert!(guide.tick(device(), Some((0, 1)), true, false, &output(true), now));
        assert!(!guide.tick(
            changed,
            selection,
            unlocked,
            quiet,
            &output(true),
            now + Duration::from_millis(33)
        ));
        assert_eq!(guide.reason(), Some(reason));
        assert!(guide.frame().iter().all(|pixel| *pixel == [0; 3]));
        assert!(!guide.tick(
            device(),
            Some((0, 1)),
            true,
            false,
            &output(true),
            now + Duration::from_millis(66)
        ));
    }
}

#[test]
fn a_finite_lease_expires_despite_continuous_valid_frames_and_requires_new_consent() {
    let now = Instant::now();
    let mut guide = Guidance::default();
    guide.begin(device(), true, &output(false), now).unwrap();
    for millis in (0..LEASE_DURATION.as_millis() as u64).step_by(100) {
        assert!(guide.tick(
            device(),
            Some((0, 12)),
            true,
            false,
            &output(true),
            now + Duration::from_millis(millis)
        ));
    }
    assert!(!guide.tick(
        device(),
        Some((0, 12)),
        true,
        false,
        &output(true),
        now + LEASE_DURATION
    ));
    assert_eq!(guide.reason(), Some(StopReason::Expired));
    assert!(!guide.tick(
        device(),
        Some((0, 12)),
        true,
        false,
        &output(true),
        now + LEASE_DURATION + Duration::from_millis(33)
    ));
    guide
        .begin(device(), true, &output(false), now + LEASE_DURATION)
        .unwrap();
    assert!(guide.tick(
        device(),
        Some((4, 4)),
        true,
        false,
        &output(true),
        now + LEASE_DURATION
    ));
}

#[test]
fn producer_stalls_and_worker_failures_never_resume_without_new_consent() {
    let now = Instant::now();
    let mut guide = Guidance::default();
    guide.begin(device(), true, &output(false), now).unwrap();
    assert!(guide.tick(device(), Some((2, 4)), true, false, &output(true), now));
    assert!(!guide.tick(
        device(),
        Some((2, 4)),
        true,
        false,
        &output(true),
        now + Duration::from_millis(500)
    ));
    assert_eq!(guide.reason(), Some(StopReason::ProducerStalled));
    guide.begin(device(), true, &output(false), now).unwrap();
    assert!(guide.tick(device(), Some((2, 4)), true, false, &output(true), now));
    let failed = OutputState {
        error: Some("fixture health failure".into()),
        ..output(false)
    };
    assert!(!guide.tick(
        device(),
        Some((2, 4)),
        true,
        false,
        &failed,
        now + Duration::from_millis(33)
    ));
    assert_eq!(guide.reason(), Some(StopReason::OutputFailed));
    assert!(!guide.tick(
        device(),
        Some((2, 4)),
        true,
        false,
        &output(true),
        now + Duration::from_millis(66)
    ));
}

#[test]
fn unsuccessful_acquisition_has_a_deadline_even_with_fresh_frames() {
    let now = Instant::now();
    let mut guide = Guidance::default();
    guide.begin(device(), true, &output(false), now).unwrap();
    for millis in (0..2000).step_by(100) {
        assert!(guide.tick(
            device(),
            Some((0, 0)),
            true,
            false,
            &output(false),
            now + Duration::from_millis(millis)
        ));
    }
    assert!(!guide.tick(
        device(),
        Some((0, 0)),
        true,
        false,
        &output(false),
        now + Duration::from_secs(2)
    ));
    assert_eq!(guide.reason(), Some(StopReason::OutputFailed));
}

#[test]
fn explicit_stop_clears_the_frame_and_invalid_reconsent_cannot_retain_an_old_lease() {
    let now = Instant::now();
    let mut guide = Guidance::default();
    guide.begin(device(), true, &output(false), now).unwrap();
    assert!(guide.tick(device(), Some((0, 12)), true, false, &output(true), now));
    guide.cancel(StopReason::Navigation);
    assert!(!guide.active());
    assert!(guide.frame().iter().all(|pixel| *pixel == [0; 3]));
    assert!(!guide.tick(device(), Some((0, 12)), true, false, &output(true), now));
    guide.begin(device(), true, &output(false), now).unwrap();
    assert!(
        guide
            .begin(
                DeviceConfig {
                    led_count: 0,
                    ..device()
                },
                true,
                &output(false),
                now
            )
            .is_err()
    );
    assert!(!guide.active());
}

#[test]
fn initial_transport_failure_consumes_permission_before_any_frame_is_sent() {
    let now = Instant::now();
    let mut guide = Guidance::default();
    guide.begin(device(), true, &output(false), now).unwrap();
    let failed = OutputState {
        error: Some("initial acquisition failed".into()),
        failure_epoch: 1,
        ..output(false)
    };
    assert!(!guide.tick(
        device(),
        Some((0, 2)),
        true,
        false,
        &failed,
        now + Duration::from_millis(33)
    ));
    assert!(!guide.tick(
        device(),
        Some((0, 2)),
        true,
        false,
        &output(true),
        now + Duration::from_millis(66)
    ));
}

#[test]
fn fresh_consent_distinguishes_previous_and_new_identical_errors() {
    let now = Instant::now();
    let mut guide = Guidance::default();
    let previous = OutputState {
        error: Some("connection refused".into()),
        failure_epoch: 9,
        ..output(false)
    };
    guide.begin(device(), true, &previous, now).unwrap();
    assert!(guide.tick(device(), Some((0, 2)), true, false, &previous, now));
    let failed_again = OutputState {
        failure_epoch: 10,
        ..previous
    };
    assert!(!guide.tick(
        device(),
        Some((0, 2)),
        true,
        false,
        &failed_again,
        now + Duration::from_millis(33)
    ));
    let recovered = OutputState {
        failure_epoch: 10,
        ..output(true)
    };
    assert!(!guide.tick(
        device(),
        Some((0, 2)),
        true,
        false,
        &recovered,
        now + Duration::from_millis(66)
    ));
}
