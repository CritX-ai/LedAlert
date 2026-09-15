//! Read-only KScreen discovery and editable, non-physical desktop-layout imports.

use std::collections::HashSet;

use anyhow::{Context, Result, ensure};
use serde_json::Value;

use crate::config::{Config, MAX_SCREENS, Point, Screen};

const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_OUTPUTS: usize = 128;
const MAX_LABEL_BYTES: usize = 128;
const MAX_DIMENSION: f32 = 32_768.0;
const MAX_COORDINATE: f32 = 131_072.0;
const ROOM_FRACTION: f32 = 0.65;

/// Logical desktop coordinates, not physical monitor measurements.
#[derive(Clone, Debug, PartialEq)]
pub struct DetectedDisplay {
    pub connector: String,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub primary: bool,
}

/// Query KDE's KScreen backend without changing any display settings.
///
/// This is blocking: callers must use a worker thread. The child has a three-second
/// deadline and a one-MiB stdout limit; stderr is discarded rather than accumulated.
#[cfg(unix)]
pub fn discover() -> Result<Vec<DetectedDisplay>> {
    use std::{
        io::{ErrorKind, Read},
        os::{fd::OwnedFd, unix::net::UnixStream},
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    struct ReapOnDrop {
        child: Child,
        reaped: bool,
    }
    impl Drop for ReapOnDrop {
        fn drop(&mut self) {
            if !self.reaped {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
    }

    let deadline = Instant::now() + Duration::from_secs(3);
    // A nonblocking socket avoids either blocking on a pipe or leaving a reader
    // thread behind if a child keeps stdout open. Only the parent end is nonblocking.
    let (mut stdout, writer) = UnixStream::pair().context("Open KScreen output channel")?;
    stdout
        .set_nonblocking(true)
        .context("Set KScreen output channel nonblocking")?;
    let writer: OwnedFd = writer.into();
    let child = Command::new("kscreen-doctor")
        .arg("-j")
        .stdin(Stdio::null())
        .stdout(Stdio::from(writer))
        .stderr(Stdio::null())
        .spawn()
        .context("KDE display discovery requires an available kscreen-doctor executable")?;
    let mut child = ReapOnDrop {
        child,
        reaped: false,
    };
    let mut bytes = Vec::with_capacity(8192);
    let mut buffer = [0_u8; 8192];
    let mut eof = false;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(
            !remaining.is_zero(),
            "KScreen discovery exceeded three seconds"
        );
        if !eof {
            // Read at most one extra byte to distinguish an exact-limit response
            // from overflow; never retain more than MAX_OUTPUT_BYTES.
            let limit = buffer.len().min(MAX_OUTPUT_BYTES - bytes.len() + 1);
            match stdout.read(&mut buffer[..limit]) {
                Ok(0) => eof = true,
                Ok(count) => {
                    ensure!(
                        count <= MAX_OUTPUT_BYTES - bytes.len(),
                        "KScreen output exceeds one MiB"
                    );
                    bytes.extend_from_slice(&buffer[..count]);
                    continue;
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) => return Err(error).context("Read KScreen output"),
            }
        }
        if !child.reaped
            && let Some(status) = child
                .child
                .try_wait()
                .context("Wait for KScreen discovery")?
        {
            child.reaped = true;
            ensure!(
                status.success(),
                "KScreen discovery failed ({status}); a working KDE display session is required"
            );
        }
        if eof && child.reaped {
            return parse_kscreen(&bytes);
        }
        thread::sleep(
            Duration::from_millis(5).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

#[cfg(not(unix))]
pub fn discover() -> Result<Vec<DetectedDisplay>> {
    anyhow::bail!("Display discovery supports KDE on Unix with kscreen-doctor only")
}

/// Parse the JSON emitted by `kscreen-doctor -j`. Inactive outputs are ignored;
/// malformed active geometry fails the whole snapshot instead of inventing sizes.
pub fn parse_kscreen(bytes: &[u8]) -> Result<Vec<DetectedDisplay>> {
    ensure!(
        bytes.len() <= MAX_OUTPUT_BYTES,
        "KScreen output exceeds one MiB"
    );
    let root: Value = serde_json::from_slice(bytes).context("Invalid KScreen JSON")?;
    let outputs = root
        .get("outputs")
        .and_then(Value::as_array)
        .context("KScreen JSON must contain an outputs array")?;
    ensure!(outputs.len() <= MAX_OUTPUTS, "Too many KScreen outputs");
    // Older KScreen backends advertise PerOutputScaling as bit 2. Newer KDE
    // versions have removed this field along with the non-per-output backend.
    let per_output_scaling = match root.get("features") {
        Some(value) => value.as_u64().context("Invalid KScreen feature flags")? & 4 != 0,
        None => true,
    };
    let mut displays = Vec::new();
    for output in outputs {
        let connected = output
            .get("connected")
            .and_then(Value::as_bool)
            .context("KScreen output has no connected state")?;
        if !connected {
            continue;
        }
        let enabled = output
            .get("enabled")
            .and_then(Value::as_bool)
            .context("Connected KScreen output has no enabled state")?;
        if !enabled {
            continue;
        }
        ensure!(
            displays.len() < MAX_SCREENS,
            "At most {MAX_SCREENS} active displays are supported"
        );
        let connector = output
            .get("name")
            .and_then(Value::as_str)
            .context("Active KScreen output has no connector name")?;
        validate_label(connector, "Display connector")?;
        let rotation = output
            .get("rotation")
            .and_then(Value::as_u64)
            .context("Active KScreen output has no rotation")?;
        ensure!(
            matches!(rotation, 1 | 2 | 4 | 8 | 16 | 32 | 64 | 128),
            "Unsupported KScreen rotation for {connector}"
        );
        let scale = number(output, "scale")?;
        ensure!(
            (0.25..=8.0).contains(&scale),
            "Invalid KScreen scale for {connector}"
        );
        let pos = output
            .get("pos")
            .context("Active KScreen output has no position")?;
        let size = output
            .get("size")
            .context("Active KScreen output has no size")?;
        let width = number(size, "width")?;
        let height = number(size, "height")?;
        ensure!(
            (1.0..=MAX_DIMENSION).contains(&width) && (1.0..=MAX_DIMENSION).contains(&height),
            "Invalid KScreen pixel dimensions for {connector}"
        );
        // WaylandOutputDevice::updateKScreenOutput stores current-mode size AFTER
        // rotation in output.size. Config::logicalSizeForOutput divides by scale.
        // Transposing this serialized size again would undo portrait orientation.
        // https://github.com/KDE/libkscreen/blob/Plasma/6.4/backends/kwayland/waylandoutputdevice.cpp
        // https://github.com/KDE/libkscreen/blob/Plasma/6.4/src/config.cpp
        let divisor = if per_output_scaling { scale } else { 1.0 };
        let primary = match output.get("priority") {
            Some(priority) => priority.as_u64().context("Invalid KScreen priority")? == 1,
            None => match output.get("primary") {
                Some(primary) => primary.as_bool().context("Invalid KScreen primary state")?,
                None => false,
            },
        };
        displays.push(DetectedDisplay {
            connector: connector.to_owned(),
            // KScreen's JSON serializer exposes the connector, not EDID model names.
            name: connector.to_owned(),
            x: number(pos, "x")?,
            y: number(pos, "y")?,
            width: width / divisor,
            height: height / divisor,
            primary,
        });
    }
    displays.sort_by(|a, b| {
        b.primary
            .cmp(&a.primary)
            .then_with(|| a.x.total_cmp(&b.x))
            .then_with(|| a.y.total_cmp(&b.y))
    });
    validate_displays(&displays)?;
    Ok(displays)
}

/// Import a centered front-plane visual starting arrangement, not physical measurements.
///
/// Existing connector identities retain their names, coordinates, widths, angles,
/// and rule references; only their aspect ratios are refreshed. Missing connectors
/// are not removed. Errors leave the entire configuration unchanged. The returned
/// count includes a newly detected display that replaces the pristine example.
pub fn import_displays(
    config: &mut Config,
    displays: &[DetectedDisplay],
    replace_example: bool,
) -> Result<usize> {
    update_displays(config, displays, replace_example, false)
}

/// Explicitly apply desktop geometry to detected connectors, importing any missing ones.
///
/// Names, IDs, connectors and rules are retained, as are undetected and manual
/// screens. Unlike ordinary discovery refresh, this resets matching screens'
/// width, position and angle. Failure leaves the configuration unchanged.
pub fn arrange_displays(config: &mut Config, displays: &[DetectedDisplay]) -> Result<usize> {
    update_displays(config, displays, false, true)?;
    Ok(displays.len())
}

fn update_displays(
    config: &mut Config,
    displays: &[DetectedDisplay],
    replace_example: bool,
    arrange: bool,
) -> Result<usize> {
    validate_displays(displays)?;
    if displays.is_empty() {
        return Ok(0);
    }
    config
        .validate()
        .context("Fix the room configuration before importing displays")?;
    let mut known = HashSet::new();
    for screen in &config.room.screens {
        if let Some(connector) = &screen.connector {
            ensure!(
                known.insert(connector.as_str()),
                "Saved screens have duplicate display connectors"
            );
        }
    }
    let added = displays
        .iter()
        .filter(|display| !known.contains(display.connector.as_str()))
        .count();
    let replace = replace_example && is_pristine_example(config);
    let final_count = config.room.screens.len() + added - usize::from(replace);
    ensure!(
        final_count <= MAX_SCREENS,
        "Import would exceed {MAX_SCREENS} room screens"
    );

    let mut candidate = config.clone();
    let mut replacement_id = if replace {
        let id = candidate.room.screens[0].id;
        candidate.room.screens.clear();
        Some(id)
    } else {
        None
    };
    let (mut min_x, mut min_y, mut max_x, mut max_y, mut widest) = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
        0.0_f32,
    );
    for display in displays {
        min_x = min_x.min(display.x);
        min_y = min_y.min(display.y);
        max_x = max_x.max(display.x + display.width);
        max_y = max_y.max(display.y + display.height);
        widest = widest.max(display.width);
    }
    let scale = (config.room.width * ROOM_FRACTION / (max_x - min_x))
        .min(config.room.height * ROOM_FRACTION / (max_y - min_y))
        .min(5.0 / widest);
    let origin_x = (config.room.width - (max_x - min_x) * scale) / 2.0;
    let origin_z = (config.room.height - (max_y - min_y) * scale) / 2.0;
    for display in displays {
        let aspect_ratio = display.width / display.height;
        let existing = candidate
            .room
            .screens
            .iter()
            .position(|screen| screen.connector.as_deref() == Some(display.connector.as_str()));
        if let Some(index) = existing {
            candidate.room.screens[index].aspect_ratio = aspect_ratio;
            if !arrange {
                continue;
            }
        }
        let width = display.width * scale;
        ensure!(
            (0.1..=5.0).contains(&width),
            "Desktop layout cannot fit this room proportionally with valid screen widths"
        );
        let position = Point {
            x: origin_x + (display.x - min_x + display.width / 2.0) * scale,
            y: config.room.depth * 0.35,
            z: origin_z + (max_y - display.y - display.height / 2.0) * scale,
        };
        if let Some(index) = existing {
            let screen = &mut candidate.room.screens[index];
            screen.width = width;
            screen.position = position;
            screen.angle = 0.0;
            continue;
        }
        let id = match replacement_id.take() {
            Some(id) => id,
            None => (1..=MAX_SCREENS as u32 + 1)
                .find(|id| candidate.room.screens.iter().all(|screen| screen.id != *id))
                .context("No available screen ID")?,
        };
        candidate.room.screens.push(Screen {
            id,
            name: display.name.clone(),
            connector: Some(display.connector.clone()),
            aspect_ratio,
            position,
            width,
            angle: 0.0,
        });
    }
    candidate
        .validate()
        .context("Imported display layout is not valid for this room")?;
    *config = candidate;
    Ok(added)
}

fn is_pristine_example(config: &Config) -> bool {
    if config.room.screens.len() != 1 || config.rules.len() != 1 {
        return false;
    }
    let mut example = Config::default();
    if config.room.screens == example.room.screens && config.rules == example.rules {
        return true;
    }
    // First-use room shaping scales geometry before asynchronous discovery can
    // finish. Recognize that same untouched example, not arbitrary saved screens.
    let ratios = (
        config.room.width / example.room.width,
        config.room.depth / example.room.depth,
        config.room.height / example.room.height,
    );
    let expected = &mut example.room.screens[0];
    let actual = &config.room.screens[0];
    let scaled_position = Point {
        x: expected.position.x * ratios.0,
        y: expected.position.y * ratios.1,
        z: expected.position.z * ratios.2,
    };
    let scaled_width = (expected.width * ratios.0).clamp(0.1, 5.0);
    let scaled_spread = (example.rules[0].spread * ratios.0).clamp(0.1, 20.0);
    // Allow only round-off from repeated proportional edits, not user geometry.
    let same = |a: f32, b: f32| (a - b).abs() <= 8.0 * f32::EPSILON * a.abs().max(b.abs()).max(1.0);
    if !same(actual.position.x, scaled_position.x)
        || !same(actual.position.y, scaled_position.y)
        || !same(actual.position.z, scaled_position.z)
        || !same(actual.width, scaled_width)
        || !same(config.rules[0].spread, scaled_spread)
    {
        return false;
    }
    expected.position = actual.position;
    expected.width = actual.width;
    example.rules[0].spread = config.rules[0].spread;
    config.room.screens == example.room.screens && config.rules == example.rules
}

fn number(object: &Value, field: &str) -> Result<f32> {
    let value = object
        .get(field)
        .and_then(Value::as_f64)
        .with_context(|| format!("KScreen {field} must be a number"))?;
    ensure!(
        value.is_finite() && value.abs() <= f32::MAX as f64,
        "KScreen {field} must be finite"
    );
    Ok(value as f32)
}

fn validate_label(label: &str, kind: &str) -> Result<()> {
    ensure!(
        !label.is_empty()
            && label == label.trim()
            && label.len() <= MAX_LABEL_BYTES
            && !label.chars().any(|c| c.is_control()
                || matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')),
        "{kind} must be 1–{MAX_LABEL_BYTES} bytes without surrounding spaces or control characters"
    );
    Ok(())
}

fn validate_displays(displays: &[DetectedDisplay]) -> Result<()> {
    ensure!(
        displays.len() <= MAX_SCREENS,
        "At most {MAX_SCREENS} active displays are supported"
    );
    let mut connectors = HashSet::with_capacity(displays.len());
    for display in displays {
        validate_label(&display.connector, "Display connector")?;
        validate_label(&display.name, "Display name")?;
        ensure!(
            connectors.insert(display.connector.as_str()),
            "Duplicate detected display connector"
        );
        ensure!(
            display.x.is_finite()
                && display.x.abs() <= MAX_COORDINATE
                && display.y.is_finite()
                && display.y.abs() <= MAX_COORDINATE,
            "Display coordinates exceed the supported desktop bounds"
        );
        ensure!(
            display.width.is_finite()
                && (1.0..=MAX_DIMENSION).contains(&display.width)
                && display.height.is_finite()
                && (1.0..=MAX_DIMENSION).contains(&display.height)
                && (0.1..=10.0).contains(&(display.width / display.height)),
            "Display dimensions or aspect ratio are outside supported bounds"
        );
    }
    Ok(())
}
