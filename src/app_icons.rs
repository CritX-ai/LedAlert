//! Local desktop-entry icons. All resolver APIs perform blocking file reads and
//! decoding: use a worker thread, never the UI thread. No icon names are guessed.

use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use resvg::{tiny_skia, usvg};

const ICON_SIZE: u32 = 64;
const MAX_ASSET_BYTES: usize = 2 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 128 * 1024;
const MAX_RASTER_SIDE: u32 = 1024;
const MAX_SVG_SIDE: f32 = 4096.0;
const MAX_THEMES: usize = 32;
// The standard hicolor index already contains more than 600 directories.
const MAX_DIRECTORIES: usize = 1024;
const MAX_PROBES: usize = 16_384;

#[derive(Debug)]
pub struct AppIcon {
    pub width: usize,
    pub height: usize,
    /// Unpremultiplied sRGB pixels, suitable for egui's RGBA texture input.
    pub rgba: Vec<u8>,
    /// An alpha-weighted source color, not a tint or an invented category color.
    pub dominant: [u8; 3],
}

/// A worker-local lookup snapshot. Theme metadata is cached within this snapshot.
/// Missing, unsupported or unsafe assets return None, never a substitute icon.
pub struct IconResolver {
    theme: String,
    roots: Vec<PathBuf>,
    fallback_dirs: Vec<PathBuf>,
    themes: HashMap<String, Option<Arc<Theme>>>,
}

struct Theme {
    directories: Vec<String>,
    inherits: Vec<String>,
}

impl IconResolver {
    /// Read the configured KDE theme (then GTK settings when KDE is absent),
    /// searching XDG data roots, legacy ~/.icons, inheritance and hicolor.
    pub fn from_environment() -> Self {
        let home = absolute_env_path("HOME");
        let mut configs = Vec::new();
        if let Some(config) = absolute_env_path("XDG_CONFIG_HOME")
            .or_else(|| home.as_ref().map(|path| path.join(".config")))
        {
            configs.push(config);
        }
        configs.extend(environment_paths("XDG_CONFIG_DIRS", "/etc/xdg"));
        let theme = configured_theme(&configs).unwrap_or_else(|| "hicolor".into());
        let data = data_directories();
        let mut roots = Vec::new();
        if let Some(home) = home {
            roots.push(home.join(".icons"));
        }
        roots.extend(data.iter().map(|path| path.join("icons")));
        let fallback = data.iter().map(|path| path.join("pixmaps")).collect();
        Self::new(&theme, roots, fallback)
    }

    /// Explicit, deterministic search roots, useful for isolated worker discovery.
    /// Root order breaks equal-size ties; theme directory order precedes root order.
    pub fn new(theme: &str, roots: Vec<PathBuf>, fallback_dirs: Vec<PathBuf>) -> Self {
        Self {
            theme: if safe_theme(theme) { theme } else { "hicolor" }.into(),
            roots: unique_absolute(roots),
            fallback_dirs: unique_absolute(fallback_dirs),
            themes: HashMap::new(),
        }
    }

    /// Resolve exactly a desktop entry's Icon value. Absolute local PNG/SVG paths
    /// are allowed; relative paths, URLs and shell expansions are not.
    pub fn resolve(&mut self, declared: &str) -> Option<Arc<AppIcon>> {
        if declared.is_empty() || declared.len() > 4096 || declared.chars().any(char::is_control) {
            return None;
        }
        let path = Path::new(declared);
        if path.is_absolute() {
            return decode_file(path).map(Arc::new);
        }
        if !safe_component(declared) {
            return None;
        }
        let names = if declared.ends_with(".png") || declared.ends_with(".svg") {
            vec![declared.to_owned()]
        } else {
            vec![format!("{declared}.png"), format!("{declared}.svg")]
        };
        let mut probes = MAX_PROBES;
        let mut seen = HashSet::new();
        let theme = self.theme.clone();
        // Resolve the winning path before decoding: a corrupt high-priority asset
        // must not silently turn into a different lower-priority application icon.
        let path = self
            .find_in_theme(&theme, &names, &mut seen, &mut probes)
            .or_else(|| self.find_in_theme("hicolor", &names, &mut seen, &mut probes))
            .or_else(|| {
                for root in self.roots.iter().chain(&self.fallback_dirs) {
                    for name in &names {
                        let path = root.join(name);
                        if exists(&path, &mut probes) {
                            return Some(path);
                        }
                    }
                }
                None
            })?;
        decode_file(&path).map(Arc::new)
    }

