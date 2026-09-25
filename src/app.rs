mod applications;
mod demos;
mod effects;
mod manipulate;
mod outline;
mod rules;
mod runtime;
mod scene;
mod setup;
mod targeting;
mod widgets;

use demos::DemoMode;
use widgets::{Action, Icon, decimal_parser, distance_slider};

use crate::{
    config::{
        Config, DeviceConfig, EffectKind, GradientStop, MAX_LEDS, MAX_POINTS, MAX_ROOM_VERTICES,
        MAX_RULES, MAX_SCREENS, NotificationMode, Point, RangeUnit, Rule, RuleOptions, Screen,
    },
    desktop::{DesktopMonitor, DesktopState, NotificationEvent},
    displays::{self, DetectedDisplay},
    engine::{Engine, rule_anchor},
    guidance::{Guidance, StopReason},
    history::{EditorHistory, EditorSnapshot},
    identity::{ACCENT, CANVAS, CORAL, ERROR, FLOOR, INK, Identity, LINE, MUTED, PANEL, WARNING},
    preferences::Preferences,
    spatial::{
        DEFAULT_PITCH, DEFAULT_YAW, Projection, closest_on_segment, screen_corners, wall_path,
    },
    taskbar::{self, PinnedApp},
    wled::{self, DeviceInfo, WledOutput},
};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind,
    Vec2,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq)]
