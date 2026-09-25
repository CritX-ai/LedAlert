//! Bounded, read-only pinned-launcher metadata and opt-in notification examples.

use std::{
    collections::HashMap,
    fs::{self, File},
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, bail, ensure};

#[cfg(not(target_os = "windows"))]
use crate::app_icons::{absolute_env_path, data_directories};
use crate::{
    app_icons::{self, IconResolver},
    config::{Rule, RuleOptions},
};

/// Portable Taskband parsing and canonical Windows application identities.
pub mod windows;
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod windows_native;

const MAX_APPLETS_BYTES: usize = 1024 * 1024;
const MAX_DESKTOP_BYTES: usize = 64 * 1024;
const MAX_LAUNCHERS: usize = 64;
// Human-readable labels and KDE metadata retain the conservative byte limit.
const MAX_LABEL_BYTES: usize = 128;
const MAX_PATH_PROBES: usize = 512;

#[derive(Clone, Debug)]
pub struct PinnedApp {
    /// Routing ID: Linux desktop ID without its final suffix, or Windows AUMID.
    pub id: String,
    pub name: String,
    pub kind: AppKind,
    /// A useful example to offer, not discovered notification or media capability.
    pub suggested: bool,
    /// The actual declared local icon, decoded off the UI thread.
    pub icon: Option<Arc<app_icons::AppIcon>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppKind {
    Communication,
    Browser,
    Productivity,
    Utility,
}

/// Blocking metadata discovery; call from a worker thread, never the UI thread.
/// Reads KDE launcher metadata or the current Windows user's Taskband pins.
/// Never invokes a launcher or executes a pinned target.
#[cfg(not(target_os = "windows"))]
pub fn discover() -> Result<Vec<PinnedApp>> {
    let home = absolute_env_path("HOME");
    let config = absolute_env_path("XDG_CONFIG_HOME")
        .or_else(|| home.as_ref().map(|path| path.join(".config")))
        .context("Pinned applications require HOME or an absolute XDG_CONFIG_HOME")?;
    let directories = application_directories();
    discover_at(
        &config.join("plasma-org.kde.plasma.desktop-appletsrc"),
        &directories,
    )
}

/// Read the current Windows user's actual Taskband pins on a worker thread.
#[cfg(target_os = "windows")]
pub fn discover() -> Result<Vec<PinnedApp>> {
    windows_native::discover()
}

#[cfg(not(target_os = "windows"))]
fn application_directories() -> Vec<PathBuf> {
    data_directories()
        .into_iter()
        .map(|path| path.join("applications"))
        .collect()
}

/// Blocking lookup of one exact routing ID, including non-pinned applications.
/// Call from a worker, never the UI thread. This neither launches nor enumerates
/// applications. The ID is a routing ID, not a URI or desktop filename: exactly
/// one `.desktop` suffix is appended, even if the routing ID ends in `.desktop`.
#[cfg(not(target_os = "windows"))]
pub fn application(id: &str) -> Result<Option<PinnedApp>> {
    application_at(id, &application_directories())
}

/// Worker-only lookup of an exact Windows AppUserModelID, pinned or otherwise.
#[cfg(target_os = "windows")]
pub fn application(id: &str) -> Result<Option<PinnedApp>> {
    windows_native::application(id)
}

/// Worker-only lookup with explicit application-directory priority. The same
/// Hidden, confinement and malformed-metadata rules apply as to pinned entries.
pub fn application_at(id: &str, application_dirs: &[PathBuf]) -> Result<Option<PinnedApp>> {
    validate_label(id)?;
    ensure!(
        !id.contains(['/', '\\', ':', '?', '#']) && !matches!(id, "." | ".." | "*"),
        "Unsafe application routing ID"
    );
    let desktop_id = format!("{id}.desktop");
    let desktop_id = launcher_id(&desktop_id)?.context("Invalid application routing ID")?;
    let roots = application_roots(application_dirs)?;
    resolve_entry(&desktop_id, &roots, &mut IconResolver::from_environment())
}

/// Resolve only pinned desktop entries, in the supplied application-directory order.
/// Missing pins are omitted. An existing higher-priority entry masks lower copies,
/// including Hidden/non-Application entries; invalid metadata fails the snapshot.
/// Symlinks must resolve inside their application directory. Nested desktop-file
/// IDs are supported, preferring an exact filename over ambiguous hyphen splits.
/// Icon reads and decoding are blocking; this API also belongs on a worker.
pub fn discover_at(applets_path: &Path, application_dirs: &[PathBuf]) -> Result<Vec<PinnedApp>> {
    let input = read_bounded(applets_path, MAX_APPLETS_BYTES)?;
    let launchers = parse_launchers(&input)?;
    if launchers.is_empty() {
        return Ok(Vec::new());
    }
    let roots = application_roots(application_dirs)?;
    let mut icons = IconResolver::from_environment();
    let mut apps = Vec::new();
    for desktop_id in launchers {
        if let Some(app) = resolve_entry(&desktop_id, &roots, &mut icons)? {
            apps.push(app);
        }
    }
    Ok(apps)
}

fn application_roots(application_dirs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    for directory in application_dirs {
        match directory.canonicalize() {
            Ok(root) => {
                ensure!(root.is_dir(), "Application search root is not a directory");
                if !roots.contains(&root) {
                    roots.push(root);
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("Resolve application search directory"),
        }
    }
    Ok(roots)
}

fn resolve_entry(
    desktop_id: &str,
    roots: &[PathBuf],
    icons: &mut IconResolver,
) -> Result<Option<PinnedApp>> {
    let mut probes = MAX_PATH_PROBES;
    for root in roots {
        if let Some(path) = locate_entry(root, root, desktop_id, &mut probes)? {
            let path = confined_path(root, &path)?;
            let entry = read_bounded(&path, MAX_DESKTOP_BYTES)?;
            return parse_desktop_entry(desktop_id, &entry, icons);
        }
    }
    Ok(None)
}

fn read_bounded(path: &Path, limit: usize) -> Result<String> {
    let metadata = fs::metadata(path).context("Inspect pinned-application metadata file")?;
    ensure!(
        metadata.is_file(),
        "Pinned-application metadata must be a regular file"
    );
    ensure!(
        metadata.len() <= limit as u64,
        "Pinned-application metadata exceeds its size limit"
    );
    let file = File::open(path).context("Open pinned-application metadata file")?;
    ensure!(
        file.metadata()?.is_file(),
        "Pinned-application metadata must be a regular file"
    );
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .context("Read pinned-application metadata file")?;
    ensure!(
        bytes.len() <= limit,
        "Pinned-application metadata exceeds its size limit"
    );
    String::from_utf8(bytes).context("Pinned-application metadata is not UTF-8")
}

fn confined_path(root: &Path, path: &Path) -> Result<PathBuf> {
    let resolved = path
        .canonicalize()
        .context("Resolve pinned desktop entry")?;
    ensure!(
        resolved.starts_with(root),
        "Pinned desktop entry escapes its application directory"
    );
    Ok(resolved)
}

fn probe(path: &Path, remaining: &mut usize) -> Result<Option<fs::Metadata>> {
    ensure!(
        *remaining > 0,
        "Pinned desktop entry requires too many path lookups"
    );
    *remaining -= 1;
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("Inspect pinned desktop entry path"),
    }
}

fn locate_entry(
    root: &Path,
    directory: &Path,
    id: &str,
    remaining: &mut usize,
) -> Result<Option<PathBuf>> {
    let direct = directory.join(id);
    if probe(&direct, remaining)?.is_some() {
        return Ok(Some(direct));
    }
    // The desktop-file ID replaces subdirectory separators with hyphens. Probe
    // only prefixes of this pin, never enumerate/read unrelated desktop entries.
    for (index, _) in id.match_indices('-') {
        let prefix = &id[..index];
        if prefix.is_empty() || matches!(prefix, "." | "..") {
            continue;
        }
        let child = directory.join(prefix);
        if probe(&child, remaining)?.is_some() {
            let child = confined_path(root, &child)?;
            if child.is_dir()
                && let Some(path) = locate_entry(root, &child, &id[index + 1..], remaining)?
            {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

type AppletKey<'a> = (&'a str, &'a str);

/// Parse complete Plasma applets KConfig, not an unscoped launcher list.
/// Only taskmanager/icontasks Configuration/General lists are eligible. Returns
/// unique desktop file IDs (including .desktop) in list/section order. Application
/// URIs and bare desktop IDs are supported; file/preferred/other URIs, localized
/// keys and expansion/deletion-marked values are not discovery inputs.
pub fn parse_launchers(input: &str) -> Result<Vec<String>> {
    ensure!(
        input.len() <= MAX_APPLETS_BYTES,
        "Plasma applets configuration exceeds one MiB"
    );
    ensure!(
        !input
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')),
        "Plasma applets configuration contains control characters"
    );
    let mut plugins = HashMap::new();
    let mut lists: Vec<(AppletKey<'_>, Option<&str>)> = Vec::new();
    let mut group = None;
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            group = applet_group(line)?;
            continue;
        }
        let Some((applet, general)) = group else {
            continue;
        };
        let (key, value) = line.split_once('=').unwrap_or((line, ""));
        let key = key.trim();
        let expected = if general { "launchers" } else { "plugin" };
        if key.split('[').next() != Some(expected) {
            continue;
        }
        let supported = key == expected || key.strip_suffix("[$i]") == Some(expected);
        ensure!(
            !supported || line.contains('='),
            "Malformed task-manager metadata key"
        );
        let value = supported.then_some(value.trim());
        if general {
            if let Some((_, previous)) = lists.iter_mut().find(|(id, _)| *id == applet) {
                *previous = value;
            } else {
                lists.push((applet, value));
            }
        } else {
            plugins.insert(applet, value);
        }
    }
    let mut ids = Vec::new();
    for (applet, value) in lists {
        let Some(Some(plugin)) = plugins.get(&applet) else {
            continue;
        };
        if !matches!(
            decode_kconfig(plugin)?.as_str(),
            "org.kde.plasma.taskmanager" | "org.kde.plasma.icontasks"
        ) {
            continue;
        }
        let Some(value) = value else { continue };
        let decoded = decode_kconfig(value)?;
        if decoded.is_empty() || decoded == "\\0" {
            continue;
        }
        let mut item = String::new();
        let mut quoted = false;
        for ch in decoded.chars().chain(std::iter::once(',')) {
            if quoted {
                ensure!(
                    matches!(ch, ',' | ';' | '\\'),
                    "Invalid launcher list escape"
                );
                item.push(ch);
                quoted = false;
            } else if ch == '\\' {
                quoted = true;
            } else if ch == ',' {
                if let Some(id) = launcher_id(&item)?
                    && !ids.contains(&id)
                {
                    ensure!(
                        ids.len() < MAX_LAUNCHERS,
                        "At most 64 unique pinned applications are supported"
                    );
                    ids.push(id);
                }
                item.clear();
            } else {
                item.push(ch);
            }
        }
        ensure!(
            !quoted && item.is_empty(),
            "Unterminated launcher list escape"
        );
    }
    Ok(ids)
}

fn applet_group(line: &str) -> Result<Option<(AppletKey<'_>, bool)>> {
    let line = line.strip_suffix("[$i]").unwrap_or(line);
    let inner = line
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .context("Malformed Plasma applet group")?;
    let mut parts = inner.split("][");
    let Some("Containments") = parts.next() else {
        return Ok(None);
    };
    let Some(containment) = parts.next() else {
        return Ok(None);
    };
    if parts.next() != Some("Applets") {
        return Ok(None);
    }
    let Some(applet) = parts.next() else {
        return Ok(None);
    };
    ensure!(
        !containment.is_empty()
            && containment.bytes().all(|ch| ch.is_ascii_digit())
            && !applet.is_empty()
            && applet.bytes().all(|ch| ch.is_ascii_digit()),
        "Malformed applet identity"
    );
    let general = match (parts.next(), parts.next(), parts.next()) {
        (None, None, None) => false,
        (Some("Configuration"), Some("General"), None) => true,
        _ => return Ok(None),
    };
    Ok(Some(((containment, applet), general)))
}

fn hex(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("Invalid metadata escape"),
    }
}

fn decode_kconfig(value: &str) -> Result<String> {
    let mut decoded = Vec::with_capacity(value.len());
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            decoded.push(byte);
            continue;
        }
        match bytes.next().context("Incomplete KConfig escape")? {
            b's' => decoded.push(b' '),
            b'n' => decoded.push(b'\n'),
            b'r' => decoded.push(b'\r'),
            b't' => decoded.push(b'\t'),
            b'\\' => decoded.push(b'\\'),
            ch @ (b',' | b';') => decoded.extend_from_slice(&[b'\\', ch]),
            b'x' => {
                let high = hex(bytes.next().context("Incomplete KConfig hex escape")?)?;
                let low = hex(bytes.next().context("Incomplete KConfig hex escape")?)?;
                decoded.push(high * 16 + low);
            }
            _ => bail!("Invalid KConfig escape"),
        }
    }
    String::from_utf8(decoded).context("KConfig escape is not UTF-8")
}

fn launcher_id(launcher: &str) -> Result<Option<String>> {
    if launcher.is_empty() {
        return Ok(None);
    }
    let id = if let Some(value) = launcher.strip_prefix("applications:") {
        let mut decoded = Vec::with_capacity(value.len());
        let mut bytes = value.bytes();
        while let Some(byte) = bytes.next() {
            if byte == b'%' {
                let high = hex(bytes.next().context("Incomplete launcher URI escape")?)?;
                let low = hex(bytes.next().context("Incomplete launcher URI escape")?)?;
                decoded.push(high * 16 + low);
            } else {
                decoded.push(byte);
            }
        }
        String::from_utf8(decoded).context("Launcher URI is not UTF-8")?
    } else if launcher.contains(':') {
        return Ok(None);
    } else {
        launcher.to_owned()
    };
    validate_label(&id)?;
    ensure!(
        !id.contains(['/', '\\', ':', '?', '#']),
        "Unsafe desktop file ID"
    );
    let routing_id = id
        .strip_suffix(".desktop")
        .context("Launcher is not a desktop file ID")?;
    ensure!(
        !matches!(routing_id, "" | "." | ".." | "*"),
        "Unsafe desktop routing ID"
    );
    validate_label(routing_id)?;
    Ok(Some(id))
}

fn validate_label(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value == value.trim()
            && value.len() <= MAX_LABEL_BYTES
            && !value.chars().any(char::is_control),
        "Application labels must be 1–128 bytes without control characters or surrounding spaces"
    );
    Ok(())
}

fn desktop_string(value: &str) -> Result<String> {
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        result.push(
            match chars.next().context("Incomplete desktop entry escape")? {
                's' => ' ',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                _ => bail!("Invalid desktop entry escape"),
            },
        );
    }
    Ok(result)
}

