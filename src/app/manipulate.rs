use super::*;

impl LedAlertApp {
    pub(super) fn orbit_camera(&mut self, ui: &egui::Ui, rect: Rect) {
        let middle = egui::PointerButton::Middle;
        ui.input(|input| {
            if input.pointer.button_pressed(middle)
                && let Some(pointer) = input.pointer.interact_pos()
                && rect.contains(pointer)
            {
                self.orbit_origin = Some((pointer, self.camera_yaw, self.camera_pitch));
                self.object_drag = None;
                self.display_drag = None;
                self.resize_origin = None;
            }
            if input.pointer.button_down(middle) {
                if let Some((origin, yaw, pitch)) = self.orbit_origin
                    && let Some(pointer) = input.pointer.interact_pos()
                {
                    let delta = pointer - origin;
                    self.camera_yaw = (yaw - delta.x * 0.008).rem_euclid(std::f32::consts::TAU);
                    self.camera_pitch = (pitch + delta.y * 0.006)
                        .clamp(15.0_f32.to_radians(), 75.0_f32.to_radians());
                }
            } else {
                self.orbit_origin = None;
            }
        });
        if self.orbit_origin.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }

    fn strip_height_bounds(&self) -> (f32, f32) {
        self.config
            .room
            .strip
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.z), hi.max(p.z))
            })
    }

    pub(super) fn strip_height_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.label("Whole-strip height");
        let (lo, hi) = self.strip_height_bounds();
        let mut height = lo;
        if ui
            .add(distance_slider(
                &mut height,
                0.0..=(self.config.room.height - (hi - lo)).max(0.0),
            ))
            .on_hover_text("Raise or lower every point together, preserving existing height differences. Scene lift handles always move only the selected point.")
            .changed()
        {
            for point in &mut self.config.room.strip {
                point.z += height - lo;
            }
        }
    }

    pub(super) fn apply_wall_route(&mut self) {
        let height = self.strip_height_bounds().0;
        match wall_path(&self.config.room, &self.selected_walls, height) {
            Ok(points) => {
                self.config.room.strip = points;
                self.config.room.led_anchors.clear();
                self.config.room.reverse = false;
                self.selection = Some(Selection::Point(0));
                self.placing_walls = false;
                self.finish_strip_placement();
            }
            Err(error) => self.notify(error.to_string()),
        }
    }

    pub(super) fn set_strip_point(&mut self, index: usize, point: Point) -> bool {
        let strip = &self.config.room.strip;
        if strip.get(index).is_none_or(|current| *current == point)
            || !point.x.is_finite()
            || !point.y.is_finite()
            || !point.z.is_finite()
            || index
                .checked_sub(1)
                .and_then(|i| strip.get(i))
                .is_some_and(|neighbor| neighbor.distance(point) < 0.01)
            || strip
                .get(index + 1)
                .is_some_and(|neighbor| neighbor.distance(point) < 0.01)
        {
            return false;
        }
        self.config.room.strip[index] = point;
        true
    }

    pub(super) fn scene_strip_controls(&mut self, ui: &mut egui::Ui) {
        // A reserved toolbar keeps controls off the drawing and avoids refitting
        // the camera when a point is selected or a placement route is completed.
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 76.0),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_min_height(76.0);
                if self.placement_open {
                    ui.horizontal(|ui| {
                        if ui.add(Action::new(Icon::Grid, "Replace the path with all four walls. Undo restores the previous path and LED allocation.")
                            .label("Around room")).clicked()
                        {
                            self.selected_walls = vec![0, 1, 2, 3];
                            self.apply_wall_route();
                        }
                        if ui.add(Action::new(Icon::Wall, "Select adjacent walls in route order, then place the strip.")
                            .label("Along walls").selected(self.placing_walls)).clicked()
                        {
                            self.placing_walls = !self.placing_walls;
                            self.selected_walls.clear();
                        }
                        if ui.add(Action::new(Icon::Close, "Cancel placement without changing the strip")).clicked() {
                            self.placement_open = false;
                            self.placing_walls = false;
                            self.selected_walls.clear();
                        }
                    });
                    if self.placing_walls {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} / 4 walls", self.selected_walls.len()));
                            if ui.add_enabled(!self.selected_walls.is_empty(),
                                Action::new(Icon::Undo, "Remove the last wall from the route")).clicked()
                            {
                                self.selected_walls.pop();
                            }
                            if ui.add_enabled(!self.selected_walls.is_empty(),
                                Action::new(Icon::Check, "Replace the strip with this ordered wall route")
                                    .label("Place strip")).clicked()
                            {
                                self.apply_wall_route();
                            }
                            ui.add(Action::new(Icon::Help, "Click adjacent walls in the room in the order followed by the real strip. Click the last wall again to remove it."));
                        });
                    }
                    return;
                }
                let (point_index, span_index) = match self.selection {
                    Some(Selection::Point(index)) if index < self.config.room.strip.len() => {
                        (Some(index), index.min(self.config.room.strip.len() - 2))
                    }
                    Some(Selection::Segment(index)) if index + 1 < self.config.room.strip.len() => (None, index),
                    _ => {
                        ui.add(Action::new(Icon::Help, "Select a point or span for position, bend and LED tools. Drag points along the floor; lift handles change one point. Double-click to insert a bend. Middle-drag to orbit."));
                        return;
                    }
                };
                let span = &self.config.room.strip[span_index..=span_index + 1];
                let midpoint = span[0].lerp(span[1], 0.5);
                let span_length = span[0].distance(span[1]);
                let can_add = self.config.room.strip.len() < MAX_POINTS
                    && midpoint.distance(span[0]) >= 0.01
                    && midpoint.distance(span[1]) >= 0.01
                    && (self.config.room.led_anchors.is_empty()
                        || self.config.room.led_anchors[span_index + 1]
                            - self.config.room.led_anchors[span_index] >= 2);
                let can_remove = point_index.is_some_and(|index| {
                    let strip = &self.config.room.strip;
                    strip.len() > 2 && (index == 0 || index + 1 == strip.len()
                        || strip[index - 1].distance(strip[index + 1]) >= 0.01)
                });
                ui.horizontal(|ui| {
                    if let Some(index) = point_index {
                        ui.strong(format!("Point {}", index + 1));
                    } else {
                        ui.strong(format!("Span {}–{}", span_index + 1, span_index + 2));
                        ui.label(format!("{span_length:.2} m"));
                    }
                    if ui.add_enabled(can_add, Action::new(Icon::Plus, "Add a bend halfway along this span; requires room for another point and LED anchor")).clicked() {
                        self.insert_point(span_index, midpoint);
                        self.finish_strip_placement();
                    }
                    if point_index.is_some() && ui.add_enabled(can_remove,
                        Action::new(Icon::Trash, "Delete this point; keep at least two points and 1 cm between neighbors")).clicked()
                    {
                        self.remove_selection();
                        self.finish_strip_placement();
                    }
                    ui.separator();
                    if ui.add(Action::new(Icon::Reverse, "Reverse the strip direction: swap which endpoint is LED 0")
                        .selected(self.config.room.reverse)).clicked()
                    {
                        self.config.room.reverse = !self.config.room.reverse;
                    }
                    if ui.add_enabled(!self.config.room.led_anchors.is_empty(),
                        Action::new(Icon::Reset, "Reset LED allocation to proportional distances along the path")).clicked()
                    {
                        self.config.room.led_anchors.clear();
                    }
                    if let Some(index) = point_index
                        && self.selection == Some(Selection::Point(index))
                    {
                        self.scene_point_allocation(ui, index);
                    }
                });
                // Topology actions above may have changed both indices and selection.
                if let Some(index) = point_index
                    && self.selection == Some(Selection::Point(index))
                {
                    let original = self.config.room.strip[index];
                    let mut point = original;
                    ui.push_id(("point-position", self.config.room.strip.len(), index), |ui| {
                        ui.horizontal(|ui| {
                            for (label, value, limit, tooltip) in [
                                ("X ", &mut point.x, self.config.room.width, "Position across the room width"),
                                ("Y ", &mut point.y, self.config.room.depth, "Position along the room depth"),
                                ("Lift ", &mut point.z, self.config.room.height, "Height of only this point above the floor"),
                            ] {
                                ui.add(egui::DragValue::new(value).prefix(label).suffix(" m")
                                    .range(0.0..=limit).speed(0.02).max_decimals(2)
                                    .custom_parser(decimal_parser).update_while_editing(false)).on_hover_text(tooltip);
                            }
                        });
                    });
                    if point != original {
                        if self.set_strip_point(index, point) {
                            self.finish_strip_placement();
                        } else {
                            self.notify("Keep adjacent strip points at least 0.01 m apart.");
                        }
                    }
                } else if point_index.is_none()
                    && let Some((first, last)) = self.guidance_area()
                {
                    ui.label(RichText::new(format!("LEDs {first}–{last}")).small().color(MUTED));
                }
            },
        );
    }

    fn scene_point_allocation(&mut self, ui: &mut egui::Ui, index: usize) {
        let count = self.config.device.led_count;
        let point_count = self.config.room.strip.len();
        if index == 0 || index + 1 == point_count {
            let last = count - 1;
            let led = if (index == 0) != self.config.room.reverse {
                0
            } else {
                last
            };
            ui.label(format!("LED {led}"))
                .on_hover_text("Endpoint allocation is fixed; reverse direction swaps LED 0.");
            return;
        }
        if count < point_count {
            ui.add_enabled(
                false,
                Action::new(
                    Icon::Leds,
                    "Point allocation needs at least one LED per point",
                ),
            );
            return;
        }
        if self.config.room.led_anchors.is_empty() {
            if ui.add(Action::new(Icon::Leds, "Set the actual LED at this bend; switches from proportional to by-point allocation")
                .label("LED at point")).clicked()
            {
                self.config.room.led_anchors = self.proportional_anchors();
            }
            return;
        }
        let anchors = &mut self.config.room.led_anchors;
        let last = count - 1;
        let lo = anchors[index - 1] + 1;
        let hi = anchors[index + 1] - 1;
        let reverse = self.config.room.reverse;
        let mut led = if reverse {
            last - anchors[index]
        } else {
            anchors[index]
        };
        let range = if reverse {
            last - hi..=last - lo
        } else {
            lo..=hi
        };
        if ui
            .push_id(("point-led", index), |ui| {
                ui.add(
                    egui::DragValue::new(&mut led)
                        .prefix("LED ")
                        .range(range)
                        .update_while_editing(false),
                )
                .on_hover_text(
                    "Actual LED index at this bend; reset restores proportional distances.",
                )
            })
            .inner
            .changed()
        {
            anchors[index] = if reverse { last - led } else { led };
        }
    }

    pub(super) fn wall_picker(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        response: &egui::Response,
        projection: Projection,
    ) {
        let room = &self.config.room;
        let z = self.strip_height_bounds().0;
        let corners = [
            Point { x: 0.0, y: 0.0, z },
            Point {
                x: room.width,
                y: 0.0,
                z,
            },
            Point {
                x: room.width,
                y: room.depth,
                z,
            },
            Point {
                x: 0.0,
                y: room.depth,
                z,
            },
        ];
        let pointer = ui.input(|input| input.pointer.hover_pos());
        let nearest = pointer
            .and_then(|p| {
                (0..4)
                    .map(|i| {
                        (
                            i,
                            closest_on_segment(
                                p,
                                projection.project(corners[i]),
                                projection.project(corners[(i + 1) % 4]),
                            )
                            .1,
                        )
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
            })
            .filter(|(_, distance)| *distance < 16.0)
            .map(|(i, _)| i);
        if response.clicked()
            && self.orbit_origin.is_none()
            && let Some(wall) = nearest
        {
            let mut selection = self.selected_walls.clone();
            if selection.last() == Some(&wall) {
                selection.pop();
            } else {
                selection.push(wall);
            }
            if selection.is_empty() || wall_path(room, &selection, z).is_ok() {
                self.selected_walls = selection;
            }
        }
        for i in 0..4 {
            let selected = self.selected_walls.iter().position(|wall| *wall == i);
            let color = if selected.is_some() {
                ACCENT
            } else if nearest == Some(i) {
                INK
            } else {
                MUTED
            };
            let a = projection.project(corners[i]);
            let b = projection.project(corners[(i + 1) % 4]);
            painter.line_segment(
                [a, b],
                Stroke::new(if selected.is_some() { 5.0 } else { 3.0 }, color),
            );
            let label = selected.map_or_else(
                || ["Back", "Right", "Front", "Left"][i].to_owned(),
                |order| format!("{} · {}", order + 1, ["Back", "Right", "Front", "Left"][i]),
            );
            painter.text(
                a.lerp(b, 0.5) + Vec2::new(0.0, -10.0),
                Align2::CENTER_BOTTOM,
                label,
                FontId::proportional(12.0),
                color,
            );
        }
        if let Ok(points) = wall_path(room, &self.selected_walls, z) {
            let start = projection.project(points[0]);
            let end = projection.project(*points.last().unwrap());
            painter.circle_filled(start, 7.0, CORAL);
            painter.text(
                start + Vec2::new(10.0, 10.0),
                Align2::LEFT_TOP,
                "Start",
                FontId::proportional(12.0),
                CORAL,
            );
            painter.circle_stroke(end, 9.0, Stroke::new(2.0, ACCENT));
        }
    }

    pub(super) fn display_handles(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        projection: Projection,
    ) {
        let Some(Selection::Screen(index)) = self.selection else {
            return;
        };
        let (corners, width, position, angle) = {
            let screen = &self.config.room.screens[index];
            (
                screen_corners(screen).map(|p| projection.project(p)),
                screen.width,
                screen.position,
                screen.angle,
            )
        };
        let resize_pos = corners[2];
        let resize_rect = Rect::from_center_size(resize_pos, Vec2::splat(20.0));
        let resize = ui
            .interact(resize_rect, ui.id().with("display-size"), Sense::drag())
            .on_hover_text("Drag to resize · aspect ratio stays fixed");
        painter.rect_filled(resize_rect, 3.0, CANVAS);
        painter.rect_stroke(
            resize_rect,
            3.0,
            Stroke::new(1.5, ACCENT),
            StrokeKind::Inside,
        );
        painter.line_segment(
            [resize_pos - Vec2::splat(5.0), resize_pos + Vec2::splat(5.0)],
            Stroke::new(2.0, ACCENT),
        );
        if resize.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
        }
        self.transform_display(index, &resize, projection, false);

        let radius = (width * 0.65).clamp(0.35, 2.0);
        let center = Point { z: 0.0, ..position };
        let at = |angle: f32| {
            projection.project(Point {
                x: center.x + radius * angle.cos(),
                y: center.y + radius * angle.sin(),
                z: 0.0,
            })
        };
        let ring: Vec<Pos2> = (0..=48)
            .map(|i| at(i as f32 / 48.0 * std::f32::consts::TAU))
            .collect();
        painter.add(egui::Shape::line(
            ring,
            Stroke::new(1.0, ACCENT.linear_multiply(0.5)),
        ));
        if self.snap_rotation {
            for i in 0..8 {
                painter.circle_filled(at((i as f32 * 45.0).to_radians()), 2.0, MUTED);
            }
        }
        let knob = at(angle.to_radians());
        let rotate = ui
            .interact(
                Rect::from_center_size(knob, Vec2::splat(24.0)),
                ui.id().with("display-rotation"),
                Sense::drag(),
            )
            .on_hover_text("Drag to rotate · 45° snap by default · hold Shift for fine control");
        painter.circle_filled(knob, 9.0, CANVAS);
        painter.circle_stroke(knob, 9.0, Stroke::new(2.0, CORAL));
        painter.circle_filled(knob, 3.0, CORAL);
        if rotate.hovered() || rotate.dragged() {
            painter.text(
                knob + Vec2::new(0.0, 15.0),
                Align2::CENTER_TOP,
                format!("{:.0}°", self.config.room.screens[index].angle),
                FontId::proportional(12.0),
                CORAL,
            );
        }
        self.transform_display(index, &rotate, projection, true);
    }

    fn transform_display(
        &mut self,
        index: usize,
        response: &egui::Response,
        projection: Projection,
        rotation: bool,
    ) {
        if self.orbit_origin.is_some() {
            return;
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pointer) = response.ctx.input(|input| input.pointer.press_origin())
        {
            self.object_drag = None;
            self.display_drag = Some(DisplayDrag {
                index,
                screen: self.config.room.screens[index].clone(),
                projection,
                pointer,
                rotation,
            });
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && let Some(drag) = &self.display_drag
            && drag.index == index
            && drag.rotation == rotation
            && let Some(pointer) = response.interact_pointer_pos()
        {
            if rotation {
                if let (Some(a), Some(b)) = (
                    drag.projection.floor_at(drag.pointer, 0.0),
                    drag.projection.floor_at(pointer, 0.0),
                ) {
                    let center = drag.screen.position;
                    let angle = drag.screen.angle
                        + ((b.y - center.y).atan2(b.x - center.x)
                            - (a.y - center.y).atan2(a.x - center.x))
                        .to_degrees();
                    self.config.room.screens[index].angle = if self.snap_rotation
                        && !response.ctx.input(|input| input.modifiers.shift)
                    {
                        snapped_angle(angle)
                    } else {
                        (angle + 180.0).rem_euclid(360.0) - 180.0
                    };
                }
            } else {
                let center = drag.projection.project(drag.screen.position);
                let corner = drag.projection.project(screen_corners(&drag.screen)[2]);
                let axis = corner - center;
                if axis.length_sq() > 0.01 {
                    self.config.room.screens[index].width = (drag.screen.width
                        * (1.0 + (pointer - drag.pointer).dot(axis) / axis.length_sq()))
                    .clamp(0.1, 5.0);
                }
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.display_drag = None;
        }
    }
}