    fn find_in_theme(
        &mut self,
        name: &str,
        names: &[String],
        seen: &mut HashSet<String>,
        probes: &mut usize,
    ) -> Option<PathBuf> {
        if seen.len() >= MAX_THEMES || !seen.insert(name.to_owned()) || *probes == 0 {
            return None;
        }
        let theme = self.load_theme(name, probes)?;
        for directory in &theme.directories {
            for root in &self.roots {
                for filename in names {
                    let path = root.join(name).join(directory).join(filename);
                    if exists(&path, probes) {
                        return Some(path);
                    }
                }
            }
        }
        for parent in &theme.inherits {
            // The required hicolor fallback is always last, even when a parent
            // names it before another inherited theme.
            if parent != "hicolor"
                && let Some(path) = self.find_in_theme(parent, names, seen, probes)
            {
                return Some(path);
            }
        }
        None
    }

    fn load_theme(&mut self, name: &str, probes: &mut usize) -> Option<Arc<Theme>> {
        if let Some(theme) = self.themes.get(name) {
            return theme.clone();
        }
        let mut theme = None;
        for root in &self.roots {
            let path = root.join(name).join("index.theme");
            if exists(&path, probes) {
                theme = read_text(&path)
                    .and_then(|text| parse_theme(&text))
                    .map(Arc::new);
                break;
            }
        }
        self.themes.insert(name.to_owned(), theme.clone());
        theme
    }
}

pub(crate) fn absolute_env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn environment_paths(key: &str, default: &str) -> Vec<PathBuf> {
    let value = std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.into());
    std::env::split_paths(&value)
        .filter(|path| path.is_absolute())
        .collect()
}

pub(crate) fn data_directories() -> Vec<PathBuf> {
    let mut data = Vec::new();
    if let Some(path) = absolute_env_path("XDG_DATA_HOME")
        .or_else(|| absolute_env_path("HOME").map(|home| home.join(".local/share")))
    {
        data.push(path);
    }
    data.extend(environment_paths(
        "XDG_DATA_DIRS",
        "/usr/local/share:/usr/share",
    ));
    unique_absolute(data)
}

fn unique_absolute(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for path in paths {
        if path.is_absolute() && !result.contains(&path) {
            result.push(path);
        }
    }
    result
}

fn configured_theme(configs: &[PathBuf]) -> Option<String> {
    for (file, section, key) in [
        ("kdeglobals", "Icons", "Theme"),
        ("gtk-4.0/settings.ini", "Settings", "gtk-icon-theme-name"),
        ("gtk-3.0/settings.ini", "Settings", "gtk-icon-theme-name"),
    ] {
        for config in configs {
            let Some(text) = read_text(&config.join(file)) else {
                continue;
            };
            let entries = ini(&text);
            let Some(group) = entries.get(section) else {
                continue;
            };
            // KConfig deletion/expansion flags never authorize expansion or a
            // lower-priority theme. Immutable scalar values remain usable.
            if group.contains_key(format!("{key}[$d]").as_str())
                || group.contains_key(format!("{key}[$e]").as_str())
            {
                return Some("hicolor".into());
            }
            if let Some(value) = group
                .get(key)
                .or_else(|| group.get(format!("{key}[$i]").as_str()))
            {
                return Some(if safe_theme(value) {
                    (*value).into()
                } else {
                    "hicolor".into()
                });
            }
        }
    }
    None
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !matches!(value, "." | "..")
        && !value.contains(['/', '\\', ':', '?', '#', '$', '~'])
        && !value.chars().any(char::is_control)
        && value == value.trim()
}

fn safe_theme(value: &str) -> bool {
    safe_component(value) && value.is_ascii() && !value.contains([' ', ','])
}

fn exists(path: &Path, remaining: &mut usize) -> bool {
    if *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    fs::symlink_metadata(path).is_ok()
}

fn read_bytes(path: &Path, limit: usize) -> Option<Vec<u8>> {
    // Check before open so ordinary devices/FIFOs cannot block a worker. Recheck
    // the opened handle; only regular local files are decoding inputs.
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return None;
    }
    let file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit as u64 + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= limit).then_some(bytes)
}

fn read_text(path: &Path) -> Option<String> {
    String::from_utf8(read_bytes(path, MAX_METADATA_BYTES)?).ok()
}

fn ini(text: &str) -> HashMap<&str, HashMap<&str, &str>> {
    let mut groups = HashMap::<&str, HashMap<&str, &str>>::new();
    let mut group = "";
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            group = name;
        } else if let Some((key, value)) = line.split_once('=') {
            groups
                .entry(group)
                .or_default()
                .insert(key.trim(), value.trim());
        }
    }
    groups
}

