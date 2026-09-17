use super::*;
use crate::config::Room;

const RECTANGLE: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
const L_SHAPE: [[f32; 2]; 6] = [
    [0.0, 0.0],
    [1.0, 0.0],
    [1.0, 0.55],
    [0.55, 0.55],
    [0.55, 1.0],
    [0.0, 1.0],
];

#[derive(Clone, Copy)]
enum Handle {
    Corner(usize),
    Edge(usize),
}

struct OutlineDrag {
    points: Vec<[f32; 2]>,
    handle: Handle,
    pointer: Pos2,
    scale: f32,
    width: f32,
    depth: f32,
}

pub(super) struct OutlineEditor {
    pub(super) open: bool,
    selected: Option<usize>,
    snap: bool,
    drag: Option<OutlineDrag>,
}

impl Default for OutlineEditor {
    fn default() -> Self {
        Self {
            open: false,
            selected: None,
            snap: true,
            drag: None,
        }
    }
}

impl OutlineEditor {
    pub(super) fn cancel_drag(&mut self) {
        self.selected = None;
        self.drag = None;
    }
}

/// Mesh triangles, not a convex fill: a concave cutout must stay empty.
pub(super) fn paint_floor(
    painter: &egui::Painter,
    room: &Room,
    project: impl Fn(Point) -> Pos2,
    color: Color32,
) {
    let Some(floor) = room.floor_mesh() else {
        return;
    };
    let mut mesh = egui::Mesh {
        vertices: Vec::with_capacity(floor.vertex_count),
        indices: Vec::with_capacity(floor.triangle_count * 3),
        ..Default::default()
    };
    for &point in &floor.vertices[..floor.vertex_count] {
        mesh.colored_vertex(project(point), color);
    }
    for &[a, b, c] in &floor.triangles[..floor.triangle_count] {
        mesh.add_triangle(a as u32, b as u32, c as u32);
    }
    painter.add(mesh);
}

fn topology_button(
    ui: &mut egui::Ui,
    label: &str,
    points: &[[f32; 2]],
    selected: bool,
) -> egui::Response {
    let response = ui.add(
        egui::Button::new(label)
            .selected(selected)
            .min_size(Vec2::new(126.0, 64.0)),
    );
    let origin = Pos2::new(response.rect.center().x - 14.0, response.rect.top() + 5.0);
    let at = |p: [f32; 2]| origin + Vec2::new(p[0] * 28.0, p[1] * 19.0);
    for i in 0..points.len() {
        ui.painter().line_segment(
            [at(points[i]), at(points[(i + 1) % points.len()])],
            Stroke::new(1.5, if selected { ACCENT } else { MUTED }),
        );
    }
    response
}

impl AppState {
    fn replace_outline(&mut self, points: Vec<[f32; 2]>) {
        if points == self.config.room.outline {
            return;
        }
        self.stop_guidance(StopReason::Navigation);
        self.config.room.outline = points;
        self.outline_editor.cancel_drag();
        self.selected_walls.clear();
        self.placing_walls = false;
        self.record_discrete();
    }

    fn editable_outline(&self) -> Vec<[f32; 2]> {
        if self.config.room.outline.is_empty() {
            RECTANGLE.to_vec()
        } else {
            self.config.room.outline.clone()
        }
    }

