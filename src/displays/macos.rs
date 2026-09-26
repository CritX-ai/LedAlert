//! Read-only macOS display geometry in one common desktop coordinate space.

#[cfg(target_os = "macos")]
use std::str;

#[cfg(target_os = "macos")]
use anyhow::{Context, bail};
use anyhow::{Result, ensure};

use super::{DetectedDisplay, MAX_OUTPUTS, validate_displays, validate_label};

#[cfg(target_os = "macos")]
const UUID_CAP: usize = 64;

/// One active CoreGraphics display in the common global coordinate space
/// (points, top-left origin of the main display). CoreGraphics already scales
/// each display by its own backing scale, so mixed-DPI arrangements stay in
/// one coordinate space without per-display division. `main` marks the
/// CoreGraphics main display.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayBounds {
    /// Stable CoreGraphics display UUID; this identifies saved connectors.
    pub uuid: String,
    /// Localized display name, or empty when the system provides none.
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub main: bool,
}

/// The fixed-layout record the native adapter writes. Mirrors
/// `LedDisplayBounds` in `macos_native.m`.
#[cfg(target_os = "macos")]
#[repr(C)]
struct DisplayBoundsRecord {
    uuid: [u8; UUID_CAP],
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    main: u8,
    builtin: u8,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn ledalert_displays_bounds(
        out: *mut DisplayBoundsRecord,
        capacity: usize,
        count: *mut usize,
    ) -> i32;
}

/// Query the active CoreGraphics displays without changing any display
/// setting. This is blocking: callers must use a worker thread.
#[cfg(target_os = "macos")]
pub(crate) fn discover() -> Result<Vec<DetectedDisplay>> {
    let mut records = [const {
        DisplayBoundsRecord {
            uuid: [0; UUID_CAP],
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            main: 0,
            builtin: 0,
        }
    }; MAX_OUTPUTS];
    let mut count = 0usize;
    // SAFETY: out points to `capacity` initialized records that the native
    // side fills, and count addresses a live local.
    let status =
        unsafe { ledalert_displays_bounds(records.as_mut_ptr(), records.len(), &mut count) };
    ensure!(status == 0, "CoreGraphics display enumeration failed");
    ensure!(
        count <= records.len(),
        "CoreGraphics reported more displays than the reserved buffer"
    );
    let records = &records[..count];
    let bounds = records
        .iter()
        .map(|record| {
            Ok(DisplayBounds {
                uuid: c_string(&record.uuid).context("Display identity")?,
                name: match record.builtin {
                    0 => String::new(),
                    1 => "Built-in display".to_owned(),
                    _ => bail!("Display has an invalid built-in flag"),
                },
                // An out-of-range cast becomes infinite and fails validation
                // below instead of silently wrapping around.
                x: record.x as f32,
                y: record.y as f32,
                width: record.width as f32,
                height: record.height as f32,
                main: match record.main {
                    0 => false,
                    1 => true,
                    _ => bail!("Display has an invalid main flag"),
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    from_bounds(bounds)
}

/// Convert native bounds into validated detected displays. Display UUIDs
/// identify saved connectors across reboots and renames. Exactly one main
/// display is required; anything else fails the whole snapshot instead of
/// guessing a primary. Unusable display names fall back to a stable label
/// rather than failing cosmetic metadata.
pub fn from_bounds(bounds: Vec<DisplayBounds>) -> Result<Vec<DetectedDisplay>> {
    ensure!(
        bounds.len() <= MAX_OUTPUTS,
        "Too many active macOS displays"
    );
    let mut displays = Vec::with_capacity(bounds.len());
    for bound in bounds {
        ensure!(
            bound.width.is_finite()
                && bound.height.is_finite()
                && bound.width > 0.0
                && bound.height > 0.0
                && bound.x.is_finite()
                && bound.y.is_finite(),
            "CoreGraphics reported unusable display geometry"
        );
        validate_label(&bound.uuid, "Display connector")?;
        let name = if validate_label(&bound.name, "Display name").is_ok() {
            bound.name
        } else {
            format!("Display {}", displays.len() + 1)
        };
        displays.push(DetectedDisplay {
            connector: bound.uuid,
            name,
            x: bound.x,
            y: bound.y,
            width: bound.width,
            height: bound.height,
            primary: bound.main,
        });
    }
    ensure!(
        displays.iter().filter(|display| display.primary).count() == 1,
        "CoreGraphics must report exactly one main display"
    );
    displays.sort_by(|a, b| {
        b.primary
            .cmp(&a.primary)
            .then_with(|| a.x.total_cmp(&b.x))
            .then_with(|| a.y.total_cmp(&b.y))
            .then_with(|| a.connector.cmp(&b.connector))
    });
    validate_displays(&displays)?;
    Ok(displays)
}

#[cfg(target_os = "macos")]
fn c_string(buffer: &[u8]) -> Result<String> {
    let text = str::from_utf8(
        // A missing terminator fails closed instead of inventing a value.
        c_buffer(buffer)?,
    )?;
    Ok(text.to_owned())
}

#[cfg(target_os = "macos")]
fn c_buffer(buffer: &[u8]) -> Result<&[u8]> {
    let end = buffer
        .iter()
        .position(|byte| *byte == 0)
        .context("Native display metadata was not terminated")?;
    Ok(&buffer[..end])
}