fn parse_desktop_entry(
    desktop_id: &str,
    input: &str,
    icons: &mut IconResolver,
) -> Result<Option<PinnedApp>> {
    let mut active = false;
    let mut found = false;
    let (mut name, mut categories, mut kind, mut hidden) = (None, None, None, None);
    let mut icon = None;
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            ensure!(line.ends_with(']'), "Malformed desktop entry group");
            active = line == "[Desktop Entry]";
            if active {
                ensure!(!found, "Duplicate Desktop Entry group");
                found = true;
            }
            continue;
        }
        if !active {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .context("Malformed desktop entry key")?;
        let target = match key.trim() {
            "Name" => &mut name,
            "Categories" => &mut categories,
            "Type" => &mut kind,
            "Hidden" => &mut hidden,
            "Icon" => &mut icon,
            // In particular, Exec, TryExec and desktop-action contents are ignored.
            _ => continue,
        };
        ensure!(target.is_none(), "Duplicate desktop entry metadata key");
        *target = Some(value.trim());
    }
    match hidden {
        Some("true") => return Ok(None),
        None | Some("false") => {}
        _ => bail!("Invalid desktop entry Hidden value"),
    }
    if kind != Some("Application") {
        return Ok(None);
    }
    let name = desktop_string(name.context("Pinned desktop entry has no Name")?)?;
    validate_label(&name)?;
    let categories = desktop_string(categories.unwrap_or_default())?;
    ensure!(
        !categories.chars().any(char::is_control),
        "Invalid desktop entry categories"
    );
    let id = desktop_id
        .strip_suffix(".desktop")
        .context("Invalid pinned desktop file ID")?
        .to_owned();
    let kind = classify(&id, &categories);
    Ok(Some(PinnedApp {
        id,
        name,
        kind,
        suggested: matches!(kind, AppKind::Communication | AppKind::Productivity),
        icon: icon
            .and_then(|value| desktop_string(value).ok())
            .and_then(|value| icons.resolve(&value)),
    }))
}

