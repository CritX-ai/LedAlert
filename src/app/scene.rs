use super::*;

fn quad(painter: &egui::Painter, corners: [Pos2; 4], color: Color32, stroke: Stroke) {
    let mut points = corners.to_vec();
    let area = (0..4)
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % 4];
            a.x * b.y - b.x * a.y
        })
        .sum::<f32>();
    if area < 0.0 {
        points.reverse();
    }
    painter.add(egui::Shape::convex_polygon(points, color, stroke));
}

impl AppState {
    pub(super) fn advance_scene(&mut self, now: Instant) {
        let elapsed = now
            .saturating_duration_since(self.scene_clock)
            .as_secs_f32();
        self.scene_clock = now;
        if !self.sidebar_visible
            && !self.config.reduced_motion
            && self.orbit_origin.is_none()
            && self.scene_motion_ready()
        {
            self.camera_yaw =
                (self.camera_yaw + elapsed.min(0.1) * 0.035).rem_euclid(std::f32::consts::TAU);
        }
    }

    pub(super) fn room_plan(&mut self, ui: &mut egui::Ui) {
        let editing = self.sidebar_visible;
        if !editing {
            self.object_drag = None;
            self.display_drag = None;
            self.resize_origin = None;
        }
        if editing && self.inspector == Inspector::Room && self.outline_editor.open {
            self.outline_plan(ui);
            return;
        }
        ui.horizontal(|ui| {
            ui.heading(if editing {
                match self.inspector {
                    Inspector::Room => "Shape the room",
                    Inspector::Displays => "Place your displays",
                    Inspector::Strip => "Trace the real strip",
                    Inspector::Rules => "Give apps a signal",
                }
            } else {
                "Your room"
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if editing
                    && self.inspector != Inspector::Rules
                    && self.selection.is_some()
                    && ui
                        .add(Action::new(Icon::Close, "Deselect the current object"))
                        .clicked()
                {
                    self.display_drag = None;
                    self.selection = None;
                }
                if ui
                    .add(Action::new(
                        Icon::Reset,
                        "Reset view · middle-drag to orbit the room",
                    ))
                    .clicked()
                {
                    self.camera_yaw = DEFAULT_YAW;
                    self.camera_pitch = DEFAULT_PITCH;
                    self.orbit_origin = None;
                }
            });
        });
        if editing && self.inspector == Inspector::Strip {
            self.scene_strip_controls(ui);
        }
        if editing && self.inspector == Inspector::Rules {
            self.rule_position_controls(ui);
        }
        let (outer, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), ui.available_height().max(120.0)),
            Sense::click_and_drag(),
        );
        self.orbit_camera(ui, outer);
        let Some(fitted) =
            Projection::new(&self.config.room, outer, self.camera_yaw, self.camera_pitch)
        else {
            return;
        };
        let projection = if let Some((start, frame, _, edge)) = &self.resize_origin {
            let displacement = match edge {
                0 => Point {
                    x: start.room.width - self.config.room.width,
                    y: 0.0,
                    z: 0.0,
                },
                2 => Point {
                    x: 0.0,
                    y: start.room.depth - self.config.room.depth,
                    z: 0.0,
                },
                _ => Point {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
            };
            frame.translated(
                frame.project(displacement)
                    - frame.project(Point {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    }),
            )
        } else {
            fitted
        };
        let (w, d, h) = (
            self.config.room.width,
            self.config.room.depth,
            self.config.room.height,
        );
        let painter = ui.painter_at(outer);
        let room = &self.config.room;
        let walls = room.wall_count();
        let outline_valid = room.validate_outline().is_ok();
        for i in 0..walls {
            let a = projection.project(room.corner(i, 0.0));
            let b = projection.project(room.corner((i + 1) % walls, 0.0));
            for (width, alpha) in [(16.0, 4), (10.0, 7), (4.0, 12)] {
                painter.line_segment(
                    [a + Vec2::new(0.0, 8.0), b + Vec2::new(0.0, 8.0)],
                    Stroke::new(width, Color32::from_black_alpha(alpha)),
                );
            }
        }
        outline::paint_floor(&painter, room, |p| projection.project(p), FLOOR);
        let facing = projection.camera_facing();
        let mut order: [usize; MAX_ROOM_VERTICES] = std::array::from_fn(|i| i);
        order[..walls].sort_by(|&a, &b| {
            let center = |i| {
                room.corner(i, h * 0.5)
                    .lerp(room.corner((i + 1) % walls, h * 0.5), 0.5)
            };
            projection
                .depth(center(a))
                .total_cmp(&projection.depth(center(b)))
        });
        for &i in &order[..walls] {
            let a = room.corner(i, 0.0);
            let b = room.corner((i + 1) % walls, 0.0);
            let top_a = Point { z: h, ..a };
            let top_b = Point { z: h, ..b };
            let interior_faces_camera = facing.x * (a.y - b.y) + facing.y * (b.x - a.x) > 0.0;
            if outline_valid && interior_faces_camera {
                quad(
                    &painter,
                    [a, b, top_b, top_a].map(|p| projection.project(p)),
                    if (a.y - b.y).abs() > (a.x - b.x).abs() {
                        Color32::from_rgb(27, 31, 54)
                    } else {
                        Color32::from_rgb(34, 39, 66)
                    },
                    Stroke::new(1.0, LINE),
                );
            } else if outline_valid {
                let pa = projection.project(top_a);
                let pb = projection.project(top_b);
                for segment in (0..20).step_by(2) {
                    painter.line_segment(
                        [
                            pa.lerp(pb, segment as f32 / 20.0),
                            pa.lerp(pb, (segment + 1) as f32 / 20.0),
                        ],
                        Stroke::new(1.0, LINE.linear_multiply(0.55)),
                    );
                }
            }
            painter.line_segment(
                [projection.project(a), projection.project(b)],
                Stroke::new(1.2, if outline_valid { LINE } else { ERROR }),
            );
        }
        for x in 1..12 {
            for y in 1..10 {
                let point = Point {
                    x: w * x as f32 / 12.0,
                    y: d * y as f32 / 10.0,
                    z: 0.0,
                };
                if room.contains_floor(point) {
                    painter.circle_filled(
                        projection.project(point),
                        0.8,
                        Color32::from_rgb(58, 71, 97),
                    );
                }
            }
        }
        let show_displays = !editing
            || !self.guide
            || self.inspector != Inspector::Room
            || self.validation_error.is_some();
        let show_strip = !editing
            || !self.guide
            || matches!(self.inspector, Inspector::Strip | Inspector::Rules)
            || self.validation_error.is_some();
        let motion = if self.config.reduced_motion {
            0.0
        } else {
            0.18
        };
        let display_alpha = if editing {
            ui.ctx()
                .animate_bool_with_time(ui.id().with("display-reveal"), show_displays, motion)
        } else {
            1.0
        };
        let strip_alpha = if editing {
            ui.ctx()
                .animate_bool_with_time(ui.id().with("strip-reveal"), show_strip, motion)
        } else {
            1.0
        };
        if editing && self.inspector == Inspector::Displays && strip_alpha > 0.0 {
            self.paint_wire(&painter, projection, 0.28 * strip_alpha, false);
        }
        if display_alpha > 0.0 {
            let mut indices = [0usize; MAX_SCREENS];
            for (i, value) in indices
                .iter_mut()
                .take(self.config.room.screens.len())
                .enumerate()
            {
                *value = i;
            }
            let order = &mut indices[..self.config.room.screens.len()];
            order.sort_by(|a, b| {
                let a = self.config.room.screens[*a].position;
                let b = self.config.room.screens[*b].position;
                projection.depth(a).total_cmp(&projection.depth(b))
            });
            for &index in order.iter() {
                self.display_in_room(ui, &painter, projection, index, display_alpha);
            }
        }
        if strip_alpha > 0.0 && (!editing || self.inspector != Inspector::Displays) {
            self.paint_wire(
                &painter,
                projection,
                strip_alpha,
                !editing || self.inspector != Inspector::Room,
            );
        }
        if !editing {
            self.scene_rule_markers(ui, &painter, projection, outer);
            return;
        }
        if self.inspector == Inspector::Strip && !self.placing_walls {
            let hover = ui.input(|input| input.pointer.hover_pos());
            let nearest = hover.and_then(|pointer| {
                self.config
                    .room
                    .strip
                    .windows(2)
                    .enumerate()
                    .map(|(index, pair)| {
                        let (t, distance) = closest_on_segment(
                            pointer,
                            projection.project(pair[0]),
                            projection.project(pair[1]),
                        );
                        (index, t, distance)
                    })
                    .min_by(|a, b| a.2.total_cmp(&b.2))
            });
            if let Some((index, _, distance)) = nearest
                && distance <= 12.0
            {
                response.clone().on_hover_text(format!(
                    "Span {}–{} · {:.2} m · click for bend and LED tools · double-click to add a bend",
                    index + 1, index + 2,
                    self.config.room.strip[index].distance(self.config.room.strip[index + 1]),
                ));
            }
            if response.clicked() {
                self.selection = nearest
                    .filter(|(_, _, distance)| *distance <= 12.0)
                    .map(|(index, _, _)| Selection::Segment(index));
            }
            if response.double_clicked()
                && let Some((index, t, distance)) = nearest
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let a = self.config.room.strip[index];
                let b = self.config.room.strip[index + 1];
                let point = if distance <= 12.0 {
                    Some(a.lerp(b, t))
                } else {
                    projection.floor_at(pointer, (a.z + b.z) * 0.5)
                };
                if let Some(mut point) = point {
                    point.x = point.x.clamp(0.0, w);
                    point.y = point.y.clamp(0.0, d);
                    point.z = point.z.clamp(0.0, h);
                    if point.separated_from(a) && point.separated_from(b) {
                        let count = self.config.room.strip.len();
                        self.insert_point(index, point);
                        if self.config.room.strip.len() != count {
                            self.finish_strip_placement();
                        }
                    }
                }
            }
            for index in 0..self.config.room.strip.len() {
                let world = self.config.room.strip[index];
                let actual = projection.project(world);
                let coincident_end = index + 1 == self.config.room.strip.len()
                    && world.distance(self.config.room.strip[0]) < 0.01;
                let pos = if coincident_end {
                    actual + Vec2::new(22.0, -12.0)
                } else {
                    actual
                };
                let selected = self.selection == Some(Selection::Point(index));
                let handle = ui
                    .interact(
                        Rect::from_center_size(pos, Vec2::splat(24.0)),
                        ui.id().with(("strip-point", index)),
                        Sense::click_and_drag(),
                    )
                    .on_hover_text(format!(
                        "Point {} · {:.2} m high · drag along the floor · select for position, lift and LED tools",
                        index + 1, world.z
                    ));
                if handle.clicked() || handle.drag_started_by(egui::PointerButton::Primary) {
                    self.selection = Some(Selection::Point(index));
                }
                self.drag_object(Selection::Point(index), &handle, projection, false);
                let color = if selected || handle.hovered() {
                    ACCENT
                } else {
                    INK
                };
                if coincident_end {
                    painter.line_segment([actual, pos], Stroke::new(1.0, LINE));
                }
                painter.circle_filled(pos, if selected { 8.0 } else { 5.0 }, color);
                painter.circle_stroke(
                    pos,
                    if selected { 8.0 } else { 5.0 },
                    Stroke::new(2.0, CANVAS),
                );
                painter.text(
                    pos + Vec2::new(10.0, 9.0),
                    Align2::LEFT_TOP,
                    format!("{}", index + 1),
                    FontId::proportional(12.0),
                    color,
                );
            }
        } else if self.inspector == Inspector::Room {
            self.room_edges(ui, &painter, &response, projection, fitted);
        } else if response.clicked() {
            self.selection = None;
        }
        if self.inspector == Inspector::Strip && self.placement_open && self.placing_walls {
            self.wall_picker(ui, &painter, &response, projection);
        }
        if self.inspector == Inspector::Displays {
            self.display_handles(ui, &painter, projection);
        }
        if self.inspector == Inspector::Rules {
            self.rule_scene_target(ui, &painter, projection);
        }
        if matches!(self.inspector, Inspector::Displays | Inspector::Strip) && !self.placing_walls {
            self.lift_handle(ui, &painter, projection);
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.resize_origin = None;
        }
        if ui.input(|input| input.pointer.any_released()) {
            self.object_drag = None;
            self.display_drag = None;
        }
    }

    fn room_edges(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        response: &egui::Response,
        projection: Projection,
        fitted: Projection,
    ) {
        if self.orbit_origin.is_some() {
            return;
        }
        let room = &self.config.room;
        let (w, d, h) = (room.width, room.depth, room.height);
        let facing = projection.camera_facing();
        let mut edges = [([Pos2::ZERO; 2], 0usize); MAX_ROOM_VERTICES * 2];
        let mut count = 0;
        for i in 0..room.wall_count() {
            let a = room.corner(i, 0.0);
            let b = room.corner((i + 1) % room.wall_count(), 0.0);
            let code = if a.x == 0.0 && b.x == 0.0 {
                Some(0)
            } else if a.x == w && b.x == w {
                Some(1)
            } else if a.y == 0.0 && b.y == 0.0 {
                Some(2)
            } else if a.y == d && b.y == d {
                Some(3)
            } else {
                None
            };
            if let Some(code) = code {
                edges[count] = ([projection.project(a), projection.project(b)], code);
                count += 1;
            }
            if facing.x * (a.y - b.y) + facing.y * (b.x - a.x) > 0.0 {
                edges[count] = (
                    [
                        projection.project(Point { z: h, ..a }),
                        projection.project(Point { z: h, ..b }),
                    ],
                    4,
                );
                count += 1;
            }
        }
        let edges = &edges[..count];
        let closest = |pointer: Pos2| {
            edges
                .iter()
                .enumerate()
                .map(|(i, (pair, _))| (i, closest_on_segment(pointer, pair[0], pair[1]).1))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .filter(|(_, distance)| *distance <= 14.0)
                .map(|(i, _)| i)
        };
        let hovered = ui
            .input(|input| input.pointer.hover_pos())
            .and_then(closest);
        if hovered.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pointer) = ui.input(|input| input.pointer.press_origin())
            && let Some(edge) = closest(pointer)
        {
            self.resize_origin = Some((self.config.clone(), fitted, pointer, edges[edge].1));
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && let Some((start, frame, origin, edge)) = &self.resize_origin
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let (mut w, mut d, mut h) = (start.room.width, start.room.depth, start.room.height);
            if *edge < 4 {
                if let (Some(a), Some(b)) =
                    (frame.floor_at(*origin, 0.0), frame.floor_at(pointer, 0.0))
                {
                    match edge {
                        0 => w -= b.x - a.x,
                        1 => w += b.x - a.x,
                        2 => d -= b.y - a.y,
                        _ => d += b.y - a.y,
                    }
                }
            } else {
                h += frame.height_delta(pointer - *origin);
            }
            apply_room_size(
                &mut self.config,
                Some(start),
                w.clamp(0.5, 50.0),
                d.clamp(0.5, 50.0),
                h.clamp(0.5, 50.0),
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        for (index, (edge, code)) in edges.iter().enumerate() {
            let active = hovered == Some(index)
                || self
                    .resize_origin
                    .as_ref()
                    .is_some_and(|(_, _, _, i)| i == code);
            painter.line_segment(
                *edge,
                Stroke::new(
                    if active { 3.0 } else { 1.5 },
                    if active { ACCENT } else { LINE },
                ),
            );
            let midpoint = edge[0].lerp(edge[1], 0.5);
            let tangent = (edge[1] - edge[0]).normalized();
            painter.line_segment(
                [midpoint - tangent * 12.0, midpoint + tangent * 12.0],
                Stroke::new(4.0, if active { ACCENT } else { MUTED }),
            );
            if active {
                let (name, value) = if *code < 2 {
                    ("Width", self.config.room.width)
                } else if *code < 4 {
                    ("Depth", self.config.room.depth)
                } else {
                    ("Height", self.config.room.height)
                };
                response
                    .clone()
                    .on_hover_text(format!("{name} · {value:.2} m · drag to resize"));
                if self.resize_origin.is_some() {
                    painter.text(
                        midpoint + Vec2::new(0.0, -16.0),
                        Align2::CENTER_BOTTOM,
                        format!("{name} {value:.2} m"),
                        FontId::proportional(12.0),
                        INK,
                    );
                }
            }
        }
    }

    fn object_position(&self, selection: Selection) -> Option<Point> {
        match selection {
            Selection::Screen(i) => self
                .config
                .room
                .screens
                .get(i)
                .map(|screen| screen.position),
            Selection::Point(i) => self.config.room.strip.get(i).copied(),
            Selection::Segment(_) => None,
        }
    }

    fn drag_object(
        &mut self,
        selection: Selection,
        response: &egui::Response,
        projection: Projection,
        vertical: bool,
    ) {
        if self.orbit_origin.is_some() || self.display_drag.is_some() {
            return;
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pointer) = response.ctx.input(|input| input.pointer.press_origin())
            && let Some(point) = self.object_position(selection)
        {
            self.object_drag = Some(ObjectDrag {
                selection,
                point,
                projection,
                pointer,
                vertical,
            });
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && let Some(drag) = &self.object_drag
            && drag.selection == selection
            && drag.vertical == vertical
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let mut point = drag.point;
            if vertical {
                point.z += drag.projection.height_delta(pointer - drag.pointer);
            } else if let (Some(a), Some(b)) = (
                drag.projection.floor_at(drag.pointer, point.z),
                drag.projection.floor_at(pointer, point.z),
            ) {
                point.x += b.x - a.x;
                point.y += b.y - a.y;
            }
            point.x = point.x.clamp(0.0, self.config.room.width);
            point.y = point.y.clamp(0.0, self.config.room.depth);
            point.z = point.z.clamp(0.0, self.config.room.height);
            match selection {
                Selection::Screen(i) => self.config.room.screens[i].position = point,
                Selection::Point(i) => {
                    self.set_strip_point(i, point);
                }
                Selection::Segment(_) => {}
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            if matches!(selection, Selection::Point(_))
                && let Some(drag) = &self.object_drag
                && self.object_position(selection) != Some(drag.point)
            {
                self.finish_strip_placement();
            }
            self.object_drag = None;
        }
    }

    fn display_in_room(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        projection: Projection,
        index: usize,
        alpha: f32,
    ) {
        let screen = &self.config.room.screens[index];
        let id = screen.id;
        let corners = screen_corners(screen).map(|point| projection.project(point));
        let hit = Rect::from_points(&corners).expand(6.0);
        if self.sidebar_visible && matches!(self.inspector, Inspector::Displays | Inspector::Rules)
        {
            let response = ui
                .interact(
                    hit,
                    ui.id().with(("display", id)),
                    if self.inspector == Inspector::Displays {
                        Sense::click_and_drag()
                    } else {
                        Sense::click()
                    },
                )
                .on_hover_text(if self.inspector == Inspector::Displays {
                    "Drag to move · select for size, rotation and height handles".to_owned()
                } else if let Some(rule) = self.config.rules.get(self.selected_rule) {
                    format!(
                        "Assign {}’s light destination to this display",
                        rule.application
                    )
                } else {
                    "Select an application rule to assign its light destination".to_owned()
                });
            if response.clicked() || response.drag_started_by(egui::PointerButton::Primary) {
                self.selection = Some(Selection::Screen(index));
                if self.inspector == Inspector::Rules
                    && let Some(rule) = self.config.rules.get_mut(self.selected_rule)
                {
                    rule.screen_id = id;
                    rule.options.position = None;
                }
            }
            if self.inspector == Inspector::Displays {
                self.drag_object(Selection::Screen(index), &response, projection, false);
            }
        }
        let screen = &self.config.room.screens[index];
        let selected = self.sidebar_visible
            && if self.inspector == Inspector::Rules {
                self.config
                    .rules
                    .get(self.selected_rule)
                    .is_some_and(|rule| rule.screen_id == id)
            } else {
                self.selection == Some(Selection::Screen(index))
            };
        let corners = screen_corners(screen).map(|point| projection.project(point));
        let pos = projection.project(screen.position);
        let foot = projection.project(Point {
            z: 0.0,
            ..screen.position
        });
        if self.sidebar_visible && (selected || self.inspector == Inspector::Displays) {
            painter.line_segment([pos, foot], Stroke::new(1.0, LINE.linear_multiply(alpha)));
            painter.circle_stroke(foot, 4.0, Stroke::new(1.0, LINE.linear_multiply(alpha)));
        }
        let color = if !self.config.room.contains_floor(screen.position) {
            ERROR
        } else if selected {
            ACCENT
        } else {
            MUTED
        }
        .linear_multiply(alpha);
        quad(
            painter,
            corners,
            Color32::from_rgb(15, 23, 41).linear_multiply(alpha),
            Stroke::new(if selected { 2.0 } else { 1.2 }, color),
        );
        // A small in-plane accent gives the upright surface a recognizable face.
        let inset = corners.map(|p| pos + (p - pos) * 0.86);
        painter.line_segment(
            [inset[0], inset[1]],
            Stroke::new(
                2.0,
                if selected {
                    ACCENT
                } else {
                    Color32::from_rgb(58, 91, 111)
                }
                .linear_multiply(alpha),
            ),
        );
        let bottom = screen.position.z - screen.width / screen.aspect_ratio * 0.5;
        let stem = projection.project(Point {
            z: bottom,
            ..screen.position
        });
        let base = projection.project(Point {
            z: bottom - screen.width * 0.08,
            ..screen.position
        });
        painter.line_segment([stem, base], Stroke::new(2.0, color));
        let (sin, cos) = screen.angle.to_radians().sin_cos();
        let base_world = Point {
            z: bottom - screen.width * 0.08,
            ..screen.position
        };
        let a = projection.project(Point {
            x: base_world.x - screen.width * 0.18 * cos,
            y: base_world.y - screen.width * 0.18 * sin,
            ..base_world
        });
        let b = projection.project(Point {
            x: base_world.x + screen.width * 0.18 * cos,
            y: base_world.y + screen.width * 0.18 * sin,
            ..base_world
        });
        painter.line_segment([a, b], Stroke::new(2.0, color));
        let mut number = painter.layout_no_wrap(
            (index + 1).to_string(),
            FontId::proportional(32.0),
            Color32::PLACEHOLDER,
        );
        crate::spatial::project_screen_galley(
            projection,
            screen,
            std::sync::Arc::make_mut(&mut number),
        );
        painter.galley(Pos2::ZERO, number, color);
        if !self.sidebar_visible || self.inspector != Inspector::Room {
            let label = Rect::from_points(&corners).center_bottom() + Vec2::new(0.0, 12.0);
            painter.text(
                label,
                Align2::CENTER_TOP,
                &screen.name,
                FontId::proportional(12.0),
                INK.linear_multiply(alpha),
            );
        }
    }

    fn paint_wire(
        &self,
        painter: &egui::Painter,
        projection: Projection,
        alpha: f32,
        activity: bool,
    ) {
        for (index, pair) in self.config.room.strip.windows(2).enumerate() {
            let points = [projection.project(pair[0]), projection.project(pair[1])];
            let selected =
                self.sidebar_visible && self.selection == Some(Selection::Segment(index));
            if selected {
                painter.line_segment(
                    points,
                    Stroke::new(11.0, ACCENT.linear_multiply(0.12 * alpha)),
                );
            }
            painter.line_segment(
                points,
                Stroke::new(
                    if selected { 4.0 } else { 3.0 },
                    if !self.config.room.contains_floor_segment(pair[0], pair[1]) {
                        ERROR
                    } else if selected {
                        ACCENT
                    } else {
                        Color32::from_rgb(110, 131, 160)
                    }
                    .linear_multiply(alpha),
                ),
            );
        }
        if activity && self.validation_error.is_none() {
            let shown = if self.sidebar_visible && self.preview.is_active() {
                &self.preview
            } else {
                &self.engine
            };
            let guidance = self.sidebar_visible && self.guidance.active();
            let pixels = if guidance {
                self.guidance.frame()
            } else {
                shown.frame()
            };
            let brightness = if guidance {
                0.1
            } else {
                self.config.brightness
            };
            for (point, pixel) in shown
                .positions()
                .iter()
                .zip(pixels)
                .step_by(pixels.len().div_ceil(1600).max(1))
            {
                if *pixel != [0; 3] {
                    let p = projection.project(*point);
                    let color = preview_color(*pixel, brightness).linear_multiply(alpha);
                    painter.circle_filled(p, 5.0, color.linear_multiply(0.10));
                    painter.circle_filled(p, 2.4, color);
                }
            }
        }
        let start = if self.config.room.reverse {
            self.config.room.strip.last()
        } else {
            self.config.room.strip.first()
        };
        if (self.inspector == Inspector::Strip || !self.sidebar_visible)
            && let Some(start) = start
        {
            painter.text(
                projection.project(*start) + Vec2::new(-12.0, -14.0),
                Align2::RIGHT_BOTTOM,
                "LED 0",
                FontId::proportional(11.0),
                MUTED,
            );
        }
    }

    fn lift_handle(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, projection: Projection) {
        let Some(selection) = self.selection else {
            return;
        };
        let Some(point) = self.object_position(selection) else {
            return;
        };
        let p = projection.project(point);
        let floor = projection.project(Point { z: 0.0, ..point });
        painter.line_segment([floor, p], Stroke::new(1.0, ACCENT.linear_multiply(0.4)));
        painter.circle_stroke(floor, 5.0, Stroke::new(1.0, ACCENT.linear_multiply(0.5)));
        let handle = p + Vec2::new(0.0, -42.0);
        painter.line_segment([p, handle], Stroke::new(1.5, ACCENT));
        let rect = Rect::from_center_size(handle, Vec2::new(22.0, 26.0));
        let response = ui
            .interact(rect, ui.id().with("lift-handle"), Sense::drag())
            .on_hover_text(if matches!(selection, Selection::Point(_)) {
                "Drag up or down to lift only this point"
            } else {
                "Drag up or down to change this display’s height"
            });
        painter.rect_filled(rect, 4.0, CANVAS);
        painter.rect_stroke(rect, 4.0, Stroke::new(1.0, ACCENT), StrokeKind::Inside);
        if response.hovered() || response.dragged() {
            painter.text(
                handle + Vec2::new(16.0, 0.0),
                Align2::LEFT_CENTER,
                format!("{:.2} m", point.z),
                FontId::proportional(12.0),
                INK,
            );
        }
        painter.line_segment(
            [handle + Vec2::new(0.0, 7.0), handle - Vec2::new(0.0, 7.0)],
            Stroke::new(1.5, ACCENT),
        );
        for sign in [-1.0, 1.0] {
            painter.line_segment(
                [
                    handle + Vec2::new(-4.0, sign * 3.0),
                    handle + Vec2::new(0.0, sign * 7.0),
                ],
                Stroke::new(1.5, ACCENT),
            );
            painter.line_segment(
                [
                    handle + Vec2::new(4.0, sign * 3.0),
                    handle + Vec2::new(0.0, sign * 7.0),
                ],
                Stroke::new(1.5, ACCENT),
            );
        }
        self.drag_object(selection, &response, projection, true);
    }
}
