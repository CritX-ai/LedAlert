//! Read-only macOS pinned-launcher discovery: the active Sidebar.app Dock
//! replacement, or the conventional Dock, resolved against installed bundles.
//!
//! Sidebar is consulted only while it actually runs as the active Dock
//! replacement; a malformed or unreadable Sidebar state fails the snapshot
//! instead of silently substituting Dock pins. CFPreferences and NSWorkspace
//! are read-only, no pinned target is ever launched, and custom names, custom
//! icons and window behavior are not used. Only identity, pin visibility and
//! manual ordering participate; display names come from the installed bundle.

use std::collections::HashSet;
#[cfg(target_os = "macos")]
use std::{
    ffi::{CStr, CString, c_char, c_void},
    ptr, slice,
    sync::Arc,
};

use anyhow::{Context, Result, bail, ensure};

#[cfg(target_os = "macos")]
use super::{AppKind, MAX_LABEL_BYTES, PinnedApp, validate_label};
#[cfg(target_os = "macos")]
use crate::app_icons::{self, AppIcon};

const MAX_CONFIG_BYTES: usize = 1024 * 1024;
// Sidebar currently stores hundreds of records (many non-pinned); a working
// Dock replacement cannot meaningfully pin more than this many launchers.
const MAX_RECORDS: usize = 512;
const MAX_PINS: usize = 64;
const MAX_BUNDLE_ID_BYTES: usize = 512;
const MAX_PATH_BYTES: usize = 4096;
#[cfg(target_os = "macos")]
const ICON_SIZE: usize = 64;

/// One discovered pin in configured order: an optional declared identity and
/// the decoded local application path. Identities are confirmed against the
/// installed bundle before anything is reported.
#[derive(Clone, Debug, PartialEq)]
pub struct PinCandidate {
    pub id: Option<String>,
    pub path: String,
}

#[cfg(target_os = "macos")]
type ConfigSink = unsafe extern "C" fn(*const u8, usize, *mut c_void) -> i32;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn ledalert_taskbar_sidebar_active() -> i32;
    fn ledalert_taskbar_sidebar_config(sink: ConfigSink, context: *mut c_void) -> i32;
    fn ledalert_taskbar_dock_config(sink: ConfigSink, context: *mut c_void) -> i32;
    fn ledalert_taskbar_application_lookup(
        id: *const c_char,
        path: *mut c_char,
        path_cap: usize,
    ) -> i32;
    fn ledalert_taskbar_resolve_pin(
        path: *const c_char,
        declared: *const c_char,
        id: *mut c_char,
        id_cap: usize,
        name: *mut c_char,
        name_cap: usize,
        rgba: *mut u8,
        rgba_cap: usize,
        rgba_len: *mut usize,
    ) -> i32;
}

/// Actual pinned applications of the active Dock replacement on a worker
/// thread: Sidebar's pins while Sidebar runs, otherwise Dock persistent apps.
#[cfg(target_os = "macos")]
pub(crate) fn discover() -> Result<Vec<PinnedApp>> {
    let sidebar = native_sidebar_active()?;
    let bytes = native_config(sidebar)?;
    let candidates = if sidebar {
        parse_sidebar_pins(&bytes)?
    } else {
        parse_dock_pins(&bytes)?
    };
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let mut apps = Vec::new();
    let mut seen = HashSet::new();
    for candidate in candidates {
        let Some(app) = resolve_pin(&candidate)? else {
            continue;
        };
        // Identities are shown once; a later tile/pin never duplicates one.
        if !seen.insert(app.id.clone()) {
            continue;
        }
        ensure!(
            apps.len() < MAX_PINS,
            "At most {MAX_PINS} unique pinned applications are supported"
        );
        apps.push(app);
    }
    Ok(apps)
}

/// Worker-only lookup of an exact macOS bundle identifier, pinned or otherwise.
#[cfg(target_os = "macos")]
pub(crate) fn application(id: &str) -> Result<Option<PinnedApp>> {
    validate_id(id)?;
    let requested = CString::new(id).map_err(|_| anyhow::anyhow!("Unsafe application identity"))?;
    let mut path = [0_u8; MAX_PATH_BYTES + 1];
    // SAFETY: the identifier and output buffer are live for the whole call;
    // the native side only queries LaunchServices and never launches anything.
    let status = unsafe {
        ledalert_taskbar_application_lookup(
            requested.as_ptr(),
            path.as_mut_ptr().cast(),
            path.len(),
        )
    };
    match status {
        0 => {}
        1 => return Ok(None),
        _ => bail!("Installed application lookup failed"),
    }
    let path = c_buffer(&path)?;
    resolve_pin(&PinCandidate {
        id: Some(id.to_owned()),
        path,
    })
}