    pub(super) fn outline_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if topology_button(
                ui,
                "Rectangle",
                &RECTANGLE,
                self.config.room.outline.is_empty(),
            )
            .clicked()
            {
                self.replace_outline(Vec::new());
            }
            if topology_button(
                ui,
                "L-shaped",
                &L_SHAPE,
                self.config.room.outline == L_SHAPE,
            )
            .clicked()
            {
                self.replace_outline(L_SHAPE.to_vec());
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.outline_editor.snap, "Snap · 5 cm");
            if ui.add_enabled(!self.config.room.outline.is_empty(),
                Action::new(Icon::Reverse, "Rotate the outline 90°. Displays, strip points and LED allocation stay unchanged.").label("Rotate outline")).clicked() {
                let points = self.config.room.outline.iter().map(|p| [1.0 - p[1], p[0]]).collect();
                self.replace_outline(points);
            }
        });
        if let Some(index) = self
            .outline_editor
            .selected
            .filter(|&i| i < self.config.room.wall_count())
        {
            egui::CollapsingHeader::new(format!("Corner {} measurements", index + 1))
                .id_salt("outline-corner-measurements")
                .show(ui, |ui| {
                    let point = self.config.room.corner(index, 0.0);
                    let (mut x, mut y) = (point.x, point.y);
                    scalar(ui, "Across", &mut x, 0.0..=self.config.room.width, " m");
                    scalar(ui, "Along", &mut y, 0.0..=self.config.room.depth, " m");
                    if (x, y) != (point.x, point.y) {
                        if self.config.room.outline.is_empty() {
                            self.config.room.outline = RECTANGLE.to_vec();
                        }
                        self.config.room.outline[index] =
                            [x / self.config.room.width, y / self.config.room.depth];
                    }
                });
        }
        if let Err(error) = self.config.room.validate_outline() {
            ui.colored_label(ERROR, error.to_string());
            ui.label(
                "Move a corner back or Undo. Lighting is paused; the saved setup is unchanged.",
            );
        } else {
            let screens = self
                .config
                .room
                .screens
                .iter()
                .filter(|s| !self.config.room.contains_floor(s.position))
                .count();
            let spans = self
                .config
                .room
                .strip
                .windows(2)
                .filter(|p| !self.config.room.contains_floor_segment(p[0], p[1]))
                .count();
            if screens + spans > 0 {
                ui.add_space(10.0);
                ui.colored_label(
                    ERROR,
                    format!("Outside footprint: {screens} displays, {spans} strip spans"),
                );
                ui.label("Positions are preserved. Adjust the outline or move the affected objects before saving.");
                ui.horizontal(|ui| {
                    if screens > 0 && ui.button("Edit displays").clicked() {
                        self.select_step(Inspector::Displays);
                    }
                    if spans > 0 && ui.button("Edit strip").clicked() {
                        self.select_step(Inspector::Strip);
                    }
                });
            }
        }
    }

    pub(super) fn outline_plan(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Edit room outline");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(
                        Action::new(
                            Icon::Check,
                            "Return to the 3D room; this does not save or enable lighting.",
                        )
                        .label("Done"),
                    )
                    .clicked()
                {
                    self.outline_editor.open = false;
                    self.outline_editor.cancel_drag();
                }
            });
        });
        ui.horizontal(|ui| {
            let selected = self.outline_editor.selected.filter(|&i| i < self.config.room.wall_count());
            if ui.add_enabled(selected.is_some() && self.config.room.wall_count() < MAX_ROOM_VERTICES,
                Action::new(Icon::Plus, "Insert a corner after the selected one, on its outgoing edge.").label("Add corner")).clicked() {
                let index = selected.unwrap();
                let mut points = self.editable_outline();
                let a = points[index];
                let b = points[(index + 1) % points.len()];
                points.insert(index + 1, [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]);
                self.replace_outline(points);
                self.outline_editor.selected = Some(index + 1);
            }
            if ui.add_enabled(selected.is_some() && self.config.room.wall_count() > 3,
                Action::new(Icon::Trash, "Remove the selected corner. Existing strip points and LED indices are not changed.")).clicked() {
                let mut points = self.editable_outline();
                points.remove(selected.unwrap());
                self.replace_outline(points);
            }
            ui.add(Action::new(Icon::Help, "Drag corners or edge grips. Select a corner for precise measurements. Up to 12 corners; no crossing edges or holes. Invalid drafts pause lighting and cannot replace a valid save."));
        });
        let (outer, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), ui.available_height().max(120.0)),
            Sense::hover(),
        );
        let available = outer.shrink(42.0);
        let room = &self.config.room;
        let scale = (available.width() / room.width)
            .min(available.height() / room.depth)
            .max(1.0);
        let origin = available.center() - Vec2::new(room.width, room.depth) * scale * 0.5;
        let at = |p: Point| origin + Vec2::new(p.x, p.y) * scale;
        let painter = ui.painter_at(outer);
        let valid = room.validate_outline().is_ok();
        paint_floor(&painter, room, at, FLOOR);
        for pair in room.strip.windows(2) {
            painter.line_segment(
                [at(pair[0]), at(pair[1])],
                Stroke::new(
                    2.0,
                    if room.contains_floor_segment(pair[0], pair[1]) {
                        MUTED.linear_multiply(0.6)
                    } else {
                        ERROR
                    },
                ),
            );
        }
        for screen in &room.screens {
            let pos = at(screen.position);
            let color = if room.contains_floor(screen.position) {
                MUTED
            } else {
                ERROR
            };
            painter.rect_stroke(
                Rect::from_center_size(pos, Vec2::new(18.0, 11.0)),
                1.0,
                Stroke::new(1.5, color),
                StrokeKind::Inside,
            );
            painter.text(
                pos + Vec2::new(0.0, 10.0),
                Align2::CENTER_TOP,
                &screen.name,
                FontId::proportional(12.0),
                color,
            );
        }
        let count = room.wall_count();
        let corners: [Point; MAX_ROOM_VERTICES] =
            std::array::from_fn(|i| room.corner(i % count, 0.0));
        for i in 0..count {
            let a = at(corners[i]);
            let b = at(corners[(i + 1) % count]);
            painter.line_segment([a, b], Stroke::new(2.0, if valid { LINE } else { ERROR }));
            let center = a.lerp(b, 0.5);
            let response = ui
                .interact(
                    Rect::from_center_size(center, Vec2::splat(22.0)),
                    ui.id().with(("outline-edge", i)),
                    Sense::drag(),
                )
                .on_hover_text(format!(
                    "Wall {} · {:.2} m · drag to move this edge",
                    i + 1,
                    corners[i].distance(corners[(i + 1) % count])
                ));
            let tangent = (b - a).normalized();
            painter.line_segment(
                [center - tangent * 8.0, center + tangent * 8.0],
                Stroke::new(
                    4.0,
                    if response.hovered() || response.dragged() {
                        ACCENT
                    } else {
                        MUTED
                    },
                ),
            );
            self.drag_outline(&response, Handle::Edge(i), scale);
        }
        for (i, point) in corners.iter().enumerate().take(count) {
            let pos = at(*point);
            let response = ui
                .interact(
                    Rect::from_center_size(pos, Vec2::splat(26.0)),
                    ui.id().with(("outline-corner", i)),
                    Sense::click_and_drag(),
                )
                .on_hover_text(format!(
                    "Corner {} · drag to reshape; click for measurements",
                    i + 1
                ));
            if response.clicked() || response.drag_started() {
                self.outline_editor.selected = Some(i);
            }
            let selected = self.outline_editor.selected == Some(i);
            painter.circle_filled(
                pos,
                if selected { 7.0 } else { 5.0 },
                if selected || response.hovered() {
                    ACCENT
                } else {
                    INK
                },
            );
            painter.circle_stroke(
                pos,
                if selected { 7.0 } else { 5.0 },
                Stroke::new(2.0, CANVAS),
            );
            painter.text(
                pos + Vec2::new(12.0, -12.0),
                Align2::LEFT_BOTTOM,
                format!("{}", i + 1),
                FontId::proportional(12.0),
                MUTED,
            );
            self.drag_outline(&response, Handle::Corner(i), scale);
        }
        if ui.input(|input| input.pointer.any_released()) {
            self.outline_editor.drag = None;
        }
    }

    fn drag_outline(&mut self, response: &egui::Response, handle: Handle, scale: f32) {
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pointer) = response.ctx.input(|input| input.pointer.press_origin())
        {
            self.stop_guidance(StopReason::Navigation);
            self.outline_editor.drag = Some(OutlineDrag {
                points: self.editable_outline(),
                handle,
                pointer,
                scale,
                width: self.config.room.width,
                depth: self.config.room.depth,
            });
        }
        if response.hovered() || response.dragged() {
            response.ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
        if !response.dragged_by(egui::PointerButton::Primary) {
            return;
        }
        let Some(drag) = &self.outline_editor.drag else {
            return;
        };
        let Some(pointer) = response.interact_pointer_pos() else {
            return;
        };
        let mut delta = (pointer - drag.pointer) / drag.scale;
        let mut points = [[0.0; 2]; MAX_ROOM_VERTICES];
        let count = drag.points.len();
        points[..count].copy_from_slice(&drag.points);
        match drag.handle {
            Handle::Corner(index) => {
                let mut x = drag.points[index][0] * drag.width + delta.x;
                let mut y = drag.points[index][1] * drag.depth + delta.y;
                if self.outline_editor.snap {
                    x = (x / 0.05).round() * 0.05;
                    y = (y / 0.05).round() * 0.05;
                }
                points[index] = [
                    (x / drag.width).clamp(0.0, 1.0),
                    (y / drag.depth).clamp(0.0, 1.0),
                ];
            }
            Handle::Edge(index) => {
                if self.outline_editor.snap {
                    delta.x = (delta.x / 0.05).round() * 0.05;
                    delta.y = (delta.y / 0.05).round() * 0.05;
                }
                let a = drag.points[index];
                let next = (index + 1) % count;
                let b = drag.points[next];
                let dx = (delta.x / drag.width).clamp(-a[0].min(b[0]), 1.0 - a[0].max(b[0]));
                let dy = (delta.y / drag.depth).clamp(-a[1].min(b[1]), 1.0 - a[1].max(b[1]));
                points[index] = [a[0] + dx, a[1] + dy];
                points[next] = [b[0] + dx, b[1] + dy];
            }
        }
        if self.config.room.outline.is_empty() && points[..count] == RECTANGLE {
            return;
        }
        if self.config.room.outline.as_slice() != &points[..count] {
            self.config.room.outline.clear();
            self.config.room.outline.extend_from_slice(&points[..count]);
            self.selected_walls.clear();
        }
        response.ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    }
}
