use anyhow::{Context, Result, bail, ensure};
use windows::Win32::{
    Devices::Display::{
        DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER,
        DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_PATH_INFO,
        DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes,
        QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
    },
    Foundation::ERROR_INSUFFICIENT_BUFFER,
};

use super::{DisplayPath, from_paths};
use crate::displays::{DetectedDisplay, MAX_OUTPUTS};

pub fn discover() -> Result<Vec<DetectedDisplay>> {
    // Hot-plug can change the required buffer between the two CCD calls. Bound
    // retries and allocations; a later explicit Refresh can retry a busy desktop.
    for _ in 0..3 {
        let (mut path_count, mut mode_count) = (0, 0);
        // SAFETY: both outputs point to live, writable u32 values.
        unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
        }
        .ok()
        .context("Read Windows display buffer sizes")?;
        ensure!(
            path_count as usize <= MAX_OUTPUTS && mode_count as usize <= MAX_OUTPUTS * 3,
            "Windows display topology exceeds supported limits"
        );
        if path_count == 0 {
            return Ok(Vec::new());
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        // SAFETY: buffers contain initialized elements and match the supplied
        // capacities. Windows writes at most those capacities on success.
        let result = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if result == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        result.ok().context("Read active Windows displays")?;
        ensure!(
            path_count as usize <= paths.len() && mode_count as usize <= modes.len(),
            "Windows returned invalid display counts"
        );
        modes.truncate(mode_count as usize);
        let mut snapshot = Vec::with_capacity(path_count as usize);
        for path in paths.iter().take(path_count as usize) {
            // SAFETY: without QDC_VIRTUAL_MODE_AWARE this union is modeInfoIdx.
            let index = unsafe { path.sourceInfo.Anonymous.modeInfoIdx };
            let mode = modes
                .get(index as usize)
                .context("Windows display source mode missing")?;
            ensure!(
                mode.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE
                    && mode.id == path.sourceInfo.id
                    && mode.adapterId == path.sourceInfo.adapterId,
                "Windows display source mode is inconsistent"
            );
            // SAFETY: infoType was checked before reading the tagged union.
            let source = unsafe { mode.Anonymous.sourceMode };
            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                    adapterId: path.targetInfo.adapterId,
                    id: path.targetInfo.id,
                },
                ..Default::default()
            };
            // SAFETY: the header is the first member of the correctly sized
            // native target-name packet and stays live for the entire call.
            let status = unsafe { DisplayConfigGetDeviceInfo(&mut target.header) };
            ensure!(
                status == 0,
                "Read Windows monitor identity failed ({status})"
            );
            let monitor_path = text(&target.monitorDevicePath)?;
            let name = text(&target.monitorFriendlyDeviceName)?;
            snapshot.push(DisplayPath {
                name: if name.is_empty() {
                    monitor_path.clone()
                } else {
                    name
                },
                monitor_path,
                x: source.position.x,
                y: source.position.y,
                width: source.width,
                height: source.height,
            });
        }
        return from_paths(snapshot);
    }
    bail!("Windows display topology changed during discovery; Refresh to retry")
}

fn text(units: &[u16]) -> Result<String> {
    let end = units
        .iter()
        .position(|unit| *unit == 0)
        .context("Unterminated Windows display name")?;
    String::from_utf16(&units[..end]).context("Invalid Windows display name")
}
