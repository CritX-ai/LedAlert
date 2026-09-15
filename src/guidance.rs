//! Runtime-only, target-bound strip identification. This is not saved editor state.
use crate::{config::DeviceConfig, wled::OutputState};
use anyhow::{Result, ensure};
use std::time::{Duration, Instant};

pub const LEASE_DURATION: Duration = Duration::from_secs(120);
const PRODUCER_LIMIT: Duration = Duration::from_millis(500);
const ACQUISITION_LIMIT: Duration = Duration::from_secs(2);
const IDENTIFY_COLOR: [u8; 3] = [25, 25, 25];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Stopped,
    Navigation,
    Locked,
    Quiet,
    TargetChanged,
    InvalidSelection,
    Expired,
    ProducerStalled,
    OutputFailed,
    History,
    Closed,
}

impl StopReason {
    pub fn message(self) -> &'static str {
        match self {
            Self::Stopped => "Guidance stopped; releasing control to WLED.",
            Self::Navigation => "Guidance stopped when leaving strip setup.",
            Self::Locked => {
                "Guidance stopped: the session is locked or lock status is unavailable."
            }
            Self::Quiet => "Guidance stopped by quiet mode.",
            Self::TargetChanged => "The device changed. Connect and request guidance again.",
            Self::InvalidSelection => "Guidance stopped: select a valid LED area.",
            Self::Expired => "Guidance time is up. Request another session to continue.",
            Self::ProducerStalled => {
                "Guidance stopped because the editor paused. Request control again."
            }
            Self::OutputFailed => {
                "Guidance stopped: check the connection and request control again."
            }
            Self::History => "Guidance stopped by undo or redo.",
            Self::Closed => "Guidance stopped while closing the editor.",
        }
    }
}

struct Lease {
    device: DeviceConfig,
    started: Instant,
    last_frame: Instant,
    saw_sending: bool,
    failure_epoch: u64,
}

#[derive(Default)]
pub struct Guidance {
    lease: Option<Lease>,
    pixels: Vec<[u8; 3]>,
    reason: Option<StopReason>,
}

impl Guidance {
    /// Call only in response to the explicit in-app consent action.
    /// A new call starts a new bounded grant; no implicit renewal exists.
    pub fn begin(
        &mut self,
        device: DeviceConfig,
        unlocked: bool,
        output: &OutputState,
        now: Instant,
    ) -> Result<()> {
        self.cancel(StopReason::Stopped);
        device.validate()?;
        ensure!(
            unlocked,
            "An unlocked session is required for live guidance"
        );
        self.pixels.resize(device.led_count, [0; 3]);
        self.pixels.fill([0; 3]);
        self.lease = Some(Lease {
            device,
            started: now,
            last_frame: now,
            saw_sending: false,
            failure_epoch: output.failure_epoch,
        });
        self.reason = None;
        Ok(())
    }

    pub fn active(&self) -> bool {
        self.lease.is_some()
    }
    pub fn reason(&self) -> Option<StopReason> {
        self.reason
    }
    pub fn frame(&self) -> &[[u8; 3]] {
        &self.pixels
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        self.lease.as_ref().map_or(Duration::ZERO, |lease| {
            LEASE_DURATION.saturating_sub(now.saturating_duration_since(lease.started))
        })
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.lease
            .as_ref()
            .map(|lease| lease.started + LEASE_DURATION)
    }

    pub fn cancel(&mut self, reason: StopReason) {
        if self.lease.take().is_some() {
            self.pixels.fill([0; 3]);
            self.reason = Some(reason);
        }
    }

    /// Returns whether this frame may be submitted. Every rejection consumes the
    /// lease; later healthy input cannot silently rearm it. The worker separately
    /// enforces transport ownership, health, freshness and local-address bounds.
    pub fn tick(
        &mut self,
        device: DeviceConfig,
        selection: Option<(usize, usize)>,
        unlocked: bool,
        quiet: bool,
        output: &OutputState,
        now: Instant,
    ) -> bool {
        let Some(lease) = &mut self.lease else {
            return false;
        };
        let elapsed = now.saturating_duration_since(lease.started);
        let failure = if device != lease.device {
            Some(StopReason::TargetChanged)
        } else if !unlocked {
            Some(StopReason::Locked)
        } else if quiet {
            Some(StopReason::Quiet)
        } else if elapsed >= LEASE_DURATION {
            Some(StopReason::Expired)
        } else if now < lease.last_frame || now.duration_since(lease.last_frame) >= PRODUCER_LIMIT {
            Some(StopReason::ProducerStalled)
        } else if selection.is_none_or(|(first, last)| first > last || last >= device.led_count) {
            Some(StopReason::InvalidSelection)
        } else if output.failure_epoch != lease.failure_epoch
            || (lease.saw_sending && (output.error.is_some() || !output.active))
            || (elapsed >= ACQUISITION_LIMIT && !output.active)
        {
            Some(StopReason::OutputFailed)
        } else {
            None
        };
        if let Some(reason) = failure {
            self.cancel(reason);
            return false;
        }
        lease.last_frame = now;
        lease.saw_sending |= output.active;
        self.pixels.fill([0; 3]);
        let (first, last) = selection.expect("selection checked above");
        self.pixels[first..=last].fill(IDENTIFY_COLOR);
        true
    }
}