fn parse_theme(text: &str) -> Option<Theme> {
    let groups = ini(text);
    let header = groups.get("Icon Theme")?;
    let mut directories = Vec::new();
    for name in header
        .get("Directories")
        .into_iter()
        .chain(header.get("ScaledDirectories"))
        .flat_map(|value| value.split(','))
        .map(str::trim)
    {
        if directories.len() >= MAX_DIRECTORIES {
            return None;
        }
        if name.is_empty()
            || name.contains('\\')
            || !Path::new(name)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            continue;
        }
        let Some(values) = groups.get(name) else {
            continue;
        };
        let number = |key, default| {
            values
                .get(key)
                .map_or(Some(default), |value| value.parse::<u32>().ok())
        };
        let Some(size) = number("Size", 0).filter(|size| (1..=4096).contains(size)) else {
            continue;
        };
        let Some(scale) = number("Scale", 1).filter(|scale| (1..=16).contains(scale)) else {
            continue;
        };
        let (minimum, maximum) = match values.get("Type").copied().unwrap_or("Threshold") {
            "Fixed" => (size, size),
            "Scalable" => (number("MinSize", size)?, number("MaxSize", size)?),
            "Threshold" => {
                let threshold = number("Threshold", 2)?;
                (
                    size.saturating_sub(threshold),
                    size.saturating_add(threshold),
                )
            }
            _ => continue,
        };
        if minimum > maximum || maximum > 8192 {
            continue;
        }
        let minimum = minimum * scale;
        let maximum = maximum * scale;
        let distance = if ICON_SIZE < minimum {
            minimum - ICON_SIZE
        } else {
            ICON_SIZE.saturating_sub(maximum)
        };
        // At scale 1, exact-size matches precede closest-size matches at other
        // scales, as specified by the freedesktop lookup algorithm.
        let exact = scale == 1 && distance == 0;
        directories.push((!exact, distance, name.to_owned()));
    }
    directories.sort_by_key(|(inexact, distance, _)| (*inexact, *distance));
    let inherits = header
        .get("Inherits")
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|name| safe_theme(name))
        .take(MAX_THEMES)
        .map(str::to_owned)
        .collect();
    Some(Theme {
        directories: directories.into_iter().map(|(_, _, name)| name).collect(),
        inherits,
    })
}

fn decode_file(path: &Path) -> Option<AppIcon> {
    let extension = path.extension()?.to_str()?;
    if !matches!(extension, "png" | "svg") {
        return None;
    }
    let bytes = read_bytes(path, MAX_ASSET_BYTES)?;
    let (width, height, rgba) = match extension {
        "png" => decode_png(&bytes)?,
        "svg" => decode_svg(&bytes)?,
        _ => return None,
    };
    let dominant = dominant_color(&rgba)?;
    let (width, height, rgba) = shrink(width, height, rgba)?;
    Some(AppIcon {
        width: width as usize,
        height: height as usize,
        rgba,
        dominant,
    })
}

fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: 16 * 1024 * 1024,
        },
    );
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let header = decoder.read_header_info().ok()?;
    if header.width == 0
        || header.height == 0
        || header.width > MAX_RASTER_SIDE
        || header.height > MAX_RASTER_SIDE
    {
        return None;
    }
    let mut reader = decoder.read_info().ok()?;
    let mut data = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut data).ok()?;
    data.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => data,
        color => {
            let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
            let channels = match color {
                png::ColorType::Rgb => 3,
                png::ColorType::GrayscaleAlpha => 2,
                png::ColorType::Grayscale => 1,
                _ => return None,
            };
            for pixel in data.chunks_exact(channels) {
                let value = match color {
                    png::ColorType::Rgb => [pixel[0], pixel[1], pixel[2], 255],
                    png::ColorType::GrayscaleAlpha => [pixel[0], pixel[0], pixel[0], pixel[1]],
                    _ => [pixel[0], pixel[0], pixel[0], 255],
                };
                rgba.extend_from_slice(&value);
            }
            rgba
        }
    };
    Some((info.width, info.height, rgba))
}

fn decode_svg(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let text = std::str::from_utf8(bytes).ok()?;
    // roxmltree can accept empty external DTD declarations without resolving
    // them; reject those too rather than treating an external reference as safe.
    if text.contains("<!DOCTYPE") {
        return None;
    }
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 16_384,
        },
    )
    .ok()?;
    for node in document.descendants() {
        if node.is_pi() || node.ancestors().take(65).count() > 64 {
            return None;
        }
        if !node.is_element() {
            continue;
        }
        if matches!(
            node.tag_name().name(),
            "script" | "foreignObject" | "image" | "feImage"
        ) {
            return None;
        }
        for attribute in node.attributes() {
            let name = attribute.name();
            if name.starts_with("on")
                || (name == "href" && !local_fragment(attribute.value()))
                || (name == "base"
                    && attribute.namespace() == Some("http://www.w3.org/XML/1998/namespace"))
                || !safe_css_resources(attribute.value())
            {
                return None;
            }
        }
        if node.tag_name().name() == "style"
            && node
                .children()
                .filter_map(|child| child.text())
                .any(|text| !safe_css_resources(text))
        {
            return None;
        }
    }
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = usvg::Tree::from_xmltree(&document, &options).ok()?;
    let size = tree.size();
    if size.width() > MAX_SVG_SIDE || size.height() > MAX_SVG_SIDE {
        return None;
    }
    let scale = ICON_SIZE as f32 / size.width().max(size.height());
    let width = (size.width() * scale).round().max(1.0) as u32;
    let height = (size.height() * scale).round().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut rgba = pixmap.take();
    unpremultiply(&mut rgba);
    Some((width, height, rgba))
}

