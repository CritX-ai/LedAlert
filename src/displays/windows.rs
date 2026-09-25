//! Windows CCD snapshots. Geometry conversion stays portable for Linux regression tests.
use anyhow::{Result, ensure};

use super::{DetectedDisplay, MAX_OUTPUTS, validate_displays};

#[cfg(windows)]
#[allow(unsafe_code)]
mod native;
#[cfg(windows)]
pub use native::discover;

/// One active QueryDisplayConfig path. Source dimensions are already rotated.
#[derive(Clone, Debug)]
pub struct DisplayPath {
    pub monitor_path: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Keep Windows desktop pixels in one coordinate space, including mixed-DPI
/// displays. Dividing each monitor independently by its scale breaks adjacency.
/// Monitor device paths, not enumeration indexes or friendly names, identify
/// saved placements. Clone targets deliberately retain their distinct identities.
pub fn from_paths(paths: Vec<DisplayPath>) -> Result<Vec<DetectedDisplay>> {
    ensure!(paths.len() <= MAX_OUTPUTS, "Too many Windows display paths");
    let mut displays: Vec<_> = paths
        .into_iter()
        .map(|path| DetectedDisplay {
            connector: path.monitor_path,
            name: path.name,
            primary: path.x == 0 && path.y == 0,
            x: path.x as f32,
            y: path.y as f32,
            width: path.width as f32,
            height: path.height as f32,
        })
        .collect();
    validate_displays(&displays)?;
    displays.sort_by(|a, b| {
        b.primary
            .cmp(&a.primary)
            .then_with(|| a.x.total_cmp(&b.x))
            .then_with(|| a.y.total_cmp(&b.y))
            .then_with(|| a.connector.cmp(&b.connector))
    });
    Ok(displays)
}
