use super::*;

impl AppState {
    pub(super) fn rule_controls(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().button_padding = Vec2::splat(5.0);
        ui.spacing_mut().interact_size.y = 28.0;
        ui.spacing_mut().item_spacing = Vec2::splat(6.0);
        self.sync_rule_display();
        self.pinned_controls(ui);
        ui.horizontal(|ui| {
            ui.heading("Rules");
            ui.label(
                RichText::new(format!("{}/{}", self.config.rules.len(), MAX_RULES))
                    .small()
                    .color(MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.config.rules.len() < MAX_RULES,
                        Action::new(Icon::Plus, "Add an application rule")
                            .label("Add app")
                            .selected(self.adding_rule),
                    )
                    .clicked()
                {
                    self.adding_rule = !self.adding_rule;
                }
            });
        });
        ui.add_space(6.0);
        let columns = choice_columns(ui.available_width());
        let mut picked_rule = None;
        egui::ScrollArea::vertical()
            .id_salt("rule-grid")
            .max_height(136.0)
            .auto_shrink([false, true])
            .show_rows(ui, 28.0, self.config.rules.len().div_ceil(columns), |ui, rows| {
                let width = (ui.available_width() - 6.0 * (columns - 1) as f32) / columns as f32;
                for row in rows {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for index in row * columns..((row + 1) * columns).min(self.config.rules.len()) {
                            let rule = &mut self.config.rules[index];
                            let name = if rule.application == "*" {
                                "All other apps"
                            } else {
                                self.application_assets.get(&rule.application)
                                    .and_then(applications::AppVisual::name)
                                    .unwrap_or(&rule.application)
                            };
                            let tooltip = format!("{name}\nApplication: {}\nSelect to edit. Drag its marker on either strip view to position the light. Power toggles the rule independently.", rule.application);
                            let texture = self.application_assets.get(&rule.application).and_then(applications::AppVisual::texture);
                            ui.push_id(index, |ui| {
                                ui.allocate_ui_with_layout(Vec2::new(width, 28.0), Layout::left_to_right(Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    if app_choice(ui, name, texture, self.selected_rule == index, width - 32.0, &tooltip).clicked() {
                                        picked_rule = Some(index);
                                    }
                                    if ui.add_sized(
                                        [28.0, 28.0],
                                        Action::new(Icon::Power, &format!("{} {name}", if rule.enabled { "Disable" } else { "Enable" }))
                                            .selected(rule.enabled),
                                    ).clicked() {
                                        rule.enabled = !rule.enabled;
                                    }
                                });
                            });
                        }
                    });
                }
            });
        if let Some(index) = picked_rule {
            self.choose_rule(index);
        }
        if self.config.rules.is_empty()
            && ui
                .add(
                    Action::new(
                        Icon::Plus,
                        "Create a fallback rule for unmatched applications",
                    )
                    .label("Use a simple default"),
                )
                .clicked()
        {
            self.add_rule("*".into());
        }
        if self.adding_rule {
            ui.add_space(6.0);
            let mut picked = None;
            egui::ScrollArea::vertical()
                .id_salt("recent-app-grid")
                .max_height(96.0)
                .auto_shrink([false, true])
                .show_rows(
                    ui,
                    28.0,
                    self.recent_applications.len().div_ceil(columns),
                    |ui, rows| {
                        let width =
                            (ui.available_width() - 6.0 * (columns - 1) as f32) / columns as f32;
                        for row in rows {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                for index in row * columns
                                    ..((row + 1) * columns).min(self.recent_applications.len())
                                {
                                    let app = &self.recent_applications[index];
                                    ui.push_id(index, |ui| {
                                        let visual = self.application_assets.get(app);
                                        let name = visual
                                            .and_then(applications::AppVisual::name)
                                            .unwrap_or(app);
                                        if app_choice(
                                            ui,
                                            name,
                                            visual.and_then(applications::AppVisual::texture),
                                            false,
                                            width,
                                            app,
                                        )
                                        .clicked()
                                        {
                                            picked = Some(app.clone());
                                        }
                                    });
                                }
                            });
                        }
                    },
                );
            if let Some(app) = picked {
                self.add_rule(app);
            }
            ui.horizontal(|ui| {
                let response = ui.add_sized(
                    [ui.available_width() - 2.0 * (28.0 + ui.spacing().item_spacing.x), 28.0],
                    egui::TextEdit::singleline(&mut self.new_application)
                        .hint_text("Application ID")
                        .char_limit(128),
                ).on_hover_text("Choose a recently seen app above, or enter its exact application ID. Send a desktop notification to discover an app.");
                if ui.add_enabled(
                    !self.new_application.trim().is_empty(),
                    Action::new(Icon::Plus, "Create rule from application ID"),
                ).clicked() || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    self.add_rule(self.new_application.clone());
                }
                if ui.add(Action::new(Icon::Close, "Cancel adding an application")).clicked() {
                    self.adding_rule = false;
                }
            });
        }
        self.sync_rule_display();
        self.demo_controls(ui);
        let reach = self.config.room.width;
        let Some(rule) = self.config.rules.get_mut(self.selected_rule) else {
            return;
        };
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            for (label, duration, intensity, fraction) in [
                ("Calm", 3.0, 0.45, 0.16),
                ("Balanced", 4.0, 1.0, 0.24),
                ("Prominent", 7.0, 1.0, 0.4),
            ] {
                let spread = (reach * fraction).clamp(0.1, 20.0);
                let selected = (rule.duration - duration).abs() < 0.01
                    && (rule.spread - spread).abs() < 0.01
                    && rule.options.range_unit == RangeUnit::Room
                    && (rule.options.intensity - intensity).abs() < 0.01
                    && rule.options.fade;
                if ui
                    .add(
                        Action::new(
                            Icon::Spark,
                            "Set duration, reach and intensity; keep the app, triggers and color.",
                        )
                        .label(label)
                        .selected(selected),
                    )
                    .clicked()
                {
                    rule.duration = duration;
                    rule.spread = spread;
                    rule.options.range_unit = RangeUnit::Room;
                    rule.options.intensity = intensity;
                    rule.options.fade = true;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Notice");
            if ui.selectable_value(&mut rule.options.mode, NotificationMode::OneOff, "One-off")
                .on_hover_text("A finite animated indication, then fade out. Reduced motion keeps it steady for the selected duration.").clicked() {
                rule.options.fade = true;
            }
            ui.selectable_value(&mut rule.options.mode, NotificationMode::Persistent, "Persistent")
                .on_hover_text("Stays until dismissed here or explicitly closed on the desktop. Popup expiry alone does not dismiss the light. A ripple enters once, over a steady marker.");
        });
        ui.push_id(("rule-effects", self.selected_rule), |ui| {
            effects::controls(ui, rule, self.config.reduced_motion);
        });
        ui.add_space(6.0);
        egui::CollapsingHeader::new("Triggers & timing")
            .id_salt(("rule-advanced", self.selected_rule))
            .show(ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut rule.application).char_limit(128).desired_width(f32::INFINITY))
                    .on_hover_text("Application ID: exact, case-insensitive. * is the fallback for unmatched apps.");
                ui.horizontal(|ui| {
                    if ui.add(Action::new(Icon::Bell, "Activate from desktop notifications").label("Notifications").selected(rule.options.notifications)).clicked() {
                        rule.options.notifications = !rule.options.notifications;
                    }
                    let media_hint = if cfg!(target_os = "macos") {
                        "Activate while this app has an active audio output stream. May include calls or silence; no audio is recorded."
                    } else {
                        "Activate while this app is playing media"
                    };
                    let media_label = if cfg!(target_os = "macos") { "Audio" } else { "Media" };
                    if ui.add(Action::new(Icon::Media, media_hint).label(media_label).selected(rule.options.media)).clicked() {
                        rule.options.media = !rule.options.media;
                    }
                });
                ui.add_enabled_ui(rule.options.notifications, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Urgency");
                        egui::ComboBox::from_id_salt("minimum-urgency")
                            .selected_text(["Any", "Normal", "Critical"][rule.options.minimum_urgency.min(2) as usize])
                            .width(72.0)
                            .show_ui(ui, |ui| {
                                for (value, label) in [(0, "Any"), (1, "Normal"), (2, "Critical")] {
                                    ui.selectable_value(&mut rule.options.minimum_urgency, value, label);
                                }
                            }).response.on_hover_text("Minimum notification urgency that activates this rule");
                        ui.checkbox(&mut rule.options.critical_accent, "Critical accent")
                            .on_hover_text("Warmer color and a longer hold for critical notifications.");
                    });
                });
                scalar(ui, if rule.options.mode == NotificationMode::Persistent { "Entrance" } else { "Duration" }, &mut rule.duration, 0.5..=15.0, " s");
                ui.add(egui::Slider::new(&mut rule.options.intensity, 0.0..=1.0).text("Intensity")
                    .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)));
                ui.checkbox(&mut rule.options.fade, "Fade in and out")
                    .on_hover_text("Animate entrance and one-off fade. Reduced motion always takes precedence; persistent lights remain until dismissed.");
            });
        if !rule.options.notifications && !rule.options.media {
            ui.colored_label(ACCENT, "Choose a trigger to activate this rule.");
        }
        ui.add_space(8.0);
        if ui
            .add(Action::new(Icon::Trash, "Remove the selected rule").label("Remove rule"))
            .clicked()
        {
            self.config.rules.remove(self.selected_rule);
            self.selected_rule = self.selected_rule.saturating_sub(1);
            self.sync_rule_display();
        }
    }

    pub(super) fn sync_rule_display(&mut self) {
        if let Some(rule) = self.config.rules.get(self.selected_rule) {
            self.selection = self
                .config
                .room
                .screens
                .iter()
                .position(|screen| screen.id == rule.screen_id)
                .map(Selection::Screen);
        }
    }
    fn selected_examples(&self) -> Vec<Rule> {
        let screen_id = match self.selection {
            Some(Selection::Screen(i)) => self.config.room.screens.get(i).map(|screen| screen.id),
            _ => None,
        }
        .unwrap_or(self.config.room.screens[0].id);
        self.pinned
            .iter()
            .zip(&self.pinned_selected)
            .filter(|(_, selected)| **selected)
            .map(|(app, _)| taskbar::suggested_rule(app, screen_id, self.config.room.width))
            .collect()
    }

    fn start_examples(&mut self) {
        self.stop_demos();
        if let Err(error) = self.valid() {
            self.notify(error);
            return;
        }
        if self.config.brightness == 0.0 {
            self.notify("Raise the alert brightness to preview.");
            return;
        }
        let rules = self.selected_examples();
        if rules.is_empty() {
            self.notify("Choose at least one pinned app.");
            return;
        }
        let mut config = self.config.clone();
        config.rules = rules;
        config.notifications_enabled = true;
        config.media_enabled = false;
        self.preview.clear();
        match self.preview.configure(&config) {
            Ok(()) => {
                self.example_config = Some(config);
                self.example_index = 0;
                self.example_next = Instant::now();
                self.notify("Local demo. No light commands are sent.");
            }
            Err(error) => self.notify(error.to_string()),
        }
    }

    pub(super) fn pinned_controls(&mut self, ui: &mut egui::Ui) {
        if self.preferences.hide_demos {
            return;
        }
        ui.spacing_mut().button_padding = Vec2::splat(5.0);
        ui.spacing_mut().interact_size.y = 28.0;
        ui.spacing_mut().item_spacing = Vec2::splat(6.0);
        ui.horizontal(|ui| {
            if ui.add(Action::new(
                if self.show_suggestions { Icon::ChevronDown } else { Icon::ChevronRight },
                "Pinned-app examples are synthetic and local. Real alerts depend on each app's desktop notification support.",
            ).label("Examples").selected(self.show_suggestions)).clicked() {
                self.show_suggestions = !self.show_suggestions;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(Action::new(Icon::EyeOff, "Hide examples. Show them again in Settings.")).clicked() {
                    self.preferences.hide_demos = true;
                    self.show_suggestions = false;
                    if self.example_config.take().is_some() {
                        self.preview.clear();
                    }
                    self.persist_preferences();
                }
            });
        });
        if !self.show_suggestions || self.preferences.hide_demos {
            ui.add_space(6.0);
            return;
        }
        if let Some(error) = &self.pinned_error {
            ui.colored_label(ERROR, format!("Pinned apps unavailable: {error}"));
        }
        if self.pinned.is_empty() {
            ui.label(
                RichText::new(if self.pinned_scan.is_some() {
                    "Reading pinned apps…"
                } else {
                    "No supported pinned apps found."
                })
                .color(MUTED),
            );
        }
        let columns = choice_columns(ui.available_width());
        egui::ScrollArea::vertical()
            .id_salt("pinned-choices")
            .max_height(104.0)
            .auto_shrink([false, true])
            .show_rows(ui, 28.0, self.pinned.len().div_ceil(columns), |ui, rows| {
                let width = (ui.available_width() - 6.0 * (columns - 1) as f32) / columns as f32;
                for row in rows {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for index in row * columns..((row + 1) * columns).min(self.pinned.len()) {
                            let app = &self.pinned[index];
                            let selected = &mut self.pinned_selected[index];
                            let tooltip = format!("{}\n{}\n{} this local example; notification support is not detected.", app.name, app.id, if *selected { "Deselect" } else { "Select" });
                            ui.push_id(index, |ui| {
                                let texture = self.application_assets.get(&app.id).and_then(applications::AppVisual::texture);
                                if app_choice(ui, &app.name, texture, *selected, width, &tooltip).clicked() {
                                    *selected = !*selected;
                                }
                            });
                        }
                    });
                }
            });
        ui.horizontal_wrapped(|ui| {
            let selected = self
                .pinned_selected
                .iter()
                .filter(|selected| **selected)
                .count();
            if ui
                .add_enabled(
                    selected > 0 && !self.guidance.active(),
                    Action::new(
                        Icon::Play,
                        "Try selected examples locally. No light commands are sent.",
                    )
                    .label("Try"),
                )
                .clicked()
            {
                self.start_examples();
            }
            if self.example_config.is_some()
                && ui
                    .add(Action::new(Icon::Stop, "Stop the local examples"))
                    .clicked()
            {
                self.example_config = None;
                self.preview.clear();
            }
            if ui
                .add_enabled(
                    selected > 0 && self.config.rules.len() < MAX_RULES,
                    Action::new(
                        Icon::Plus,
                        "Add selected apps as rules; existing application rules are kept.",
                    )
                    .label("Add selected"),
                )
                .clicked()
            {
                let mut candidate = self.config.clone();
                let first = candidate.rules.len();
                for rule in self.selected_examples() {
                    if !candidate
                        .rules
                        .iter()
                        .any(|current| current.application.eq_ignore_ascii_case(&rule.application))
                    {
                        candidate.rules.push(rule);
                    }
                }
                match candidate.validate() {
                    Ok(()) => {
                        let added = candidate.rules.len() - first;
                        self.config = candidate;
                        if added > 0 {
                            self.selected_rule = first;
                            self.sync_rule_display();
                            self.record_discrete();
                        }
                        self.notify(if added > 0 {
                            format!("{added} rules added. Undo removes the batch.")
                        } else {
                            "These apps already have rules.".into()
                        });
                    }
                    Err(error) => self.notify(error.to_string()),
                }
            }
            if ui
                .add_enabled(
                    self.pinned_scan.is_none(),
                    Action::new(Icon::Reset, "Refresh pinned applications"),
                )
                .clicked()
            {
                self.scan_pinned();
            }
            if !self.pinned.is_empty() {
                ui.label(
                    RichText::new(format!("{selected} selected"))
                        .small()
                        .color(MUTED),
                );
            }
        });
        ui.add_space(10.0);
    }
}

fn choice_columns(width: f32) -> usize {
    ((width - 12.0) / 160.0).floor().max(1.0) as usize
}

fn app_choice(
    ui: &mut egui::Ui,
    name: &str,
    texture: Option<&egui::TextureHandle>,
    selected: bool,
    width: f32,
    tooltip: &str,
) -> egui::Response {
    let icon_id = ui.id().with("shortcut-icon");
    let button = if let Some(texture) = texture {
        let size = texture.size_vec2();
        egui::Button::new((
            egui::Atom::custom(icon_id, size * (20.0 / size.max_elem())),
            name,
        ))
    } else {
        egui::Button::new(name)
    }
    .selected(selected)
    .truncate();
    let mut response = None;
    ui.allocate_ui_with_layout(
        Vec2::new(width, 28.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_size(Vec2::new(width, 28.0));
            let painted = button.min_size(Vec2::new(width, 28.0)).atom_ui(ui);
            if let Some(texture) = texture
                && let Some(rect) = painted.rect(icon_id)
            {
                ui.painter().image(
                    texture.id(),
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            painted.response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    selected,
                    tooltip,
                )
            });
            response = Some(painted.response.on_hover_text(tooltip));
        },
    );
    response.expect("application button rendered")
}