enum Inspector {
    Room,
    Displays,
    Strip,
    Rules,
}
impl Inspector {
    const ALL: [Self; 4] = [Self::Room, Self::Displays, Self::Strip, Self::Rules];
    fn index(self) -> usize {
        match self {
            Self::Room => 0,
            Self::Displays => 1,
            Self::Strip => 2,
            Self::Rules => 3,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Room => "Room",
            Self::Displays => "Displays",
            Self::Strip => "Strip",
            Self::Rules => "Rules",
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Selection {
    Screen(usize),
    Point(usize),
    Segment(usize),
}

struct ObjectDrag {
    selection: Selection,
    point: Point,
    projection: Projection,
    pointer: Pos2,
    vertical: bool,
}

struct DisplayDrag {
    index: usize,
    screen: Screen,
    projection: Projection,
    pointer: Pos2,
    rotation: bool,
}

pub struct LedAlertApp {
    state: Arc<Mutex<AppState>>,
    stop: mpsc::SyncSender<()>,
    worker: Option<JoinHandle<()>>,
    #[cfg(windows)]
    notification_permission: crate::desktop::NotificationPermission,
}

struct AppState {
    identity: Identity,
    preferences: Preferences,
    preferences_path: PathBuf,
    guide: bool,
    sidebar_visible: bool,
    scene_clock: Instant,
    demo_mode: DemoMode,
    demo_request: Option<DemoMode>,
    finish_requested: bool,
    demo_next: Instant,
    demo_rng: u64,
    demo_order: Vec<usize>,
    guide_started: Instant,
    guidance: Guidance,
    guidance_consent: Option<DeviceConfig>,
    pinned_scan: Option<mpsc::Receiver<Result<Vec<PinnedApp>, String>>>,
    pinned: Vec<PinnedApp>,
    pinned_selected: Vec<bool>,
    pinned_error: Option<String>,
    application_assets: HashMap<String, applications::AppVisual>,
    application_scan: Option<mpsc::Receiver<Vec<applications::Resolution>>>,
    application_requests: Vec<String>,
    pending_rule: Option<(String, u32)>,
    show_suggestions: bool,
    example_config: Option<Config>,
    example_next: Instant,
    example_index: usize,
    object_drag: Option<ObjectDrag>,
    display_drag: Option<DisplayDrag>,
    camera_yaw: f32,
    camera_pitch: f32,
    orbit_origin: Option<(Pos2, f32, f32)>,
    snap_rotation: bool,
    placing_walls: bool,
    placement_open: bool,
    outline_editor: outline::OutlineEditor,
    selected_walls: Vec<usize>,
    config: Config,
    saved: Option<Config>,
    history: EditorHistory,
    path: PathBuf,
    load_error: Option<String>,
    validation_error: Option<String>,
    notice: String,
    notice_until: Instant,
    address: String,
    inspector: Inspector,
    selection: Option<Selection>,
    selected_rule: usize,
    enabled: bool,
    quiet: bool,
    notification_gate: runtime::NotificationGate,
    desktop: DesktopMonitor,
    #[cfg(windows)]
    notification_permission_requested: bool,
    desktop_state: DesktopState,
    desktop_poll: Instant,
    output: WledOutput,
    engine: Engine,
    preview: Engine,
    recent_applications: Vec<String>,
    probe: Option<mpsc::Receiver<Result<DeviceInfo, String>>>,
    probe_address: std::net::Ipv4Addr,
    connected: Option<DeviceInfo>,
    probe_error: Option<String>,
    display_scan: Option<mpsc::Receiver<Result<Vec<DetectedDisplay>, String>>>,
    detected: Vec<DetectedDisplay>,
    display_error: Option<String>,
    auto_import: bool,
    adding_rule: bool,
    new_application: String,
    show_settings: bool,
    resize_origin: Option<(Config, Projection, Pos2, usize)>,
    confirm_close: bool,
}

impl AppState {
    fn new(ctx: &egui::Context, path: PathBuf) -> anyhow::Result<Self> {
        let identity = Identity::install(ctx)?;
        let (config, saved, load_error) = if path.exists() {
            match Config::load(&path) {
                Ok(config) => (config.clone(), Some(config), None),
                Err(error) => (Config::default(), None, Some(format!("{error:#}"))),
            }
        } else {
            (Config::default(), None, None)
        };
        let auto_import = saved.is_none() && load_error.is_none();
        let preferences_path = Preferences::path_for(&path)?;
        let (mut preferences, preference_error) = match Preferences::load(&preferences_path) {
            Ok(preferences) => (preferences, None),
            Err(error) => (Preferences::default(), Some(error.to_string())),
        };
        let guide = load_error.is_none()
            && !preferences.guide_dismissed
            && (auto_import || preferences.guide_started);
        if guide && !preferences.guide_started {
            preferences.guide_started = true;
        }
        let inspector = if guide {
            Inspector::ALL[preferences.guide_step as usize]
        } else {
            Inspector::Room
        };
        let desktop = DesktopMonitor::start()?;
        let desktop_state = desktop.snapshot();
        let address = config.device.address.to_string();
        let probe_address = config.device.address;
        let placement_open = saved.is_none() && !preferences.strip_placed;
        let sidebar_visible =
            !preferences.setup_complete || saved.is_none() || guide || load_error.is_some();
        // Older saved setups have no connection history; validate them read-only once.
        let reconnect = saved.is_some()
            && load_error.is_none()
            && preferences
                .verified_address
                .is_none_or(|address| address == config.device.address);
        let mut app = Self {
            identity,
            preferences,
            preferences_path,
            guide,
            sidebar_visible,
            scene_clock: Instant::now(),
            demo_mode: DemoMode::Idle,
            demo_request: None,
            finish_requested: false,
            demo_next: Instant::now(),
            demo_rng: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |elapsed| elapsed.as_nanos() as u64 | 1),
            demo_order: Vec::with_capacity(MAX_RULES),
            guide_started: Instant::now(),
            guidance: Guidance::default(),
            guidance_consent: None,
            pinned_scan: None,
            pinned: Vec::new(),
            pinned_selected: Vec::new(),
            pinned_error: None,
            application_assets: HashMap::new(),
            application_scan: None,
            application_requests: Vec::new(),
            pending_rule: None,
            show_suggestions: guide && inspector == Inspector::Rules,
            example_config: None,
            example_next: Instant::now(),
            example_index: 0,
            object_drag: None,
            display_drag: None,
            camera_yaw: DEFAULT_YAW,
            camera_pitch: DEFAULT_PITCH,
            orbit_origin: None,
            snap_rotation: true,
            placing_walls: false,
            placement_open,
            outline_editor: outline::OutlineEditor::default(),
            selected_walls: Vec::with_capacity(MAX_ROOM_VERTICES),
            history: EditorHistory::new(config.clone(), address.clone()),
            engine: Engine::new(&config)?,
            preview: Engine::new(&config)?,
            config,
            saved,
            path,
            load_error,
            validation_error: None,
            notice: String::new(),
            notice_until: Instant::now(),
            address,
            inspector,
            selection: None,
            selected_rule: 0,
            enabled: false,
            quiet: false,
            notification_gate: runtime::NotificationGate::new(Instant::now()),
            desktop,
            #[cfg(windows)]
            notification_permission_requested: false,
            desktop_state,
            desktop_poll: Instant::now(),
            output: WledOutput::start()?,
            recent_applications: Vec::new(),
            probe: None,
            probe_address,
            connected: None,
            probe_error: None,
            display_scan: None,
            detected: Vec::new(),
            display_error: None,
            auto_import,
            adding_rule: false,
            new_application: String::new(),
            show_settings: false,
            resize_origin: None,
            confirm_close: false,
        };
        app.scan_displays();
        app.scan_pinned();
        for index in 0..app.config.rules.len() {
            let id = app.config.rules[index].application.clone();
            app.queue_application(&id);
        }
        if reconnect {
            app.begin_probe();
        }
        if guide {
            app.persist_preferences();
        }
        if let Some(error) = preference_error {
            app.notify(error);
        }
        Ok(app)
    }

    fn notify(&mut self, text: impl Into<String>) {
        self.notice = text.into();
        self.notice_until = Instant::now() + Duration::from_secs(7);
    }
    fn valid(&self) -> Result<(), String> {
        if self.load_error.is_some() {
            return Err("Resolve the saved-setup error first".into());
        }
        if self.address.parse::<std::net::Ipv4Addr>().ok() != Some(self.config.device.address) {
            return Err("Enter a valid local IPv4 address".into());
        }
        self.validation_error.clone().map_or(Ok(()), Err)
    }
    fn changed(&mut self) {
        self.notice.clear();
        self.stop_demos();
        let was_invalid = self.validation_error.is_some();
        self.validation_error = self.config.validate().err().map(|e| e.to_string());
        if was_invalid || self.validation_error.is_some() {
            // An invalid draft can be fixed between producer ticks. Fence queued
            // notifications at both editor transitions, not just at the next tick.
            self.notification_gate.discard_before(Instant::now());
        }
        if self.inspector == Inspector::Displays {
            self.auto_import = false;
        }
        if self.enabled
            && (self.address.parse().ok() != Some(self.probe_address)
                || self.config.device.address != self.probe_address
                || self
                    .connected
                    .as_ref()
                    .is_none_or(|info| info.led_count != self.config.device.led_count))
        {
            self.enabled = false;
            self.engine.clear();
            self.notify("Device changed; reconnect and enable lighting for this strip.");
        }
        self.engine.clear_transients();
        self.preview.clear();
        self.example_config = None;
        if self.address.parse::<std::net::Ipv4Addr>().ok() != Some(self.config.device.address) {
            self.stop_guidance(StopReason::TargetChanged);
        }
    }
    fn record_discrete(&mut self) {
        self.history.finish_group();
        if self.history.record(&self.config, &self.address, None) {
            self.changed();
        }
    }
    fn restore(&mut self, state: EditorSnapshot) {
        self.stop_guidance(StopReason::History);
        self.object_drag = None;
        self.display_drag = None;
        self.orbit_origin = None;
        self.placing_walls = false;
        self.selected_walls.clear();
        self.outline_editor.cancel_drag();
        if state.config.device != self.config.device || state.address != self.address {
            self.connected = None;
            self.probe_error = None;
        }
        self.config = state.config;
        self.address = state.address;
        self.probe = None;
        self.resize_origin = None;
        self.selection = self.selection.filter(|selection| match selection {
            Selection::Screen(i) => *i < self.config.room.screens.len(),
            Selection::Point(i) => *i < self.config.room.strip.len(),
            Selection::Segment(i) => i + 1 < self.config.room.strip.len(),
        });
        self.selected_rule = self
            .selected_rule
            .min(self.config.rules.len().saturating_sub(1));
        self.changed();
    }
    fn undo(&mut self) {
        if let Some(state) = self.history.undo() {
            self.restore(state);
            self.notify("Undone");
        }
    }
    fn redo(&mut self) {
        if let Some(state) = self.history.redo() {
            self.restore(state);
            self.notify("Redone");
        }
    }
    fn save_config(&mut self) {
        match self
            .valid()
            .and_then(|()| self.config.save(&self.path).map_err(|e| format!("{e:#}")))
        {
            Ok(()) => {
                self.saved = Some(self.config.clone());
                self.history.finish_group();
                self.remember_connection();
                self.notify("Saved");
            }
            Err(error) => self.notify(format!("Save failed: {error}")),
        }
    }
    fn stop_local_examples(&mut self) {
        self.preview.clear();
        self.example_config = None;
    }
    fn scan_displays(&mut self) {
        if self.display_scan.is_some() {
            return;
        }
        self.display_error = None;
        let (tx, rx) = mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("ledalert-displays".into())
            .spawn(move || {
                let _ = tx.send(displays::discover().map_err(|e| format!("{e:#}")));
            }) {
            Ok(_) => self.display_scan = Some(rx),
            Err(error) => self.display_error = Some(error.to_string()),
        }
    }
    fn use_detected(&mut self, replace_example: bool) {
        self.auto_import = false;
        self.history.finish_group();
        match displays::import_displays(&mut self.config, &self.detected, replace_example) {
            Ok(count) => {
                self.record_discrete();
                self.selection = None;
                if count > 0 {
                    self.notify(format!(
                        "{count} displays added. Drag to place them in your room."
                    ));
                } else {
                    self.notify("Detected displays are already in this room.");
                }
            }
            Err(error) => self.notify(error.to_string()),
        }
    }
    fn begin_probe(&mut self) {
        self.stop_guidance(StopReason::TargetChanged);
        self.stop_demos();
        self.stop_local_examples();
        self.enabled = false;
        self.engine.clear();
        self.connected = None;
        self.probe_error = None;
        self.notice.clear();
        let device = self.config.device;
        let (tx, rx) = mpsc::sync_channel(1);
        self.probe_address = device.address;
        match std::thread::Builder::new()
            .name("ledalert-probe".into())
            .spawn(move || {
                let _ = tx.send(wled::probe(device).map_err(|e| format!("{e:#}")));
            }) {
            Ok(_) => self.probe = Some(rx),
            Err(error) => {
                self.probe_error = Some(format!("Cannot connect: {error}"));
                self.select_step(Inspector::Strip);
            }
        }
    }

    fn resize_room(&mut self, width: f32, depth: f32, height: f32) {
        apply_room_size(&mut self.config, None, width, depth, height);
    }
    fn add_screen(&mut self) {
        let Some(id) = self
            .config
            .room
            .screens
            .iter()
            .map(|s| s.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
        else {
            self.notify("No display IDs available");
            return;
        };
        let n = self.config.room.screens.len() + 1;
        let mut position = Point {
            x: self.config.room.width / 2.0,
            y: self.config.room.depth / 2.0,
            z: self.config.room.height / 2.0,
        };
        if !self.config.room.contains_floor(position)
            && let Some(mesh) = self.config.room.floor_mesh()
        {
            let [a, b, c] = mesh.triangles[0].map(|i| mesh.vertices[i]);
            position.x = (a.x + b.x + c.x) / 3.0;
            position.y = (a.y + b.y + c.y) / 3.0;
        }
        self.config.room.screens.push(Screen {
            id,
            name: format!("Screen {n}"),
            position,
            width: 0.7,
            angle: 0.0,
            connector: None,
            aspect_ratio: 16.0 / 9.0,
        });
        self.selection = Some(Selection::Screen(n - 1));
    }
    fn screen_controls(&mut self, ui: &mut egui::Ui, index: usize) {
        let bounds = (
            self.config.room.width,
            self.config.room.depth,
            self.config.room.height,
        );
        let Some(screen) = self.config.room.screens.get_mut(index) else {
            return;
        };
        ui.heading("Display");
        ui.add(
            egui::TextEdit::singleline(&mut screen.name)
                .char_limit(128)
                .desired_width(f32::INFINITY),
        );
        if let Some(connector) = &screen.connector {
            ui.label(RichText::new(connector).small().color(MUTED));
        }
        ui.add_space(8.0);
        ui.add(distance_slider(&mut screen.width, 0.1..=bounds.0.clamp(0.1, 5.0)).text("Width"));
        ui.horizontal(|ui| {
            let old = screen.angle;
            scalar(ui, "Rotation", &mut screen.angle, -180.0..=180.0, "°");
            if old != screen.angle && self.snap_rotation && !ui.input(|i| i.modifiers.shift) {
                screen.angle = snapped_angle(screen.angle);
            }
        });
        ui.checkbox(&mut self.snap_rotation, "Snap to 45°")
            .on_hover_text(
                "Handles and typed angles snap to 45°. Turn off for fine rotation, or hold Shift when dragging or committing a typed angle.",
            );
        ui.add(distance_slider(&mut screen.position.z, 0.0..=bounds.2).text("Height"))
            .on_hover_text(
                "Height above the floor; also adjustable with the vertical handle in the room.",
            );
        egui::CollapsingHeader::new("Position & dimensions")
            .show(ui, |ui| point_controls(ui, &mut screen.position, bounds));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.config.room.screens.len() > 1,
                    egui::Button::new("Remove"),
                )
                .clicked()
            {
                self.remove_selection();
            }
        });
    }
    fn remove_selection(&mut self) {
        match self.selection {
            Some(Selection::Screen(i))
                if self.config.room.screens.len() > 1 && i < self.config.room.screens.len() =>
            {
                let removed = self.config.room.screens.remove(i).id;
                let first = self.config.room.screens[0].id;
                for rule in &mut self.config.rules {
                    if rule.screen_id == removed {
                        rule.screen_id = first;
                    }
                }
                self.selection = None;
                self.notify("Display removed; its rules use the first display.");
            }
            Some(Selection::Point(i))
                if self.config.room.strip.len() > 2 && i < self.config.room.strip.len() =>
            {
                if i > 0
                    && i + 1 < self.config.room.strip.len()
                    && !self.config.room.strip[i - 1].separated_from(self.config.room.strip[i + 1])
                {
                    self.notify("Keep this bend: its neighbouring points would overlap.");
                    return;
                }
                self.config.room.strip.remove(i);
                if !self.config.room.led_anchors.is_empty() {
                    self.config.room.led_anchors.remove(i);
                    self.config.room.led_anchors[0] = 0;
                    *self.config.room.led_anchors.last_mut().unwrap() =
                        self.config.device.led_count - 1;
                }
                self.selection = None;
            }
            _ => {}
        }
    }
    fn insert_point(&mut self, index: usize, point: Point) {
        let Some(pair) = self.config.room.strip.get(index..index.saturating_add(2)) else {
            return;
        };
        if pair.len() != 2 || !point.separated_from(pair[0]) || !point.separated_from(pair[1]) {
            return;
        }
        if self.config.room.strip.len() >= MAX_POINTS {
            return;
        }
        if !self.config.room.led_anchors.is_empty() {
            let a = self.config.room.led_anchors[index];
            let b = self.config.room.led_anchors[index + 1];
            if b - a < 2 {
                self.notify("Move the LED allocation handles apart before adding a point here.");
                return;
            }
            self.config
                .room
                .led_anchors
                .insert(index + 1, a + (b - a) / 2);
        }
        self.config.room.strip.insert(index + 1, point);
        self.selection = Some(Selection::Point(index + 1));
    }
    fn strip_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading(if self.connected.is_some() {
            "Your strip"
        } else {
            "Connect your strip"
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.address)
                        .hint_text("WLED IPv4 address")
                        .char_limit(15)
                        .desired_width(166.0),
                )
                .on_hover_text("Address of an already configured WLED device")
                .changed()
            {
                self.stop_guidance(StopReason::TargetChanged);
                if let Ok(address) = self.address.parse() {
                    self.config.device.address = address;
                }
                self.connected = None;
                self.probe_error = None;
                self.probe = None;
            }
            if ui
                .add_enabled(
                    !self.enabled
                        && self.probe.is_none()
                        && self.config.device.validate().is_ok()
                        && self.address.parse().ok() == Some(self.config.device.address),
                    egui::Button::new(if self.probe.is_some() {
                        "Connecting…"
                    } else {
                        "Connect"
                    }),
                )
                .on_hover_text(
                    "Read existing WLED configuration; no pixels or settings are written.",
                )
                .clicked()
            {
                self.begin_probe();
            }
        });
        if let Some(info) = &self.connected {
            ui.label(
                RichText::new(format!("{} LEDs · WLED {}", info.led_count, info.version))
                    .color(MUTED),
            );
            if info.live {
                ui.colored_label(ACCENT, "Another realtime source is active")
                    .on_hover_text("LedAlert will not take over an active controller.");
            }
        } else if let Some(error) = &self.probe_error {
            ui.colored_label(ERROR, error);
        }
        self.guidance_controls(ui);
        ui.add_space(18.0);
        if ui.add(Action::new(Icon::Reset, "Show starting placements in the room. Your current path stays unchanged until you choose a replacement.").label("Starting placements").selected(self.placement_open)).clicked() {
            self.placement_open = !self.placement_open;
            self.placing_walls = false;
            self.selected_walls.clear();
        }
        ui.add_space(12.0);
        self.strip_height_controls(ui);
        ui.add_space(20.0);
        ui.label("Alert brightness");
        ui.add(
            egui::Slider::new(&mut self.config.brightness, 0.0..=1.0)
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                .custom_parser(|text| decimal_parser(text.trim_end_matches('%')).map(|v| v / 100.0)),
        )
        .on_hover_text("Notification/media RGB cap. Live guidance uses its separate 10% cap; WLED's own current limit still applies.");
    }
    fn finish_strip_placement(&mut self) {
        self.placement_open = false;
        self.placing_walls = false;
        if !self.preferences.strip_placed {
            self.preferences.strip_placed = true;
            self.persist_preferences();
        }
    }
    fn proportional_anchors(&self) -> Vec<usize> {
        let points = &self.config.room.strip;
        let count = self.config.device.led_count;
        if count < points.len() {
            return Vec::new();
        }
        let total: f32 = points.windows(2).map(|p| p[0].distance(p[1])).sum();
        let mut result = vec![0];
        let mut distance = 0.0;
        for index in 1..points.len() - 1 {
            distance += points[index - 1].distance(points[index]);
            let approximate = (distance / total * (count - 1) as f32).round() as usize;
            result.push(approximate.clamp(result[index - 1] + 1, count - (points.len() - index)));
        }
        result.push(count - 1);
        result
    }
    fn add_rule(&mut self, application: String) {
        let application = application.trim();
        if application.is_empty() || self.config.rules.len() >= MAX_RULES {
            return;
        }
        if let Some(index) = self
            .config
            .rules
            .iter()
            .position(|rule| rule.application.eq_ignore_ascii_case(application))
        {
            self.selected_rule = index;
            self.adding_rule = false;
            return;
        }
        let screen_id = match self.selection {
            Some(Selection::Screen(index)) => {
                self.config.room.screens.get(index).map(|screen| screen.id)
            }
            _ => None,
        }
        .unwrap_or(self.config.room.screens[0].id);
        if application != "*"
            && self
                .application_assets
                .get(application)
                .is_none_or(|asset| matches!(asset, applications::AppVisual::Loading))
        {
            self.queue_application(application);
            self.pending_rule = Some((application.to_owned(), screen_id));
            self.adding_rule = false;
            self.new_application.clear();
            return;
        }
        self.insert_rule(application.to_owned(), screen_id);
    }

    fn insert_rule(&mut self, application: String, screen_id: u32) {
        if self.config.rules.len() >= MAX_RULES {
            return;
        }
        if let Some(index) = self
            .config
            .rules
            .iter()
            .position(|rule| rule.application.eq_ignore_ascii_case(&application))
        {
            self.selected_rule = index;
            return;
        }
        let screen_id = if self
            .config
            .room
            .screens
            .iter()
            .any(|screen| screen.id == screen_id)
        {
            screen_id
        } else {
            self.config.room.screens[0].id
        };
        let color = self
            .application_assets
            .get(&application)
            .and_then(applications::AppVisual::dominant)
            .unwrap_or([80, 175, 220]);
        self.config.rules.push(Rule {
            application,
            screen_id,
            color,
            duration: 4.0,
            spread: (self.config.room.width * 0.24).clamp(0.1, 20.0),
            enabled: true,
            options: RuleOptions::default(),
        });
        self.selected_rule = self.config.rules.len() - 1;
        self.adding_rule = false;
        self.new_application.clear();
        self.record_discrete();
    }

    fn strip_preview(&mut self, ui: &mut egui::Ui) {
        let local = self.sidebar_visible && self.preview.is_active();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if self.guidance.active() {
                    "Live guidance · selected area"
                } else if local {
                    "Local preview"
                } else if self.inspector == Inspector::Strip {
                    "LED allocation"
                } else {
                    "Strip preview"
                })
                .strong(),
            )
            .on_hover_text("Local visualization; not a measurement of physical brightness.");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} LEDs", self.config.device.led_count))
                        .small()
                        .color(MUTED),
                )
                .on_hover_text(if self.connected.is_some() {
                    "Count read from WLED"
                } else {
                    "Saved/example count; connect WLED to confirm"
                });
            });
        });
        let map = self.sidebar_visible && self.inspector == Inspector::Strip;
        let targeting = self.sidebar_visible && self.inspector == Inspector::Rules;
        let (area, rail_response) = ui.allocate_exact_size(
            Vec2::new(
                ui.available_width(),
                if !self.sidebar_visible || map || targeting {
                    74.0
                } else {
                    40.0
                },
            ),
            if map { Sense::click() } else { Sense::hover() },
        );
        let rail = Rect::from_min_max(
            Pos2::new(area.left() + 8.0, area.top() + 12.0),
            Pos2::new(area.right() - 8.0, area.top() + 30.0),
        );
        ui.painter().rect_filled(rail, 3.0, LINE);
        let pixels = if self.guidance.active() {
            self.guidance.frame()
        } else if local {
            self.preview.frame()
        } else {
            self.engine.frame()
        };
        // At most one draw per screen column; avoid thousands of overlapping subpixel rectangles.
        let columns = (rail.width().ceil() as usize).max(1).min(pixels.len());
        for col in 0..columns {
            let begin = col * pixels.len() / columns;
            let end = ((col + 1) * pixels.len() / columns).max(begin + 1);
            let mut pixel = [0; 3];
            for value in &pixels[begin..end] {
                for c in 0..3 {
                    pixel[c] = pixel[c].max(value[c]);
                }
            }
            if pixel != [0; 3] {
                let x = rail.left() + col as f32 * rail.width() / columns as f32;
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        Pos2::new(x, rail.top()),
                        Vec2::new(rail.width() / columns as f32 + 0.5, rail.height()),
                    ),
                    0.0,
                    preview_color(
                        pixel,
                        if self.guidance.active() {
                            0.1
                        } else {
                            self.config.brightness
                        },
                    ),
                );
            }
        }
        if !self.sidebar_visible {
            self.scene_rail_markers(ui, rail);
        }
        if targeting {
            self.rule_rail_target(ui, rail);
        }
        if map && let Some((first, last)) = self.guidance_area() {
            let count = self.config.device.led_count.max(1) as f32;
            let selected = Rect::from_min_max(
                Pos2::new(
                    rail.left() + first as f32 / count * rail.width(),
                    rail.top(),
                ),
                Pos2::new(
                    rail.left() + (last + 1) as f32 / count * rail.width(),
                    rail.bottom(),
                ),
            );
            ui.painter()
                .rect_stroke(selected, 2.0, Stroke::new(1.5, ACCENT), StrokeKind::Outside);
        }
        if map
            && rail_response.clicked()
            && let Some(pointer) = rail_response.interact_pointer_pos()
            && rail.contains(pointer)
        {
            let index = (((pointer.x - rail.left()) / rail.width()).clamp(0.0, 1.0)
                * (self.config.device.led_count - 1) as f32)
                .round() as usize;
            self.selection = (0..self.config.room.strip.len() - 1)
                .find(|i| {
                    self.point_led(*i)
                        .zip(self.point_led(*i + 1))
                        .is_some_and(|(a, b)| (a.min(b)..=a.max(b)).contains(&index))
                })
                .map(Selection::Segment);
        }
        if map && self.config.device.led_count >= self.config.room.strip.len() {
            let mut manual = !self.config.room.led_anchors.is_empty();
            let mut anchors = if manual {
                self.config.room.led_anchors.clone()
            } else {
                self.proportional_anchors()
            };
            let last = self.config.device.led_count - 1;
            for i in 0..anchors.len() {
                let actual = if self.config.room.reverse {
                    last - anchors[i]
                } else {
                    anchors[i]
                };
                let x = rail.left() + actual as f32 / last.max(1) as f32 * rail.width();
                let p = Pos2::new(x, rail.center().y);
                let selected = self.selection == Some(Selection::Point(i));
                let movable = i > 0 && i + 1 < anchors.len();
                let handle = ui
                    .interact(
                        Rect::from_center_size(p, Vec2::new(22.0, 36.0)),
                        ui.id().with(("allocation", i)),
                        if movable {
                            Sense::click_and_drag()
                        } else {
                            Sense::click()
                        },
                    )
                    .on_hover_text(format!(
                        "Point {} · LED {}{}",
                        i + 1,
                        actual,
                        if movable {
                            " · drag to allocate"
                        } else {
                            " · endpoint fixed"
                        }
                    ));
                if handle.clicked() || handle.dragged_by(egui::PointerButton::Primary) {
                    self.selection = Some(Selection::Point(i));
                }
                if handle.dragged_by(egui::PointerButton::Primary)
                    && movable
                    && let Some(pointer) = handle.interact_pointer_pos()
                {
                    let value = (((pointer.x - rail.left()) / rail.width()).clamp(0.0, 1.0)
                        * last as f32)
                        .round() as usize;
                    let value = if self.config.room.reverse {
                        last - value
                    } else {
                        value
                    };
                    anchors[i] = value.clamp(anchors[i - 1] + 1, anchors[i + 1] - 1);
                    manual = true;
                }
                ui.painter().circle_filled(
                    p,
                    if selected { 7.0 } else { 5.0 },
                    if selected { ACCENT } else { INK },
                );
                ui.painter().circle_stroke(
                    p,
                    if selected { 7.0 } else { 5.0 },
                    Stroke::new(2.0, CANVAS),
                );
                ui.painter().text(
                    Pos2::new(x, rail.bottom() + 8.0),
                    Align2::CENTER_TOP,
                    format!("{}", i + 1),
                    FontId::proportional(12.0),
                    if selected { ACCENT } else { MUTED },
                );
            }
            if manual {
                self.config.room.led_anchors = anchors;
            }
        }
    }
    fn settings(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        let mut replay = false;
        let mut preferences_changed = false;
        egui::Window::new("Settings").open(&mut open).resizable(false).show(ctx,|ui|{
            if ui.checkbox(&mut self.quiet,"Quiet mode").changed() {
                self.notification_gate.discard_before(Instant::now());
                if self.quiet {self.stop_guidance(StopReason::Quiet);}
            }
            ui.checkbox(&mut self.config.reduced_motion,"Reduced motion");
            ui.checkbox(&mut self.config.notifications_enabled,"Desktop notifications");
            ui.checkbox(&mut self.config.media_enabled,"Media playback");
            ui.add_space(12.0);
            preferences_changed |= ui.checkbox(&mut self.preferences.hide_tooltips, "Hide tooltips").changed();
            preferences_changed |= ui.checkbox(&mut self.preferences.hide_demos, "Hide demo content").changed();
            if ui.button("Replay setup guide").clicked(){replay=true;}
            ui.add_space(14.0);
            egui::CollapsingHeader::new("Integration status").show(ui,|ui|{
                ui.label(&self.desktop_state.notification_status);ui.label(&self.desktop_state.media_status);ui.label(&self.desktop_state.lock_status);
                #[cfg(windows)]
                if ui.button("Allow Windows notifications").on_hover_text("Ask Windows for access to notification app identities. Requires the installed MSIX package; lighting stays disabled until you enable it separately.").clicked() {
                    self.notification_permission_requested = true;
                }
                if let Err(error) = self.valid() { ui.colored_label(ERROR, error); }
                if let Some(error) = self.output.snapshot().error { ui.colored_label(ERROR, error); }
                ui.label("Lock or unavailable lock status always pauses lighting.");
                ui.label("WLED: HTTP 80 · DDP 4048 · 30 fps maximum");
                ui.label("One controller at a time. Release returns control to WLED; it does not guarantee darkness.");
                ui.add(egui::Label::new(self.path.display().to_string()).wrap());
            });
        });
        if preferences_changed {
            if self.preferences.hide_demos {
                self.show_suggestions = false;
                if self.example_config.take().is_some() {
                    self.preview.clear();
                }
            }
            self.persist_preferences();
        }
        if replay {
            self.replay_guide();
            open = false;
        }
        self.show_settings = open;
    }
}

