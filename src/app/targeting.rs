use super::*;

#[derive(Clone, Copy)]
struct SceneRuleMarker {
    rule: usize,
    anchor: usize,
    point: Pos2,
    group: usize,
}

#[derive(Clone, Copy)]
struct SceneMarkerGroup {
    rect: Rect,
    first: usize,
    count: usize,
}

impl LedAlertApp {
    fn selected_anchor(&self) -> Option<usize> {
        let rule = self.config.rules.get(self.selected_rule)?;
        rule_anchor(&self.config, self.engine.positions(), rule)
    }

    fn set_rule_anchor(&mut self, index: usize) {
        if let Some(rule) = self.config.rules.get_mut(self.selected_rule) {
            rule.options.position = Some(
                index.min(self.config.device.led_count - 1) as f32
                    / (self.config.device.led_count - 1).max(1) as f32,
            );
        }
    }

    pub(super) fn rule_position_controls(&mut self, ui: &mut egui::Ui) {
        if !self.sidebar_visible {
            return;
        }
        let Some(anchor) = self.selected_anchor() else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.label("Position").on_hover_text("Drag the colored marker along the room strip or along the LED rail below. Both views set the same actual device LED.");
            let mut led = anchor;
            if ui.add(egui::DragValue::new(&mut led).range(0..=self.config.device.led_count - 1)
                .prefix("LED ").update_while_editing(false)).changed() {
                self.set_rule_anchor(led);
            }
            if ui.add(Action::new(Icon::Monitor, "Follow the nearest LED to the assigned display; dragging either marker chooses an explicit strip position.")
                .selected(self.config.rules[self.selected_rule].options.position.is_none())).clicked() {
                self.config.rules[self.selected_rule].options.position = None;
            }
            let waiting = self.engine.persistent_count(Some(self.selected_rule));
            if waiting > 0 && ui.add(Action::new(Icon::Close, "Dismiss this application's persistent lights locally. Desktop notifications are not modified.")
                .label(&format!("Dismiss {waiting}"))).clicked() {
                self.engine.dismiss_persistent(Some(self.selected_rule));
            }
        });
        ui.add_space(8.0);
    }

    pub(super) fn rule_scene_target(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        projection: Projection,
    ) {
        if !self.sidebar_visible {
            return;
        }
        let Some(anchor) = self.selected_anchor() else {
            return;
        };
        let center = projection.project(self.engine.positions()[anchor]);
        let nearest = ui
            .input(|input| input.pointer.interact_pos())
            .and_then(|pointer| {
                self.engine
                    .positions()
                    .iter()
                    .enumerate()
                    .map(|(index, point)| (index, projection.project(*point).distance_sq(pointer)))
                    .min_by(|a, b| {
                        a.1.total_cmp(&b.1)
                            .then_with(|| a.0.abs_diff(anchor).cmp(&b.0.abs_diff(anchor)))
                    })
            });
        let hit = nearest
            .filter(|(_, distance)| *distance <= 144.0)
            .map_or(center, |(index, _)| {
                projection.project(self.engine.positions()[index])
            });
        let response = ui
            .interact(
                Rect::from_center_size(hit, Vec2::splat(28.0)),
                ui.id().with(("rule-position", self.selected_rule)),
                Sense::click_and_drag(),
            )
            .on_hover_text(format!(
                "{} · LED {anchor} · drag along the strip to position this rule",
                self.config.rules[self.selected_rule].application
            ));
        if !ui.input(|input| input.pointer.button_down(egui::PointerButton::Middle))
            && (response.clicked() || response.dragged_by(egui::PointerButton::Primary))
            && let Some((index, _)) = nearest
        {
            self.set_rule_anchor(index);
        }
        let index = self.selected_anchor().unwrap_or(anchor);
        let center = projection.project(self.engine.positions()[index]);
        self.paint_rule_marker(painter, self.selected_rule, center, 11.0, ACCENT);
        painter.text(
            center + Vec2::new(15.0, -14.0),
            Align2::LEFT_BOTTOM,
            format!("LED {index}"),
            FontId::proportional(12.0),
            INK,
        );
    }

    pub(super) fn rule_rail_target(&mut self, ui: &mut egui::Ui, rail: Rect) {
        if !self.sidebar_visible {
            self.scene_rail_markers(ui, rail);
            return;
        }
        let Some(anchor) = self.selected_anchor() else {
            return;
        };
        let last = (self.config.device.led_count - 1).max(1) as f32;
        let response = ui.interact(rail.expand2(Vec2::new(0.0, 10.0)),
            ui.id().with(("rule-rail-position", self.selected_rule)), Sense::click_and_drag())
            .on_hover_text("Click or drag to place this rule at an actual device LED. The marker in the room follows the same position.");
        if (response.clicked() || response.dragged_by(egui::PointerButton::Primary))
            && let Some(pointer) = response.interact_pointer_pos()
        {
            self.set_rule_anchor(
                (((pointer.x - rail.left()) / rail.width()).clamp(0.0, 1.0) * last).round()
                    as usize,
            );
        }
        let index = self.selected_anchor().unwrap_or(anchor);
        let center = Pos2::new(
            rail.left() + index as f32 / last * rail.width(),
            rail.center().y,
        );
        self.paint_rule_marker(ui.painter(), self.selected_rule, center, 10.0, ACCENT);
        ui.painter().text(
            Pos2::new(center.x, rail.bottom() + 10.0),
            Align2::CENTER_TOP,
            format!("LED {index}"),
            FontId::proportional(12.0),
            INK,
        );
    }

    pub(super) fn scene_rule_markers(
        &self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        projection: Projection,
        bounds: Rect,
    ) {
        ui.push_id("scene-app-positions", |ui| {
            self.read_only_rule_markers(ui, painter, bounds, Vec2::new(0.0, -30.0), |anchor| {
                projection.project(self.engine.positions()[anchor])
            });
        });
    }

    pub(super) fn scene_rail_markers(&self, ui: &mut egui::Ui, rail: Rect) {
        let last = self.engine.positions().len().saturating_sub(1).max(1) as f32;
        let painter = ui.painter().clone();
        ui.push_id("rail-app-positions", |ui| {
            self.read_only_rule_markers(
                ui,
                &painter,
                rail.expand2(Vec2::new(8.0, 42.0)),
                Vec2::new(0.0, 30.0),
                |anchor| {
                    Pos2::new(
                        rail.left() + anchor as f32 / last * rail.width(),
                        rail.center().y,
                    )
                },
            );
        });
    }

    fn read_only_rule_markers(
        &self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        bounds: Rect,
        offset: Vec2,
        position: impl Fn(usize) -> Pos2,
    ) {
        // Every configured rule fits, including a fully populated 128-rule setup.
        // Only labels are grouped; their dots and leaders retain exact engine anchors.
        let mut markers = [SceneRuleMarker {
            rule: 0,
            anchor: 0,
            point: Pos2::ZERO,
            group: 0,
        }; MAX_RULES];
        let mut groups = [SceneMarkerGroup {
            rect: Rect::NOTHING,
            first: 0,
            count: 0,
        }; MAX_RULES];
        let mut marker_count = 0;
        let mut group_count = 0;
        let marker_size = Vec2::new(56.0, 40.0);
        let visible = bounds.intersect(ui.clip_rect());
        if !visible.is_positive() {
            return;
        }
        // A newly resized viewport can briefly clip the rail below a full marker's height.
        let inset = (marker_size * 0.5).min(visible.size() * 0.5);
        let label_bounds = visible.shrink2(inset);
        for (rule_index, rule) in self.config.rules.iter().enumerate() {
            let Some(anchor) = rule_anchor(&self.config, self.engine.positions(), rule) else {
                continue;
            };
            let point = position(anchor);
            let center = label_bounds.clamp(point + offset);
            let rect = Rect::from_center_size(center, marker_size);
            let group = groups[..group_count]
                .iter()
                .position(|group| group.rect.intersects(rect))
                .unwrap_or_else(|| {
                    let index = group_count;
                    groups[index] = SceneMarkerGroup {
                        rect,
                        first: marker_count,
                        count: 0,
                    };
                    group_count += 1;
                    index
                });
            groups[group].count += 1;
            markers[marker_count] = SceneRuleMarker {
                rule: rule_index,
                anchor,
                point,
                group,
            };
            marker_count += 1;
        }
        let markers = &markers[..marker_count];
        for marker in markers {
            let center = groups[marker.group].rect.center();
            let direction = (center - marker.point).normalized();
            painter.line_segment(
                [marker.point, center - direction * 14.0],
                Stroke::new(1.0, LINE),
            );
            let rule = &self.config.rules[marker.rule];
            painter.circle_filled(marker.point, 3.5, CANVAS);
            painter.circle_filled(
                marker.point,
                2.2,
                Color32::from_rgb(rule.color[0], rule.color[1], rule.color[2]),
            );
        }
        for (group_index, group) in groups[..group_count].iter().enumerate() {
            let first = &markers[group.first];
            let rule = &self.config.rules[first.rule];
            let name = self.rule_application_name(first.rule);
            let center = group.rect.center();
            let response = ui.interact(group.rect, ui.id().with(first.rule), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    format!(
                        "{name}, LED {}, {} {}. Inspect light positions",
                        first.anchor,
                        group.count,
                        if group.count == 1 { "rule" } else { "rules" },
                    ),
                )
            });
            self.paint_rule_marker(
                painter,
                first.rule,
                center,
                11.0,
                if response.hovered() || response.has_focus() {
                    ACCENT
                } else if rule.enabled {
                    MUTED
                } else {
                    LINE
                },
            );
            if group.count > 1 {
                let badge = center + Vec2::new(16.0, -10.0);
                painter.circle_filled(badge, 11.0, PANEL);
                painter.circle_stroke(badge, 11.0, Stroke::new(1.0, MUTED));
                painter.text(
                    badge,
                    Align2::CENTER_CENTER,
                    group.count.to_string(),
                    FontId::proportional(10.0),
                    INK,
                );
            }
            let response = response.on_hover_text(if group.count > 1 {
                format!(
                    "{name} · LED {}\n{} rules share this area. Click to inspect every position.",
                    first.anchor, group.count,
                )
            } else {
                format!(
                    "{name} · LED {}{}\nClick to inspect this position.",
                    first.anchor,
                    if rule.enabled {
                        ""
                    } else {
                        " · rule disabled"
                    },
                )
            });
            egui::Popup::menu(&response)
                .width(280.0)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    ui.strong(if group.count == 1 {
                        "Light position".to_owned()
                    } else {
                        format!("{} application positions", group.count)
                    });
                    egui::ScrollArea::vertical()
                        .id_salt(("position-list", first.rule))
                        .max_height(280.0)
                        .show_rows(ui, 40.0, group.count, |ui, rows| {
                            for marker in markers
                                .iter()
                                .filter(|marker| marker.group == group_index)
                                .skip(rows.start)
                                .take(rows.len())
                            {
                                let rule = &self.config.rules[marker.rule];
                                ui.horizontal(|ui| {
                                    let (rect, _) =
                                        ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
                                    self.paint_rule_marker(
                                        ui.painter(),
                                        marker.rule,
                                        rect.center(),
                                        10.0,
                                        LINE,
                                    );
                                    ui.vertical(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                self.rule_application_name(marker.rule),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&rule.application);
                                        ui.label(
                                            RichText::new(format!(
                                                "LED {}{}",
                                                marker.anchor,
                                                if rule.enabled {
                                                    ""
                                                } else {
                                                    " · rule disabled"
                                                },
                                            ))
                                            .small()
                                            .color(MUTED),
                                        );
                                    });
                                });
                            }
                        });
                });
        }
    }

    fn rule_application_name(&self, index: usize) -> &str {
        let application = &self.config.rules[index].application;
        if application == "*" {
            "All other apps"
        } else {
            self.application_assets
                .get(application)
                .and_then(applications::AppVisual::name)
                .unwrap_or(application)
        }
    }

    fn paint_rule_marker(
        &self,
        painter: &egui::Painter,
        rule_index: usize,
        center: Pos2,
        radius: f32,
        outline: Color32,
    ) {
        let rule = &self.config.rules[rule_index];
        painter.circle_filled(center, radius + 3.0, CANVAS);
        painter.circle_stroke(center, radius + 3.0, Stroke::new(2.0, outline));
        if let Some(texture) = self
            .application_assets
            .get(&rule.application)
            .and_then(applications::AppVisual::texture)
        {
            let size = texture.size_vec2();
            let fitted = size * (radius * 1.6 / size.max_elem());
            painter.image(
                texture.id(),
                Rect::from_center_size(center, fitted),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            painter.circle_filled(
                center,
                radius - 2.0,
                Color32::from_rgb(rule.color[0], rule.color[1], rule.color[2]),
            );
        }
    }
}