fn local_fragment(value: &str) -> bool {
    value
        .strip_prefix('#')
        .is_some_and(|id| !id.is_empty() && !id.chars().any(char::is_whitespace))
}

fn safe_css_resources(value: &str) -> bool {
    // Reject CSS escapes/comments/imports rather than accepting obfuscated URLs.
    // Namespace declarations are plain attributes, not fetched resources.
    if value.contains(['\\', '@']) || value.contains("/*") {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    let mut rest = lower.as_str();
    while let Some(index) = rest.find("url(") {
        rest = &rest[index + 4..];
        let Some(end) = rest.find(')') else {
            return false;
        };
        let target = rest[..end].trim().trim_matches(['\'', '"']);
        if !local_fragment(target) {
            return false;
        }
        rest = &rest[end + 1..];
    }
    true
}

fn shrink(width: u32, height: u32, mut rgba: Vec<u8>) -> Option<(u32, u32, Vec<u8>)> {
    if width.max(height) <= ICON_SIZE {
        return Some((width, height, rgba));
    }
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        for value in &mut pixel[..3] {
            *value = ((u16::from(*value) * alpha + 127) / 255) as u8;
        }
    }
    let source = tiny_skia::Pixmap::from_vec(rgba, tiny_skia::IntSize::from_wh(width, height)?)?;
    let scale = ICON_SIZE as f32 / width.max(height) as f32;
    let width = (width as f32 * scale).round().max(1.0) as u32;
    let height = (height as f32 * scale).round().max(1.0) as u32;
    let mut target = tiny_skia::Pixmap::new(width, height)?;
    target.draw_pixmap(
        0,
        0,
        source.as_ref(),
        &tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        },
        tiny_skia::Transform::from_scale(scale, scale),
        None,
    );
    let mut rgba = target.take();
    unpremultiply(&mut rgba);
    Some((width, height, rgba))
}

pub(crate) fn unpremultiply(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for value in &mut pixel[..3] {
            *value = (u32::from(*value) * 255 + alpha / 2)
                .checked_div(alpha)
                .unwrap_or(0)
                .min(255) as u8;
        }
    }
}

/// Alpha-weighted hue clusters avoid averaging a multicolored logo into gray.
/// Prefer visible chromatic artwork even against a large neutral background;
/// neutral-only artwork retains its most prevalent source shade.
pub(crate) fn dominant_color(rgba: &[u8]) -> Option<[u8; 3]> {
    let mut clusters = [[0_u64; 4]; 24];
    let mut neutrals = [[0_u64; 4]; 16];
    for pixel in rgba.as_chunks::<4>().0 {
        let alpha = u64::from(pixel[3]);
        if alpha < 16 {
            continue;
        }
        let [r, g, b] = [pixel[0], pixel[1], pixel[2]];
        let maximum = r.max(g).max(b);
        let minimum = r.min(g).min(b);
        let delta = maximum - minimum;
        let cluster = if delta >= 24 && u16::from(delta) * 4 >= u16::from(maximum) && maximum >= 40
        {
            let [r, g, b] = [f32::from(r), f32::from(g), f32::from(b)];
            let hue = if maximum == pixel[0] {
                (g - b) / f32::from(delta)
            } else if maximum == pixel[1] {
                (b - r) / f32::from(delta) + 2.0
            } else {
                (r - g) / f32::from(delta) + 4.0
            };
            // Center red at zero; neighboring red shades on either side wrap
            // into the same cluster rather than competing across the hue seam.
            &mut clusters[((hue * 4.0).round() as i32).rem_euclid(24) as usize]
        } else {
            &mut neutrals[usize::from(maximum) / 16]
        };
        cluster[0] += alpha;
        for channel in 0..3 {
            cluster[channel + 1] += u64::from(pixel[channel]) * alpha;
        }
    }
    // Explicit tie order is independent of hash seeds, filesystem order or time.
    let most = |buckets: &[[u64; 4]]| *buckets.iter().max_by_key(|bucket| bucket[0]).unwrap();
    let chromatic = most(&clusters);
    let chosen = if chromatic[0] > 0 {
        chromatic
    } else {
        most(&neutrals)
    };
    if chosen[0] == 0 {
        return None;
    }
    Some(std::array::from_fn(|channel| {
        ((chosen[channel + 1] + chosen[0] / 2) / chosen[0]) as u8
    }))
}