impl eframe::App for LedAlertApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut state = self.state.lock().expect("Application state lock poisoned");
        state.ui(ui);
        #[cfg(windows)]
        if std::mem::take(&mut state.notification_permission_requested) {
            self.notification_permission.request_access(&state.desktop);
        }
    }
}

impl AppState {
    fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.poll_gui(&ctx);
        // The setup mark is UI-only; it need not accelerate physical output ticks.
        if self.guide
            && !self.config.reduced_motion
            && self.guide_started.elapsed() < Duration::from_millis(600)
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        ctx.global_style_mut(|style| {
            style.animation_time = if self.config.reduced_motion {
                0.0
            } else {
                0.18
            };
            // egui has a delay rather than an enable flag; infinity disables
            // both enabled and disabled-widget hover tips without timers.
            style.interaction.tooltip_delay = if self.preferences.hide_tooltips {
                f32::INFINITY
            } else {
                0.5
            };
            style.interaction.tooltip_grace_time = if self.preferences.hide_tooltips {
                0.0
            } else {
                0.2
            };
            style.explanation_tooltips = false;
        });
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.stop_guidance(StopReason::Stopped);
            self.stop_demos();
            self.stop_local_examples();
            self.object_drag = None;
            self.resize_origin = None;
            self.display_drag = None;
            self.orbit_origin = None;
            self.placing_walls = false;
            self.outline_editor.cancel_drag();
            self.history.finish_group();
        }
        egui::Panel::top("toolbar")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(14))
            .show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(10))
            .show(ui, |ui| {
                let output = self.output.snapshot();
                let validation = self.valid().err();
                let error = output
                    .error
                    .as_deref()
                    .or(validation.as_deref())
                    .or(self.probe_error.as_deref());
                ui.horizontal(|ui| {
                    let state = if self.guidance.active() {
                        "Guidance active"
                    } else if self.quiet {
                        "Quiet"
                    } else if self.desktop_state.locked == Some(true) {
                        "Locked"
                    } else if self.desktop_state.locked.is_none() {
                        "Lock unavailable"
                    } else if self.enabled && error.is_some() {
                        "Lighting paused"
                    } else if self.enabled {
                        "Lighting enabled"
                    } else {
                        "Lighting off"
                    };
                    ui.label(RichText::new(state).color(if self.guidance.active() {
                        CORAL
                    } else if self.enabled {
                        ACCENT
                    } else {
                        MUTED
                    }))
                    .on_hover_text(format!(
                        "{}\n{}",
                        self.desktop_state.lock_status, output.status
                    ));
                    if self.saved.as_ref() != Some(&self.config) {
                        ui.label(RichText::new("Unsaved").small().color(ACCENT));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(Action::new(Icon::Settings, "Settings")).clicked() {
                            self.show_settings = !self.show_settings;
                        }
                        if let Some(error) = error {
                            ui.add(egui::Label::new(RichText::new(error).color(ERROR)).truncate())
                                .on_hover_text(error);
                        } else if Instant::now() < self.notice_until && !self.notice.is_empty() {
                            ui.add(
                                egui::Label::new(RichText::new(&self.notice).color(MUTED))
                                    .truncate(),
                            )
                            .on_hover_text(&self.notice);
                        }
                    });
                });
            });
        if self.sidebar_visible {
            egui::Panel::right("inspector")
                .default_size(390.0)
                .min_size(350.0)
                .max_size(480.0)
                .frame(egui::Frame::new().fill(PANEL).inner_margin(18))
                .show(ui, |ui| self.inspector(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(CANVAS).inner_margin(22))
            .show(ui, |ui| {
                if let Some(error) = self.load_error.clone() {
                    ui.heading("Saved setup could not be loaded");
                    ui.label(error);
                    ui.label("Your file has not been changed.");
                    if ui.button("Begin new setup").clicked() {
                        self.load_error = None;
                        self.notify("Saving will replace the unreadable configuration.");
                    }
                } else {
                    if !self.sidebar_visible
                        || matches!(self.inspector, Inspector::Strip | Inspector::Rules)
                    {
                        egui::Panel::bottom("strip-preview")
                            .frame(egui::Frame::new().inner_margin(0))
                            .show(ui, |ui| self.strip_preview(ui));
                    }
                    self.room_plan(ui);
                }
            });
        if self.show_settings {
            self.settings(&ctx);
        }
        self.consent_dialog(&ctx);
        // Let focused text fields keep their native text undo before editor shortcuts.
        let editing_text = ctx.output(|o| o.ime.is_some());
        if !editing_text {
            if ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers {
                        shift: true,
                        ..egui::Modifiers::COMMAND
                    },
                    egui::Key::Z,
                ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
            }) {
                self.redo();
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
                self.undo();
            }
            if self.sidebar_visible
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
            {
                self.remove_selection();
            }
        }
        let released = ctx.input(|i| i.pointer.any_released());
        let group = if editing_text {
            ctx.memory(|m| m.focused()).map(|id| id.value())
        } else if ctx.input(|i| i.pointer.any_down()) || released {
            Some(u64::MAX)
        } else {
            None
        };
        if self.history.record(&self.config, &self.address, group) {
            self.changed();
        }
        if group.is_none() || released {
            self.history.finish_group();
        }
        // Commit focused fields and editor history before consuming action buttons.
        if self.finish_requested {
            self.finish_requested = false;
            self.finish_setup();
        }
        if let Some(request) = self.demo_request.take() {
            match request {
                DemoMode::Rule(index) => {
                    self.choose_rule(index);
                    self.play_rule_demo();
                }
                DemoMode::All => self.play_all_demos(),
                DemoMode::Random => self.start_preview(),
                DemoMode::Idle => self.stop_demos(),
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save_config();
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            self.stop_guidance(StopReason::Closed);
            self.stop_demos();
            self.enabled = false;
        }
        if ctx.input(|i| i.viewport().close_requested())
            && self.saved.as_ref() != Some(&self.config)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
            self.enabled = false;
        }
        if self.confirm_close {
            egui::Window::new("Unsaved setup")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(&ctx, |ui| {
                    ui.label("Save before closing?");
                    ui.horizontal(|ui| {
                        if ui.button("Keep editing").clicked() {
                            self.confirm_close = false;
                        }
                        if ui.button("Discard and close").clicked() {
                            self.saved = Some(self.config.clone());
                            self.confirm_close = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui
                            .add_enabled(self.valid().is_ok(), egui::Button::new("Save and close"))
                            .clicked()
                        {
                            self.save_config();
                            if self.saved.as_ref() == Some(&self.config) {
                                self.confirm_close = false;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                    });
                });
        }
    }
}
fn apply_room_size(
    config: &mut Config,
    origin: Option<&Config>,
    width: f32,
    depth: f32,
    height: f32,
) {
    if origin.is_some_and(|start| start.room.strip.len() != config.room.strip.len()) {
        return;
    }
    let old = origin.map_or(
        (config.room.width, config.room.depth, config.room.height),
        |start| (start.room.width, start.room.depth, start.room.height),
    );
    let ratios = (width / old.0, depth / old.1, height / old.2);
    let transform = |p: Point| Point {
        x: (p.x * ratios.0).clamp(0.0, width),
        y: (p.y * ratios.1).clamp(0.0, depth),
        z: (p.z * ratios.2).clamp(0.0, height),
    };
    // A drag scales its pristine geometry, not the previous frame's clamped values.
    for (index, point) in config.room.strip.iter_mut().enumerate() {
        *point = transform(origin.map_or(*point, |start| start.room.strip[index]));
    }
    for screen in &mut config.room.screens {
        let (position, size) = if let Some(start) = origin {
            let Some(original) = start.room.screens.iter().find(|s| s.id == screen.id) else {
                continue;
            };
            (original.position, original.width)
        } else {
            (screen.position, screen.width)
        };
        screen.position = transform(position);
        screen.width = (size * ratios.0).clamp(0.1, 5.0);
    }
    for (index, rule) in config.rules.iter_mut().enumerate() {
        if rule.options.range_unit == RangeUnit::Leds {
            continue;
        }
        let spread = if let Some(start) = origin {
            let Some(original) = start
                .rules
                .get(index)
                .filter(|r| r.application == rule.application)
            else {
                continue;
            };
            original.spread
        } else {
            rule.spread
        };
        rule.spread = (spread * ratios.0).clamp(0.1, 20.0);
    }
    config.room.width = width;
    config.room.depth = depth;
    config.room.height = height;
}

fn snapped_angle(angle: f32) -> f32 {
    ((angle / 45.0).round() * 45.0 + 180.0).rem_euclid(360.0) - 180.0
}

fn scalar(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
) {
    // Stable field identities keep numeric edit state attached to its axis,
    // independent of sibling widgets and their editing modes.
    ui.push_id(label, |ui| {
        ui.horizontal(|ui| {
            let label = ui.label(label);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(
                    egui::DragValue::new(value)
                        .range(range)
                        .speed(0.01)
                        .suffix(suffix)
                        .max_decimals(2)
                        .custom_parser(decimal_parser)
                        .update_while_editing(false),
                )
                .labelled_by(label.id);
            });
        });
    });
}
fn point_controls(ui: &mut egui::Ui, point: &mut Point, bounds: (f32, f32, f32)) {
    scalar(ui, "X", &mut point.x, 0.0..=bounds.0, " m");
    scalar(ui, "Y", &mut point.y, 0.0..=bounds.1, " m");
    scalar(ui, "Height", &mut point.z, 0.0..=bounds.2, " m");
}
fn preview_color(pixel: [u8; 3], brightness: f32) -> Color32 {
    let peak = pixel.into_iter().max().unwrap_or(0);
    if peak == 0 {
        return Color32::TRANSPARENT;
    }
    let hue = |c: u8| (u16::from(c) * 255 / u16::from(peak)) as u8;
    Color32::from_rgba_unmultiplied(
        hue(pixel[0]),
        hue(pixel[1]),
        hue(pixel[2]),
        (f32::from(peak) / brightness.max(1.0 / 255.0)).min(255.0) as u8,
    )
}
fn shape_button(ui: &mut egui::Ui, label: &str, ratio: f32, selected: bool) -> egui::Response {
    let response = ui.add(
        egui::Button::new(label)
            .selected(selected)
            .min_size(Vec2::new(78.0, 57.0)),
    );
    let center = Pos2::new(response.rect.center().x, response.rect.top() + 13.0);
    let size = if ratio >= 1.0 {
        Vec2::new(23.0, 23.0 / ratio)
    } else {
        Vec2::new(23.0 * ratio, 23.0)
    } * 0.55;
    ui.painter().rect_stroke(
        Rect::from_center_size(center, size),
        1.0,
        Stroke::new(1.0, if selected { ACCENT } else { MUTED }),
        StrokeKind::Inside,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_drag_restores_proportions_after_clamp_excursions() {
        let mut origin = Config::default();
        origin.rules[0].spread = 0.1;
        let mut current = origin.clone();
        for width in [0.5, 50.0, 5.0] {
            apply_room_size(&mut current, Some(&origin), width, 4.0, 2.5);
        }
        assert_eq!(current.room.screens[0].width, origin.room.screens[0].width);
        assert_eq!(current, origin);
    }

    #[test]
    fn room_drag_preserves_live_metadata_and_connection_changes() {
        let origin = Config::default();
        let mut current = origin.clone();
        current.device.led_count = 512;
        current.room.led_anchors = vec![0, 12, 44, 511];
        current.room.screens[0].name = "Renamed display".into();
        current.room.screens[0].aspect_ratio = 0.75;
        current.rules[0].color = [40, 50, 60];
        apply_room_size(&mut current, Some(&origin), 10.0, 8.0, 2.5);
        assert_eq!(current.device.led_count, 512);
        assert_eq!(current.room.led_anchors, [0, 12, 44, 511]);
        assert_eq!(current.room.screens[0].name, "Renamed display");
        assert_eq!(current.room.screens[0].aspect_ratio, 0.75);
        assert_eq!(current.rules[0].color, [40, 50, 60]);
        assert_eq!(
            current.room.screens[0].position.x,
            origin.room.screens[0].position.x * 2.0
        );
        current.validate().unwrap();
    }

    #[test]
    fn strip_topology_change_cancels_an_inflight_room_resize() {
        let origin = Config::default();
        let mut current = origin.clone();
        current.room.strip.remove(1);
        let after_delete = current.clone();
        apply_room_size(&mut current, Some(&origin), 10.0, 8.0, 2.5);
        assert_eq!(current, after_delete);
    }
}
