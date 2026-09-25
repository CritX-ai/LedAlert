//! Read-only Windows 11 Taskband snapshot interpretation.
//!
//! `Favorites` is Explorer's current pinned ITEMIDLIST sequence, not the Start
//! menu inventory and not `FavoritesResolve` (which contains cached links).
//! The version-3 framing and BEEF001D identity extension are covered by captured
//! Windows 11 fixtures. Unknown framing fails closed rather than inventing pins.

use anyhow::{Context, Result, ensure};

use super::PinnedApp;

pub const MAX_TASKBAND_BYTES: usize = 1024 * 1024;
const MAX_PIDL_BYTES: usize = 64 * 1024;
const MAX_PINS: usize = 64;

/// Windows AUMIDs are opaque, case-sensitive identifiers, not executable names.
/// Preserve exactly the same identity supplied by notifications and media.
/// The Windows limit is 128 UTF-16 code units; UTF-8 storage can be larger.
pub fn canonical_app_id(id: &str) -> Option<&str> {
    (!id.is_empty()
        && id != "*"
        && id.len() <= 512
        && id == id.trim()
        && !id.chars().any(char::is_control)
        && id.encode_utf16().count() <= 128)
        .then_some(id)
}

/// Restrained examples only: these categories do not claim notification/media
/// capability. Display-name matching also covers generated/hash Win32 AUMIDs.
pub fn classify_app(id: &str, name: &str) -> super::AppKind {
    use super::AppKind;
    let named = |names: &[&str]| names.iter().any(|known| name.eq_ignore_ascii_case(known));
    if matches!(
        id,
        "org.whispersystems.signal-desktop"
            | "org.ferdium.ferdium-app"
            | "com.squirrel.proton_mail.ProtonMail"
            | "Microsoft.OutlookForWindows_8wekyb3d8bbwe!Microsoft.OutlookforWindows"
            | "MSTeams_8wekyb3d8bbwe!MSTeams"
    ) || named(&[
        "Signal",
        "Telegram",
        "Ferdium",
        "Betterbird",
        "Thunderbird",
        "Proton Mail",
        "Outlook",
        "Microsoft Teams",
        "Slack",
        "Discord",
        "Element",
    ]) {
        AppKind::Communication
    } else if matches!(id, "MSEdge" | "Chrome")
        || named(&[
            "Microsoft Edge",
            "Google Chrome",
            "Firefox",
            "Mozilla Firefox",
            "Floorp",
            "Brave",
            "Vivaldi",
        ])
    {
        AppKind::Browser
    } else if matches!(id, "Microsoft.VisualStudioCode" | "com.clickup.desktop-app")
        || named(&[
            "Visual Studio Code",
            "VSCodium",
            "ClickUp",
            "Obsidian",
            "Notion",
        ])
    {
        AppKind::Productivity
    } else {
        super::classify(id, "")
    }
}
#[derive(Debug)]
pub struct TaskbandPin<'a> {
    /// Validated, terminated ITEMIDLIST bytes; native callers must copy to an
    /// aligned allocation before handing this byte-oriented storage to Shell.
    pub pidl: &'a [u8],
    /// Explorer's explicit identity extension, including generated Win32 IDs.
    pub app_id: Option<String>,
}

/// Parse the complete `Taskband\\Favorites` version-3 value. Tombstones and
/// cached `FavoritesResolve` entries deliberately do not participate.
pub fn parse_taskband(bytes: &[u8]) -> Result<Vec<TaskbandPin<'_>>> {
    ensure!(
        bytes.len() <= MAX_TASKBAND_BYTES,
        "Taskband metadata exceeds one MiB"
    );
    let mut remaining = bytes;
    let mut pins = Vec::new();
    loop {
        if remaining == [0xff] {
            return Ok(pins);
        }
        ensure!(pins.len() < MAX_PINS, "Too many taskbar pins");
        ensure!(
            remaining.len() >= 5 && remaining[0] == 0,
            "Unsupported Taskband record framing"
        );
        let len = u32::from_le_bytes(remaining[1..5].try_into().unwrap()) as usize;
        ensure!(
            (2..=MAX_PIDL_BYTES).contains(&len),
            "Invalid Taskband PIDL size"
        );
        let pidl = remaining
            .get(5..5 + len)
            .context("Truncated Taskband PIDL")?;
        let app_id = parse_pidl(pidl)?;
        pins.push(TaskbandPin { pidl, app_id });
        remaining = &remaining[5 + len..];
    }
}