/// Parse visible Sidebar application pins, deduplicated by identity.
/// Complete `manualSortOrder` metadata determines stable order; otherwise the
/// source array order is retained rather than inventing positions for unranked
/// pins. Missing pin flags mean unpinned. Malformed flags or non-local bundle
/// URLs fail the snapshot. Names and icons come from the installed bundle.
pub fn parse_sidebar_pins(bytes: &[u8]) -> Result<Vec<PinCandidate>> {
    ensure!(
        bytes.len() <= MAX_CONFIG_BYTES,
        "Sidebar configuration exceeds one MiB"
    );
    let root: serde_json::Value =
        serde_json::from_slice(bytes).context("Invalid Sidebar configuration JSON")?;
    let records = root
        .as_array()
        .context("Sidebar configuration must be a JSON array")?;
    ensure!(
        records.len() <= MAX_RECORDS,
        "Too many Sidebar configuration records"
    );
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for record in records {
        let record = record
            .as_object()
            .context("Sidebar configuration record must be an object")?;
        let kind = record
            .get("type")
            .and_then(serde_json::Value::as_i64)
            .context("Sidebar record type must be an integer")?;
        if kind != 0 {
            // Non-application items are not pins; they are skipped, not errors.
            continue;
        }
        let id = record
            .get("id")
            .and_then(serde_json::Value::as_str)
            .context("Sidebar application record has no string identity")?;
        validate_id(id)?;
        let flag = |key: &str| -> Result<bool> {
            record
                .get(key)
                .map(|value| {
                    value
                        .as_bool()
                        .with_context(|| format!("Sidebar {key} must be a boolean"))
                })
                .transpose()
                .map(|value| value.unwrap_or(false))
        };
        let pinned = flag("keepInDock")?;
        let pinned_to_main = flag("pinnedToMainScreen")?;
        let hidden = flag("hiddenInDock")?;
        let pinned_to_screen = match record.get("pinnedScreens") {
            None => false,
            Some(value) => {
                let screens = value
                    .as_array()
                    .context("Sidebar pinnedScreens must be a list")?;
                ensure!(
                    screens
                        .iter()
                        .all(|screen| screen.is_number() || screen.is_string()),
                    "Sidebar pinnedScreens entries must be numbers or strings"
                );
                !screens.is_empty()
            }
        };
        let pinned = (pinned || pinned_to_main || pinned_to_screen) && !hidden;
        if !pinned || seen.contains(id) {
            continue;
        }
        ensure!(
            candidates.len() < MAX_PINS,
            "At most {MAX_PINS} pinned applications are supported"
        );
        let url = record
            .get("bundleURL")
            .and_then(serde_json::Value::as_str)
            .context("Pinned Sidebar record has no bundle URL")?;
        let path = local_path(url).context("Invalid Sidebar bundle URL")?;
        let order = match record.get("manualSortOrder") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(
                value
                    .as_f64()
                    .filter(|order| order.is_finite())
                    .context("Sidebar manualSortOrder must be a finite number or null")?,
            ),
        };
        seen.insert(id);
        candidates.push((
            order,
            PinCandidate {
                id: Some(id.to_owned()),
                path,
            },
        ));
    }
    if candidates.iter().all(|(order, _)| order.is_some()) {
        candidates.sort_by(|left, right| {
            left.0
                .expect("complete pin ordering")
                .total_cmp(&right.0.expect("complete pin ordering"))
        });
    }
    Ok(candidates
        .into_iter()
        .map(|(_, candidate)| candidate)
        .collect())
}