fn classify(id: &str, categories: &str) -> AppKind {
    let has = |wanted: &[&str]| {
        categories
            .split(';')
            .any(|category| wanted.contains(&category))
    };
    if has(&["System", "Settings", "Security", "Accessibility"]) {
        return AppKind::Utility;
    }
    if has(&[
        "Email",
        "InstantMessaging",
        "Chat",
        "IRCClient",
        "Telephony",
        "VideoConference",
    ]) {
        return AppKind::Communication;
    }
    if has(&["WebBrowser"]) {
        return AppKind::Browser;
    }
    if has(&["Development", "IDE", "TextEditor", "Notes"]) {
        return AppKind::Productivity;
    }
    // A small example-only fallback for widely used entries lacking specific
    // categories. Neither these IDs nor categories establish event capabilities.
    let known = |ids: &[&str]| ids.iter().any(|known| id.eq_ignore_ascii_case(known));
    if known(&[
        "signal",
        "org.signal.signal",
        "telegram",
        "org.telegram.desktop",
        "element",
        "im.riot.riot",
    ]) {
        AppKind::Communication
    } else if known(&["firefox", "org.mozilla.firefox", "chromium"]) {
        AppKind::Browser
    } else if known(&[
        "code",
        "codium",
        "com.vscodium.codium",
        "obsidian",
        "md.obsidian.obsidian",
    ]) {
        AppKind::Productivity
    } else {
        AppKind::Utility
    }
}

/// Restrained, finite notification examples only. Pinning establishes no MPRIS
/// support or guarantee of notifications. This does not modify existing rules.
pub fn suggested_rule(app: &PinnedApp, screen_id: u32, room_width: f32) -> Rule {
    let color = app.icon.as_ref().map_or_else(
        || match app.kind {
            AppKind::Communication => [85, 170, 150],
            AppKind::Browser => [95, 150, 215],
            AppKind::Productivity => [175, 140, 200],
            AppKind::Utility => [185, 160, 110],
        },
        |icon| icon.dominant,
    );
    Rule {
        application: app.id.clone(),
        screen_id,
        color,
        duration: 3.0,
        spread: if room_width.is_finite() {
            (room_width * 0.18).clamp(0.1, 2.0)
        } else {
            0.75
        },
        enabled: true,
        options: RuleOptions {
            notifications: true,
            media: false,
            minimum_urgency: 0,
            intensity: 0.35,
            fade: true,
            critical_accent: false,
            ..RuleOptions::default()
        },
    }
}
