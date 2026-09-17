//! Versioned, validated room configuration. Runtime permission is never persisted.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::Ipv4Addr,
    path::{Path, PathBuf},
};

pub const MAX_LEDS: usize = 8192;
pub const MAX_SCREENS: usize = 16;
pub const MAX_POINTS: usize = 64;
pub const MAX_RULES: usize = 128;
pub const MAX_ROOM_VERTICES: usize = 12;
pub const CURRENT_CONFIG_VERSION: u32 = 2;
const MAX_CONFIG_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub device: DeviceConfig,
    pub room: Room,
    pub rules: Vec<Rule>,
    pub brightness: f32,
    pub reduced_motion: bool,
    pub notifications_enabled: bool,
    pub media_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceConfig {
    pub address: Ipv4Addr,
    pub led_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Room {
    pub width: f32,
    pub depth: f32,
    pub height: f32,
    /// Empty retains the rectangular room; otherwise normalized counterclockwise corners.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outline: Vec<[f32; 2]>,
    pub screens: Vec<Screen>,
    /// Vertices in physical LED order, from LED 0 to the last LED.
    pub strip: Vec<Point>,
    pub reverse: bool,
    /// Empty uses visual path proportions; otherwise one ascending LED index per vertex.
    #[serde(default)]
    pub led_anchors: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Point {
    pub fn distance(self, other: Self) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2) + (self.z - other.z).powi(2))
            .sqrt()
    }
    /// At least one centimetre apart, allowing only f32 coordinate roundoff.
    /// Shared by outline/strip validation and editing so a legal wall can be routed.
    pub fn separated_from(self, other: Self) -> bool {
        let a = [self.x, self.y, self.z].map(f64::from);
        let b = [other.x, other.y, other.z].map(f64::from);
        let squared: f64 = (0..3).map(|i| (a[i] - b[i]).powi(2)).sum();
        let minimum = f64::from(0.01_f32);
        if !squared.is_finite() {
            return false;
        }
        if squared >= minimum * minimum {
            return true;
        }
        let rounding: f64 = (0..3)
            .filter(|&i| a[i] != b[i])
            .map(|i| (2.0 * f64::from(f32::EPSILON) * a[i].abs().max(b[i].abs())).powi(2))
            .sum();
        squared.sqrt() + rounding.sqrt() >= minimum
    }
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
            z: self.z + (other.z - self.z) * t,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Screen {
    pub id: u32,
    pub name: String,
    pub position: Point,
    pub width: f32,
    pub angle: f32,
    #[serde(default)]
    pub connector: Option<String>,
    #[serde(default = "default_aspect")]
    pub aspect_ratio: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Exact case-insensitive desktop-entry ID or application name. "*" is the fallback.
    pub application: String,
    pub screen_id: u32,
    pub color: [u8; 3],
    pub duration: f32,
    /// Radius in the selected range unit: physical metres or device LED indices.
    pub spread: f32,
    pub enabled: bool,
    #[serde(default)]
    pub options: RuleOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    Glow,
    Ripple,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RangeUnit {
    Room,
    Leds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationMode {
    OneOff,
    Persistent,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradientStop {
    pub position: f32,
    pub color: [u8; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuleOptions {
    pub notifications: bool,
    pub media: bool,
    pub minimum_urgency: u8,
    pub intensity: f32,
    pub fade: bool,
    pub critical_accent: bool,
    pub effect: EffectKind,
    pub range_unit: RangeUnit,
    /// Empty keeps the rule's single color; otherwise positions span exactly 0–1.
    pub gradient: Vec<GradientStop>,
    pub mode: NotificationMode,
    /// Normalized device LED index. None follows the nearest LED to the assigned display.
    pub position: Option<f32>,
}

impl Default for RuleOptions {
    fn default() -> Self {
        Self {
            notifications: true,
            media: true,
            minimum_urgency: 0,
            intensity: 1.0,
            fade: true,
            critical_accent: true,
            effect: EffectKind::Glow,
            range_unit: RangeUnit::Room,
            gradient: Vec::new(),
            mode: NotificationMode::OneOff,
            position: None,
        }
    }
}

fn default_aspect() -> f32 {
    16.0 / 9.0
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CURRENT_CONFIG_VERSION,
            device: DeviceConfig {
                address: Ipv4Addr::LOCALHOST,
                led_count: 600,
            },
            room: Room {
                width: 5.0,
                depth: 4.0,
                height: 2.5,
                outline: Vec::new(),
                screens: vec![Screen {
                    id: 1,
                    name: "Main screen".into(),
                    position: Point {
                        x: 2.5,
                        y: 0.8,
                        z: 1.2,
                    },
                    width: 0.7,
                    angle: 0.0,
                    connector: None,
                    aspect_ratio: default_aspect(),
                }],
                strip: vec![
                    Point {
                        x: 0.0,
                        y: 0.0,
                        z: 1.2,
                    },
                    Point {
                        x: 5.0,
                        y: 0.0,
                        z: 1.2,
                    },
                    Point {
                        x: 5.0,
                        y: 4.0,
                        z: 1.2,
                    },
                    Point {
                        x: 0.0,
                        y: 4.0,
                        z: 1.2,
                    },
                ],
                reverse: false,
                led_anchors: Vec::new(),
            },
            rules: vec![Rule {
                application: "*".into(),
                screen_id: 1,
                color: [80, 175, 220],
                duration: 4.0,
                spread: 1.2,
                enabled: true,
                options: RuleOptions::default(),
            }],
            brightness: 0.15,
            reduced_motion: false,
            notifications_enabled: true,
            media_enabled: true,
        }
    }
}

impl DeviceConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.address.is_private() || self.address.is_loopback() || self.address.is_link_local(),
            "Use a private LAN, link-local or loopback IPv4 address"
        );
        ensure!(
            self.address.octets()[3] != 255 && !self.address.is_unspecified(),
            "Broadcast and unspecified addresses are not device targets"
        );
        ensure!(
            (1..=MAX_LEDS).contains(&self.led_count),
            "LED count must be 1–{MAX_LEDS}"
        );
        Ok(())
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == CURRENT_CONFIG_VERSION,
            "Unsupported configuration version {}",
            self.version
        );
        self.device.validate()?;
        for (name, value) in [
            ("Room width", self.room.width),
            ("Room depth", self.room.depth),
            ("Room height", self.room.height),
        ] {
            ensure!(
                value.is_finite() && (0.5..=50.0).contains(&value),
                "{name} must be 0.5–50 metres"
            );
        }
        self.room.validate_outline()?;
        ensure!(
            self.brightness.is_finite() && (0.0..=1.0).contains(&self.brightness),
            "Brightness must be between 0 and 100%"
        );
        ensure!(
            !self.room.screens.is_empty() && self.room.screens.len() <= MAX_SCREENS,
            "Configure 1–{MAX_SCREENS} screens"
        );
        ensure!(
            (2..=MAX_POINTS).contains(&self.room.strip.len()),
            "The strip needs 2–{MAX_POINTS} points"
        );
        let anchors = &self.room.led_anchors;
        ensure!(
            anchors.is_empty()
                || (anchors.len() == self.room.strip.len()
                    && anchors.first() == Some(&0)
                    && anchors.last() == Some(&(self.device.led_count - 1))
                    && anchors.windows(2).all(|pair| pair[0] < pair[1])),
            "LED allocation must ascend from the first to the last LED, one anchor per point"
        );
        let mut ids = HashSet::new();
        for screen in &self.room.screens {
            ensure!(ids.insert(screen.id), "Screen IDs must be unique");
            validate_label(&screen.name, "Screen name")?;
            if let Some(connector) = &screen.connector {
                validate_label(connector, "Display connector")?;
            }
            ensure!(
                screen.aspect_ratio.is_finite() && (0.1..=10.0).contains(&screen.aspect_ratio),
                "Display aspect ratio must be between 0.1 and 10"
            );
            self.validate_point(screen.position)?;
            ensure!(
                screen.width.is_finite() && (0.1..=5.0).contains(&screen.width),
                "Screen width must be 0.1–5 metres"
            );
            ensure!(
                screen.angle.is_finite() && (-180.0..=180.0).contains(&screen.angle),
                "Screen angle must be -180–180 degrees"
            );
        }
        for &point in &self.room.strip {
            self.validate_point(point)?;
        }
        for pair in self.room.strip.windows(2) {
            ensure!(
                self.room.contains_floor_segment(pair[0], pair[1]),
                "A strip segment leaves the room footprint; move it or reshape the room"
            );
            ensure!(
                pair[0].separated_from(pair[1]),
                "Adjacent strip points must be at least 1 cm apart"
            );
        }
        ensure!(
            self.rules.len() <= MAX_RULES,
            "At most {MAX_RULES} application rules are supported"
        );
        let mut applications = HashSet::new();
        for rule in &self.rules {
            validate_label(&rule.application, "Application")?;
            ensure!(
                rule.options.minimum_urgency <= 2,
                "Minimum urgency must be low, normal or critical"
            );
            ensure!(
                rule.options.intensity.is_finite() && (0.0..=1.0).contains(&rule.options.intensity),
                "Rule intensity must be between 0 and 100%"
            );
            ensure!(
                applications.insert(rule.application.to_lowercase()),
                "Application rules must be unique (case-insensitive)"
            );
            ensure!(
                ids.contains(&rule.screen_id),
                "An application rule refers to a missing screen"
            );
            ensure!(
                rule.duration.is_finite() && (0.5..=15.0).contains(&rule.duration),
                "Notification duration must be 0.5–15 seconds"
            );
            ensure!(
                rule.options
                    .position
                    .is_none_or(|position| position.is_finite() && (0.0..=1.0).contains(&position)),
                "Notification position must lie on the strip"
            );
            match rule.options.range_unit {
                RangeUnit::Room => ensure!(
                    rule.spread.is_finite() && (0.1..=20.0).contains(&rule.spread),
                    "Light range must be 0.1–20 metres"
                ),
                RangeUnit::Leds => ensure!(
                    rule.spread.is_finite()
                        && (1.0..=MAX_LEDS as f32).contains(&rule.spread)
                        && rule.spread.fract() == 0.0,
                    "Light range must be a whole number of 1–{MAX_LEDS} LEDs"
                ),
            }
            let gradient = &rule.options.gradient;
            ensure!(
                gradient.is_empty()
                    || ((2..=8).contains(&gradient.len())
                        && gradient.first().is_some_and(|stop| stop.position == 0.0)
                        && gradient.last().is_some_and(|stop| stop.position == 1.0)
                        && gradient.iter().all(|stop| stop.position.is_finite()
                            && (0.0..=1.0).contains(&stop.position))
                        && gradient
                            .windows(2)
                            .all(|pair| pair[0].position < pair[1].position)),
                "A gradient needs 2–8 strictly ordered color stops, starting at 0 and ending at 1"
            );
        }
        Ok(())
    }

    fn validate_point(&self, p: Point) -> Result<()> {
        ensure!(
            p.x.is_finite() && p.y.is_finite() && p.z.is_finite(),
            "Positions must be finite numbers"
        );
        ensure!(
            self.room.contains_floor(p) && (0.0..=self.room.height).contains(&p.z),
            "A position is outside the room; move it or enlarge the room"
        );
        Ok(())
    }

    /// An explicitly disabled matching rule suppresses the source, including fallback.
    pub fn rule_for(&self, application: &str) -> Option<&Rule> {
        self.rule_index_for(application)
            .map(|index| &self.rules[index])
    }

    pub(crate) fn rule_index_for(&self, application: &str) -> Option<usize> {
        let index = self
            .rules
            .iter()
            .position(|rule| rule.application.eq_ignore_ascii_case(application))
            .or_else(|| self.rules.iter().position(|rule| rule.application == "*"))?;
        self.rules[index].enabled.then_some(index)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path).context("Cannot open configuration")?;
        let mut bytes = Vec::new();
        file.take(MAX_CONFIG_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("Cannot read configuration")?;
        ensure!(
            bytes.len() as u64 <= MAX_CONFIG_BYTES,
            "Configuration exceeds 256 KiB"
        );
        let mut config: Self =
            serde_json::from_slice(&bytes).context("Invalid configuration JSON")?;
        if config.version == 1 {
            ensure!(
                config.room.outline.is_empty(),
                "Version 1 configurations cannot contain a room outline"
            );
            // Migration is in memory only: placements, allocations and rules stay untouched.
            config.version = CURRENT_CONFIG_VERSION;
        }
        config.validate()?;
        Ok(config)
    }

    /// Same-directory atomic replacement; pre-rename failures retain the previous file.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        write_atomic(path, &serde_json::to_vec_pretty(self)?)
    }
}

/// Shared same-directory replacement for setup and non-authoritative UI preferences.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).context("Cannot create setup directory")?;
    let name = path.file_name().context("Setup data needs a filename")?;
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(".{}.tmp", std::process::id()));
    let temp = parent.join(temp_name);
    let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp)
        .context("Cannot create temporary setup data; remove a stale .tmp file if a previous save crashed")?;
    let result = (|| -> Result<()> {
        file.write_all(bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.context("Cannot persist setup data")
}

fn validate_label(value: &str, name: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value == value.trim()
            && value.len() <= 128
            && !value.chars().any(char::is_control),
        "{name} must be 1–128 bytes with no control characters or surrounding spaces"
    );
    Ok(())
}

pub fn default_path() -> Result<PathBuf> {
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME") {
        let base = PathBuf::from(value);
        if base.is_absolute() {
            return Ok(base.join("ledalert/config.json"));
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let base = PathBuf::from(home);
        if base.is_absolute() {
            return Ok(base.join(".config/ledalert/config.json"));
        }
    }
    bail!("Set HOME or an absolute XDG_CONFIG_HOME, or supply --config PATH")
}
