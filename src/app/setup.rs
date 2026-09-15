use super::*;

impl LedAlertApp {
    pub(super) fn persist_preferences(&mut self) {
        if let Err(error) = self.preferences.save(&self.preferences_path) {
            self.notify(format!("Guide preference not saved: {error}"));
        }
    }

    fn dismiss_guide(&mut self) {
        self.guide = false;
        self.preferences.guide_dismissed = true;
        self.persist_preferences();
    }

    pub(super) fn replay_guide(&mut self) {
        self.stop_guidance(StopReason::Navigation);
        self.guide = true;
        self.guide_started = Instant::now();
        self.preferences.guide_started = true;
        self.preferences.guide_dismissed = false;
        self.select_step(Inspector::Room);
    }

    pub(super) fn select_step(&mut self, step: Inspector) {
        self.sidebar_visible = true;
        if step != Inspector::Rules && matches!(self.demo_mode, DemoMode::Rule(_) | DemoMode::All) {
            self.stop_demos();
        }
        if step != Inspector::Strip {
            self.stop_guidance(StopReason::Navigation);
        }
        self.inspector = step;
        self.object_drag = None;
        self.resize_origin = None;
        self.display_drag = None;
        self.orbit_origin = None;
        self.placing_walls = false;
        self.selection = match (step, self.selection) {
            (Inspector::Displays, Some(Selection::Screen(i))) => Some(Selection::Screen(i)),
            (Inspector::Strip, Some(Selection::Point(i))) => Some(Selection::Point(i)),
            (Inspector::Strip, Some(Selection::Segment(i))) => Some(Selection::Segment(i)),
            _ => None,
        };
        if self.guide {
            if step == Inspector::Rules {
                self.show_suggestions = true;
            }
            self.preferences.guide_step = step.index() as u8;
            self.persist_preferences();
        }
    }

    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            self.identity.wordmark(ui, self.config.reduced_motion);
            ui.add_space(8.0);
            if ui.add_enabled(self.history.can_undo(), Action::new(Icon::Undo, "Undo · Ctrl+Z")).clicked() {
                self.undo();
            }
            if ui.add_enabled(self.history.can_redo(), Action::new(Icon::Redo, "Redo · Ctrl+Shift+Z")).clicked() {
                self.redo();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.guidance.active() {
                    if ui.add(Action::new(Icon::Stop, "Stop guidance and release realtime control · Esc").label("Stop guidance")).clicked() {
                        self.stop_guidance(StopReason::Stopped);
                    }
                } else {
                    let ready = self.valid().is_ok()
                        && !self.quiet
                        && self.desktop_state.locked == Some(false)
                        && self.connected.as_ref().is_some_and(|info| !info.live);
                    if ui.add_enabled(self.enabled || ready,
                        Action::new(Icon::Power, if self.enabled {
                            "Disable real lighting. Settings remain saved, permission does not."
                        } else {
                            "Enable real lighting for the connected strip. Requires a valid setup, unlocked session and quiet mode off."
                        }).label(if self.enabled { "Disable lighting" } else { "Enable lighting" }).selected(self.enabled)
                    ).clicked() {
                        self.enabled = !self.enabled;
                        self.engine.clear();
                        if !self.enabled {
                            self.stop_demos();
                            self.notify("Releasing realtime control");
                        }
                    }
                }
                let persistent = self.engine.persistent_count(None);
                if persistent > 0 && ui.add(Action::new(Icon::Close,
                    "Dismiss all persistent lights locally; desktop notifications are unchanged.")
                    .label(&format!("{persistent}"))).clicked() {
                    self.engine.dismiss_persistent(None);
                }
                if ui.add_enabled(self.valid().is_ok() && self.saved.as_ref() != Some(&self.config),
                    Action::new(Icon::Save, "Save setup · Ctrl+S")).clicked() {
                    self.save_config();
                }
                let random = self.demo_mode == DemoMode::Random;
                if ui.add_enabled(random || (self.setup_ready() && self.demo_ready()
                    && (0..self.config.rules.len()).any(|index| self.engine.demo_eligible(index))),
                    Action::new(if random { Icon::Stop } else { Icon::Play },
                        if random { "Stop randomized demo notifications · Esc" } else {
                            "Preview a finished setup: random rules, 1–5 seconds apart. Local while lighting is off; also sent to the strip when explicitly enabled."
                        }).selected(random)).clicked() {
                    self.demo_request = Some(DemoMode::Random);
                }
            });
        });
        ui.add_space(10.0);
        let width = (ui.available_width() - 24.0) / 4.0;
        ui.horizontal(|ui| {
            for step in Inspector::ALL {
                let selected = self.sidebar_visible && self.inspector == step;
                let icon = match step {
                    Inspector::Room => Icon::Wall,
                    Inspector::Displays => Icon::Monitor,
                    Inspector::Strip => Icon::Leds,
                    Inspector::Rules => Icon::Bell,
                };
                if ui
                    .add_sized(
                        [width, 34.0],
                        Action::new(icon, step.label())
                            .label(step.label())
                            .selected(selected),
                    )
                    .clicked()
                {
                    self.select_step(step);
                }
            }
        });
    }

    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        if self.guide {
            ui.horizontal(|ui| {
                let elapsed = self.guide_started.elapsed().as_secs_f32();
                let wake = if self.config.reduced_motion || elapsed >= 0.6 {
                    0.0
                } else {
                    elapsed / 0.6
                };
                self.identity.mark(ui, 38.0, wake).on_hover_text(match self.inspector {
                    Inspector::Room => "Drag room edges or enter metric measurements. Resemblance needn't be exact.",
                    Inspector::Displays => "Drag displays into position; use corner, ring and lift handles.",
                    Inspector::Strip => "Place points along the real strip. Drag LED rail handles to match each bend.",
                    Inspector::Rules => "Choose an app and position its marker. Demo buttons use real LEDs only while lighting is explicitly enabled.",
                });
                ui.vertical(|ui| {
                    ui.strong(format!("Setup · {} of 4", self.inspector.index() + 1));
                    if ui.add(Action::new(Icon::Close, "Skip setup guide; keep this setup")).clicked() {
                        self.dismiss_guide();
                    }
                });
            });
            ui.add_space(14.0);
        }
        egui::Panel::bottom("workflow-actions")
            .frame(egui::Frame::new().inner_margin(egui::Margin {
                left: 0,
                right: 0,
                top: 14,
                bottom: 0,
            }))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let index = self.inspector.index();
                    if index > 0
                        && ui
                            .add(Action::new(Icon::Back, "Previous setup step"))
                            .clicked()
                    {
                        self.select_step(Inspector::ALL[index - 1]);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if index < 3 {
                            let label = [
                                "Next: place displays",
                                "Next: map strip",
                                "Next: set up rules",
                            ][index];
                            if ui.add(Action::new(Icon::Next, label)).clicked() {
                                self.select_step(Inspector::ALL[index + 1]);
                            }
                        } else if ui
                            .add_enabled(
                                self.valid().is_ok(),
                                Action::new(
                                    Icon::Check,
                                    "Finish setup, save and show the live room. Select a top tab to edit again. Lighting stays under your control.",
                                ),
                            )
                            .clicked()
                        {
                            self.finish_requested = true;
                        }
                    });
                });
            });
        egui::ScrollArea::vertical()
            .id_salt(("task-inspector", self.inspector.index()))
            .auto_shrink([false, false])
            .show(ui, |ui| match self.inspector {
                Inspector::Room => self.room_controls(ui),
                Inspector::Displays => self.display_controls(ui),
                Inspector::Strip => self.strip_controls(ui),
                Inspector::Rules => self.rule_controls(ui),
            });
    }

    pub(super) fn room_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Room shape");
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            for (label, ratio) in [("Square", 1.0_f32), ("Wide", 1.5), ("Long", 0.67)] {
                if shape_button(
                    ui,
                    label,
                    ratio,
                    (self.config.room.width / self.config.room.depth - ratio).abs() < 0.04,
                )
                .clicked()
                {
                    self.resize_room(5.0 * ratio, 5.0, self.config.room.height);
                }
            }
        });
        ui.add_space(12.0);
        egui::CollapsingHeader::new("Measurements").show(ui, |ui| {
            let (mut w, mut d, mut h) = (
                self.config.room.width,
                self.config.room.depth,
                self.config.room.height,
            );
            scalar(ui, "Width", &mut w, 0.5..=50.0, " m");
            scalar(ui, "Depth", &mut d, 0.5..=50.0, " m");
            scalar(ui, "Height", &mut h, 0.5..=50.0, " m");
            if (w, d, h)
                != (
                    self.config.room.width,
                    self.config.room.depth,
                    self.config.room.height,
                )
            {
                self.resize_room(w, d, h);
            }
        }).header_response.on_hover_text("Drag edges for width, depth and height, or enter values in metres. Both 1.25 and 1,25 are accepted. Middle-drag to orbit.");
    }

    fn display_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Displays");
            ui.with_layout(Layout::right_to_left(Align::Center),|ui| {
                if ui.add_enabled(self.display_scan.is_none(),egui::Button::new(if self.display_scan.is_some(){"Detecting…"}else{"Refresh"})).on_hover_text("Read KDE's current setup. Never reconfigures displays or moves saved placements.").clicked(){self.scan_displays();}
            });
        });
        if let Some(error) = &self.display_error {
            ui.colored_label(ERROR, "Detection unavailable")
                .on_hover_text(error);
        }
        for (index, screen) in self.config.room.screens.iter().enumerate() {
            if ui
                .selectable_label(
                    self.selection == Some(Selection::Screen(index)),
                    &screen.name,
                )
                .on_hover_text(
                    screen
                        .connector
                        .as_deref()
                        .unwrap_or("Manually placed display"),
                )
                .clicked()
            {
                self.selection = Some(Selection::Screen(index));
            }
        }
        ui.add_space(8.0);
        if ui.add_enabled(!self.detected.is_empty(),egui::Button::new("Use desktop layout")).on_hover_text("Arrange detected displays like your desktop, standing upright. Replaces their placement; Undo restores it.").clicked() {
            if self.auto_import {self.use_detected(true);} else {
                match displays::arrange_displays(&mut self.config,&self.detected) {
                    Ok(count)=>{self.record_discrete();self.notify(format!("{count} displays arranged. Drag to place them in the room."));},
                    Err(error)=>self.notify(error.to_string()),
                }
            }
        }
        if ui
            .add_enabled(
                self.config.room.screens.len() < MAX_SCREENS,
                egui::Button::new("Add manually"),
            )
            .clicked()
        {
            self.auto_import = false;
            self.add_screen();
        }
        ui.add_space(14.0);
        if let Some(Selection::Screen(index)) = self.selection {
            self.screen_controls(ui, index);
        }
    }

    pub(super) fn stop_guidance(&mut self, reason: StopReason) {
        let active = self.guidance.active();
        self.guidance.cancel(reason);
        self.guidance_consent = None;
        if active {
            self.enabled = false;
            self.engine.clear();
            self.output.submit(self.config.device, &[], false, None);
        }
    }

    fn guidance_ready(&self) -> bool {
        self.inspector == Inspector::Strip
            && self.valid().is_ok()
            && !self.quiet
            && self.desktop_state.locked == Some(false)
            && self.probe.is_none()
            && self
                .connected
                .as_ref()
                .is_some_and(|info| !info.live && info.led_count == self.config.device.led_count)
    }

    pub(super) fn point_led(&self, index: usize) -> Option<usize> {
        let room = &self.config.room;
        let count = self.config.device.led_count;
        if count == 0 || index >= room.strip.len() {
            return None;
        }
        let last = count - 1;
        let index = if room.led_anchors.is_empty() {
            let total: f32 = room.strip.windows(2).map(|p| p[0].distance(p[1])).sum();
            if !total.is_finite() || total <= 0.0 {
                return None;
            }
            let distance: f32 = room.strip[..=index]
                .windows(2)
                .map(|p| p[0].distance(p[1]))
                .sum();
            (distance / total * last as f32).round() as usize
        } else {
            if room.led_anchors.len() != room.strip.len()
                || room.led_anchors.first() != Some(&0)
                || room.led_anchors.last() != Some(&last)
                || !room.led_anchors.windows(2).all(|p| p[0] < p[1])
            {
                return None;
            }
            room.led_anchors[index]
        };
        if index > last {
            return None;
        }
        Some(if room.reverse { last - index } else { index })
    }

    pub(super) fn guidance_area(&self) -> Option<(usize, usize)> {
        match self.selection {
            Some(Selection::Point(i)) => {
                let index = self.point_led(i)?;
                Some((
                    index.saturating_sub(2),
                    (index + 2).min(self.config.device.led_count - 1),
                ))
            }
            Some(Selection::Segment(i)) => {
                let a = self.point_led(i)?;
                let b = self.point_led(i + 1)?;
                Some((a.min(b), a.max(b)))
            }
            _ => None,
        }
    }

    pub(super) fn guidance_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(18.0);
        ui.strong("Live guidance");
        if self.guidance.active() {
            let remaining = self.guidance.remaining(Instant::now()).as_secs();
            ui.colored_label(
                ACCENT,
                format!("{}:{:02} remaining", remaining / 60, remaining % 60),
            );
            if let Some((a, b)) = self.guidance_area() {
                ui.label(format!("LEDs {a}–{b} · steady white"));
            }
            ui.horizontal(|ui| {
                if ui.button("Stop").clicked() {
                    self.stop_guidance(StopReason::Stopped);
                }
                if ui
                    .button("Extend")
                    .on_hover_text("Request a new two-minute session")
                    .clicked()
                {
                    self.guidance_consent = Some(self.config.device);
                }
            });
        } else {
            if ui.add_enabled(self.guidance_ready(),egui::Button::new("Guide with real lights").fill(Color32::from_rgb(76,37,59)))
                .on_disabled_hover_text("Connect WLED, unlock the session and leave quiet mode off. Another realtime source must stop first.")
                .on_hover_text("Requests temporary control of the whole strip; never starts without confirmation.").clicked() {
                if !matches!(self.selection,Some(Selection::Point(_)|Selection::Segment(_))){self.selection=Some(Selection::Point(0));}
                self.guidance_consent=Some(self.config.device);
            }
            if let Some(reason) = self.guidance.reason() {
                ui.label(RichText::new(reason.message()).small().color(MUTED));
            }
        }
    }

    pub(super) fn consent_dialog(&mut self, ctx: &egui::Context) {
        let Some(device) = self.guidance_consent else {
            return;
        };
        let mut take = false;
        let mut cancel = false;
        let ready =
            device == self.config.device && self.guidance_ready() && self.guidance_area().is_some();
        let response=egui::Modal::new(egui::Id::new("guidance-consent")).show(ctx,|ui| {
            ui.set_width(410.0);
            ui.heading("Use the strip for setup?");
            ui.label(RichText::new(format!("{} · {} LEDs",device.address,device.led_count)).color(ACCENT));
            ui.add_space(12.0);
            ui.label("Temporarily replaces lighting across the entire strip. Selected areas stay white; everything else goes dark.");
            ui.add_space(8.0);
            ui.colored_label(WARNING,"2 minutes · 10% RGB cap · no flashing");
            ui.label(RichText::new("Stop or Esc releases control. Settings stay unchanged; WLED decides what resumes. This is not an exclusive hardware lock.").small().color(MUTED));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                cancel=ui.button("Cancel").clicked();
                take=ui.add_enabled(ready,egui::Button::new("Take control").fill(Color32::from_rgb(108,38,59))).clicked();
            });
            if !ready {ui.colored_label(ERROR,"The setup or session changed. Cancel, reconnect and try again.");}
        });
        if cancel || response.should_close() {
            self.guidance_consent = None;
        }
        if take && ready {
            self.guidance_consent = None;
            self.enabled = false;
            self.stop_demos();
            self.engine.clear();
            self.preview.clear();
            self.example_config = None;
            match self.guidance.begin(
                device,
                self.desktop_state.locked == Some(false),
                &self.output.snapshot(),
                Instant::now(),
            ) {
                Ok(()) => self.notify(
                    "Guidance requested. Select a point or span to find it on the real strip.",
                ),
                Err(error) => self.notify(error.to_string()),
            }
        }
    }

    pub(super) fn scan_pinned(&mut self) {
        if self.pinned_scan.is_some() {
            return;
        }
        self.pinned_error = None;
        let (tx, rx) = mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("ledalert-pinned".into())
            .spawn(move || {
                let _ = tx.send(taskbar::discover().map_err(|error| format!("{error:#}")));
            }) {
            Ok(_) => self.pinned_scan = Some(rx),
            Err(error) => self.pinned_error = Some(error.to_string()),
        }
    }
}