/// Parse the serialized Dock `persistent-apps` array. Only `file-tile` entries
/// participate: spacer, directory, URL and unknown tiles are ignored, and
/// recents or running applications are never fabricated as pins. The optional
/// `bundle-identifier` is a declared identity confirmed against the installed
/// bundle; Dock display labels are never used. Wrong-typed required fields and
/// non-local URLs fail the whole snapshot instead of inventing pins.
pub fn parse_dock_pins(bytes: &[u8]) -> Result<Vec<PinCandidate>> {
    ensure!(
        bytes.len() <= MAX_CONFIG_BYTES,
        "Dock configuration exceeds one MiB"
    );
    let root: serde_json::Value =
        serde_json::from_slice(bytes).context("Invalid Dock configuration JSON")?;
    let tiles = root
        .as_array()
        .context("Dock persistent-apps must be a JSON array")?;
    ensure!(
        tiles.len() <= MAX_RECORDS,
        "Too many Dock persistent-apps tiles"
    );
    let mut candidates = Vec::new();
    for tile in tiles {
        let tile = tile
            .as_object()
            .context("Dock persistent-apps tile must be an object")?;
        match tile.get("tile-type").and_then(serde_json::Value::as_str) {
            Some("file-tile") => {}
            Some(_) => continue,
            None => bail!("Dock tile has no tile-type"),
        }
        let data = tile
            .get("tile-data")
            .and_then(serde_json::Value::as_object)
            .context("Dock file-tile has no tile-data object")?;
        let declared = match data.get("bundle-identifier") {
            None => None,
            Some(value) => {
                let id = value
                    .as_str()
                    .context("Dock bundle-identifier must be a string")?;
                validate_id(id)?;
                Some(id.to_owned())
            }
        };
        let file = data
            .get("file-data")
            .and_then(serde_json::Value::as_object)
            .context("Dock file-tile has no file-data object")?;
        let url = file
            .get("_CFURLString")
            .and_then(serde_json::Value::as_str)
            .context("Dock file-tile has no file URL")?;
        let path = local_path(url).context("Invalid Dock file-tile URL")?;
        if let Some(id) = &declared
            && candidates
                .iter()
                .any(|candidate: &PinCandidate| candidate.id.as_deref() == Some(id.as_str()))
        {
            continue;
        }
        candidates.push(PinCandidate { id: declared, path });
    }
    Ok(candidates)
}

/// Decode a `file://` URL (or a plain absolute Dock path) into a local POSIX
/// path. Remote, relative or malformed references fail closed; only empty or
/// `localhost` hosts are local.
pub fn local_path(value: &str) -> Result<String> {
    let path = if let Some(rest) = value.strip_prefix("file://") {
        let (host, tail) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => bail!("File URL has no local path"),
        };
        ensure!(
            matches!(host, "" | "localhost"),
            "File URL must address this machine"
        );
        let decoded = percent_decode(tail)?;
        ensure!(decoded.starts_with('/'), "File URL path must be absolute");
        decoded
    } else if value.starts_with('/') {
        value.to_owned()
    } else {
        bail!("Only local file URLs identify macOS applications");
    };
    ensure!(
        path.len() <= MAX_PATH_BYTES,
        "Application path exceeds the supported length"
    );
    ensure!(
        !path.chars().any(char::is_control),
        "Application path contains control characters"
    );
    Ok(path)
}

fn percent_decode(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let escape = bytes
            .get(index + 1..index + 3)
            .context("Incomplete percent escape")?;
        decoded.push(hex(escape[0])? * 16 + hex(escape[1])?);
        index += 3;
    }
    String::from_utf8(decoded).context("Percent escape is not UTF-8")
}

fn hex(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("Invalid percent escape"),
    }
}

fn validate_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id == id.trim()
            && id.len() <= MAX_BUNDLE_ID_BYTES
            && !id.chars().any(char::is_control),
        "Unsafe application bundle identifier"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn native_sidebar_active() -> Result<bool> {
    // SAFETY: no arguments; the native side only reads the running-application
    // list and never launches or activates anything.
    match unsafe { ledalert_taskbar_sidebar_active() } {
        0 => Ok(false),
        1 => Ok(true),
        _ => bail!("Sidebar Dock-replacement detection failed"),
    }
}