fn parse_pidl(pidl: &[u8]) -> Result<Option<String>> {
    let mut remaining = pidl;
    let mut identity = None;
    loop {
        ensure!(remaining.len() >= 2, "Unterminated Taskband PIDL");
        let size = u16::from_le_bytes([remaining[0], remaining[1]]) as usize;
        if size == 0 {
            ensure!(remaining.len() == 2, "Trailing Taskband PIDL data");
            return Ok(identity);
        }
        ensure!(
            size >= 2 && size <= remaining.len(),
            "Invalid Taskband shell item length"
        );
        let item = &remaining[..size];
        // Extension blocks are word-aligned relative to their shell item. Each
        // carries its own size, version and signature. Do not scrape arbitrary
        // UTF-16 strings (package paths/display names are not routing IDs).
        for offset in (2..item.len().saturating_sub(7)).step_by(2) {
            if item[offset + 4..offset + 8] != [0x1d, 0x00, 0xef, 0xbe] {
                continue;
            }
            let length = u16::from_le_bytes([item[offset], item[offset + 1]]) as usize;
            ensure!(
                length >= 14 && length.is_multiple_of(2),
                "Invalid Taskband identity extension size"
            );
            let extension = item
                .get(offset..offset + length)
                .context("Truncated Taskband identity extension")?;
            ensure!(
                extension[2..4] == [0, 0] && extension[8..10] == [2, 0],
                "Unsupported Taskband identity extension"
            );
            let encoded = &extension[10..length - 2];
            ensure!(
                encoded.ends_with(&[0, 0]),
                "Unterminated Taskband application identity"
            );
            let units = encoded[..encoded.len() - 2]
                .chunks_exact(2)
                .map(|word| u16::from_le_bytes([word[0], word[1]]))
                .collect::<Vec<_>>();
            let id = String::from_utf16(&units).context("Invalid UTF-16 taskbar identity")?;
            ensure!(
                canonical_app_id(&id).is_some(),
                "Invalid taskbar application identity"
            );
            if let Some(previous) = &identity {
                ensure!(
                    previous == &id,
                    "Conflicting taskbar application identities"
                );
            } else {
                identity = Some(id);
            }
        }
        remaining = &remaining[size..];
    }
}

/// Resolve only the actual pinned records, preserving their order. A stale or
/// unresolvable Shell item is omitted; duplicate identities are shown once.
/// The resolver must only retrieve metadata, never activate the target.
pub fn resolve_taskband(
    bytes: &[u8],
    mut resolve: impl FnMut(&TaskbandPin<'_>) -> Option<PinnedApp>,
) -> Result<Vec<PinnedApp>> {
    let pins = parse_taskband(bytes)?;
    let mut apps: Vec<PinnedApp> = Vec::new();
    for pin in pins {
        if pin
            .app_id
            .as_ref()
            .is_some_and(|id| apps.iter().any(|app| app.id == *id))
        {
            continue;
        }
        let Some(mut app) = resolve(&pin) else {
            continue;
        };
        if let Some(id) = pin.app_id {
            app.id = id;
        }
        if canonical_app_id(&app.id).is_none()
            || app.name.is_empty()
            || app.name.len() > 128
            || app.name.chars().any(char::is_control)
            || apps.iter().any(|known| known.id == app.id)
        {
            continue;
        }
        apps.push(app);
    }
    Ok(apps)
}