#[cfg(target_os = "macos")]
fn native_config(sidebar: bool) -> Result<Vec<u8>> {
    let mut data: Vec<u8> = Vec::new();
    let context = (&mut data as *mut Vec<u8>).cast();
    // SAFETY: the synchronous callback borrows native data before it is released,
    // and exclusively fills this live Vec once. No cross-allocator ownership.
    let status = unsafe {
        if sidebar {
            ledalert_taskbar_sidebar_config(config_bytes, context)
        } else {
            ledalert_taskbar_dock_config(config_bytes, context)
        }
    };
    let source = if sidebar { "Sidebar" } else { "Dock" };
    ensure!(
        status == 0 && !data.is_empty(),
        "{source} pinned-launcher configuration is unavailable or malformed"
    );
    Ok(data)
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn config_bytes(bytes: *const u8, length: usize, context: *mut c_void) -> i32 {
    if bytes.is_null() || context.is_null() || length == 0 || length > MAX_CONFIG_BYTES {
        return 0;
    }
    // SAFETY: native data and the exclusive Rust Vec context are live for this callback.
    let output = unsafe { &mut *context.cast::<Vec<u8>>() };
    if !output.is_empty() {
        return 0;
    }
    output.extend_from_slice(unsafe { slice::from_raw_parts(bytes, length) });
    1
}

/// Resolve one candidate against the installed bundle. The native side copies
/// identity, display name and a 64×64 RGBA icon (premultiplied, unpremultiplied
/// here once). Missing, stale or misidentified bundles are omitted; a broken
/// native adapter fails the snapshot.
#[cfg(target_os = "macos")]
fn resolve_pin(candidate: &PinCandidate) -> Result<Option<PinnedApp>> {
    let path = CString::new(candidate.path.as_str())
        .map_err(|_| anyhow::anyhow!("Unsafe application path"))?;
    let declared = candidate
        .id
        .as_ref()
        .map(|id| CString::new(id.as_str()))
        .transpose()
        .map_err(|_| anyhow::anyhow!("Unsafe application identity"))?;
    let declared = declared
        .as_ref()
        .map_or(ptr::null(), |value| value.as_ptr());
    let mut id = [0_u8; MAX_BUNDLE_ID_BYTES + 1];
    let mut name = [0_u8; MAX_LABEL_BYTES + 1];
    let icon_bytes = ICON_SIZE * ICON_SIZE * 4;
    let mut rgba: Vec<u8> = Vec::with_capacity(icon_bytes);
    let mut rgba_len = 0usize;
    // SAFETY: every pointer addresses live locals with the stated capacities;
    // the native side only reads bundle metadata and draws one 64×64 icon.
    let status = unsafe {
        ledalert_taskbar_resolve_pin(
            path.as_ptr(),
            declared,
            id.as_mut_ptr().cast(),
            id.len(),
            name.as_mut_ptr().cast(),
            name.len(),
            rgba.as_mut_ptr(),
            icon_bytes,
            &mut rgba_len,
        )
    };
    match status {
        0 => {}
        1 => return Ok(None),
        _ => bail!("Native application metadata lookup failed"),
    }
    ensure!(
        rgba_len == 0 || rgba_len == icon_bytes,
        "Native application icon has an unexpected size"
    );
    let id = c_buffer(&id)?;
    let name = c_buffer(&name)?;
    // Names are decorative bundle metadata, not configuration input: an
    // unusable name or identity omits the pin instead of failing the snapshot.
    if validate_label(&name).is_err() || validate_id(&id).is_err() {
        return Ok(None);
    }
    let icon = if rgba_len == icon_bytes {
        // SAFETY: the native renderer clears and fills exactly this bounded
        // buffer before publishing its length; it never retains the pointer.
        unsafe { rgba.set_len(rgba_len) };
        app_icons::unpremultiply(&mut rgba);
        let dominant = app_icons::dominant_color(&rgba);
        dominant.map(|dominant| {
            Arc::new(AppIcon {
                width: ICON_SIZE,
                height: ICON_SIZE,
                rgba,
                dominant,
            })
        })
    } else {
        None
    };
    let kind = super::windows::classify_app(&id, &name);
    Ok(Some(PinnedApp {
        id,
        name,
        kind,
        suggested: matches!(kind, AppKind::Communication | AppKind::Productivity),
        icon,
    }))
}

#[cfg(target_os = "macos")]
fn c_buffer(buffer: &[u8]) -> Result<String> {
    let text = CStr::from_bytes_until_nul(buffer)
        .context("Native application metadata was not terminated")?
        .to_str()
        .context("Native application metadata is not UTF-8")?;
    Ok(text.to_owned())
}
