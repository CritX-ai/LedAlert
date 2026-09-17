//! Orthographic cutaway geometry, pointer-plane mapping and ordered wall routes.

use anyhow::{Result, ensure};
use eframe::egui::{Pos2, Rect, Vec2};

use crate::config::{Point, Room, Screen};

/// Incumbent isometric view, in radians. Positive yaw orbits about world z.
pub const DEFAULT_YAW: f32 = std::f32::consts::FRAC_PI_4;
pub const DEFAULT_PITCH: f32 = 0.615_479_7;
const SINGULAR_EPSILON: f32 = 1e-6;
const INSET: f32 = 28.0;

/// A fixed camera frame, independent of later room edits. Copy at drag start so
/// resizing a room cannot change the pointer-to-world mapping mid-gesture.
#[derive(Clone, Copy, Debug)]
pub struct Projection {
    origin: Pos2,
    scale: f32,
    right: Point,
    down: Point,
    facing: Point,
}

impl Projection {
    /// Fit the eight room corners with a 28-point inset. No model dimensions are
    /// clamped; nonfinite, nonpositive or numerically unusable geometry is rejected.
    pub fn new(room: &Room, rect: Rect, yaw: f32, pitch: f32) -> Option<Self> {
        if !valid_room_dimensions(room)
            || !rect.min.is_finite()
            || !rect.max.is_finite()
            || !yaw.is_finite()
            || !pitch.is_finite()
        {
            return None;
        }
        let (yaw_sin, yaw_cos) = yaw.sin_cos();
        let (pitch_sin, pitch_cos) = pitch.sin_cos();
        // The floor inverse and vertical editing must both remain well-conditioned.
        if pitch_sin.abs() <= SINGULAR_EPSILON || pitch_cos.abs() <= SINGULAR_EPSILON {
            return None;
        }
        let right = Point {
            x: yaw_cos,
            y: -yaw_sin,
            z: 0.0,
        };
        let down = Point {
            x: pitch_sin * yaw_sin,
            y: pitch_sin * yaw_cos,
            z: -pitch_cos,
        };
        let facing = Point {
            x: pitch_cos * yaw_sin,
            y: pitch_cos * yaw_cos,
            z: pitch_sin,
        };
        let available = rect.shrink(INSET);
        let size = available.size();
        if !size.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
            return None;
        }
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for x in [0.0, room.width] {
            for y in [0.0, room.depth] {
                for z in [0.0, room.height] {
                    let point = Point { x, y, z };
                    let projected = Vec2::new(dot(right, point), dot(down, point));
                    if !projected.is_finite() || !dot(facing, point).is_finite() {
                        return None;
                    }
                    min = min.min(projected);
                    max = max.max(projected);
                }
            }
        }
        let extent = max - min;
        if !extent.is_finite() || extent.x <= 0.0 || extent.y <= 0.0 {
            return None;
        }
        let scale = (size.x / extent.x).min(size.y / extent.y);
        if !scale.is_finite()
            || scale <= 0.0
            || !(1.0 / (scale * pitch_sin)).is_finite()
            || !(1.0 / (scale * pitch_cos)).is_finite()
        {
            return None;
        }
        let origin = available.min + size * 0.5 - (min + extent * 0.5) * scale;
        if !origin.is_finite() {
            return None;
        }
        Some(Self {
            origin,
            scale,
            right,
            down,
            facing,
        })
    }

    pub fn project(self, point: Point) -> Pos2 {
        self.origin + Vec2::new(dot(self.right, point), dot(self.down, point)) * self.scale
    }

    /// Signed camera-space distance in metres, increasing toward the camera.
    pub fn depth(self, point: Point) -> f32 {
        dot(self.facing, point)
    }

    /// Unit world-space vector toward the camera, independent of pan and fit.
    pub fn camera_facing(self) -> Point {
        self.facing
    }

    /// Intersect the camera ray with a horizontal plane at world z. Results may
    /// lie outside the room; callers own edit bounds.
    pub fn floor_at(self, pos: Pos2, z: f32) -> Option<Point> {
        let delta = (pos - self.origin) / self.scale;
        let horizontal = (delta.y - self.down.z * z) / self.facing.z;
        finite_point(Point {
            x: self.right.x * delta.x - self.right.y * horizontal,
            y: self.right.y * delta.x + self.right.x * horizontal,
            z,
        })
    }

    /// Intersect the camera ray with a vertical plane at world y, without clamping.
    /// An edge-on plane has no unique intersection and returns `None`.
    pub fn elevation_at(self, pos: Pos2, y: f32) -> Option<Point> {
        if self.facing.y.abs() <= SINGULAR_EPSILON {
            return None;
        }
        let delta = (pos - self.origin) / self.scale;
        let x = (delta.x - self.right.y * y) / self.right.x;
        finite_point(Point {
            x,
            y,
            z: (delta.y - self.down.x * x - self.down.y * y) / self.down.z,
        })
    }

    /// World-z displacement along the projected vertical axis; horizontal pointer
    /// motion does not affect height.
    pub fn height_delta(self, delta: Vec2) -> f32 {
        delta.y / (self.scale * self.down.z)
    }

    /// Pan in logical screen points, keeping forward and inverse maps consistent.
    /// An unrepresentable translation leaves the frame unchanged.
    pub fn translated(self, delta: Vec2) -> Self {
        let origin = self.origin + delta;
        if origin.is_finite() {
            Self { origin, ..self }
        } else {
            self
        }
    }

    /// Logical screen points per orthographic camera-space metre.
    pub fn scale(self) -> f32 {
        self.scale
    }
}

/// Project laid-out font vertices onto an upright display face. Keep texel UVs
/// intact so egui normalizes them against the final atlas size at tessellation.
/// The caller owns a small copy of the cached galley, never the cached original.
pub fn project_screen_galley(
    projection: Projection,
    screen: &Screen,
    galley: &mut eframe::egui::Galley,
) {
    let bounds = galley.mesh_bounds;
    if !bounds.is_finite() || bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return;
    }
    let [tl, tr, _, bl] = screen_corners(screen).map(|point| projection.project(point));
    let height = screen.width / screen.aspect_ratio;
    let metres_per_point =
        (screen.width * 0.65 / bounds.width()).min(height * 0.58 / bounds.height());
    let right = (tr - tl) * (metres_per_point / screen.width);
    let down = (bl - tl) * (metres_per_point / height);
    let center = projection.project(screen.position);
    let mut projected_bounds = Rect::NOTHING;
    for placed in &mut galley.rows {
        let offset = placed.pos.to_vec2();
        let row = std::sync::Arc::make_mut(&mut placed.row);
        let mut row_bounds = Rect::NOTHING;
        for vertex in &mut row.visuals.mesh.vertices {
            let local = vertex.pos + offset - bounds.center();
            vertex.pos = center + right * local.x + down * local.y;
            row_bounds.extend_with(vertex.pos);
        }
        row.visuals.mesh_bounds = row_bounds;
        projected_bounds = projected_bounds.union(row_bounds);
        placed.pos = Pos2::ZERO;
    }
    galley.rect = projected_bounds;
    galley.mesh_bounds = projected_bounds;
}

fn dot(a: Point, b: Point) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn valid_room_dimensions(room: &Room) -> bool {
    [room.width, room.depth, room.height]
        .into_iter()
        .all(|dimension| dimension.is_finite() && dimension > 0.0)
}

/// Route along ordered, unique adjacent walls at a fixed room height.
/// Rectangle corners retain (0,0), (width,0), (width,depth), (0,depth):
/// walls 0 Back, 1 Right, 2 Front, 3 Left. Outlines follow their stored corner order.
/// A single wall follows that order; selecting every wall closes the perimeter.
pub fn wall_path(room: &Room, walls: &[usize], height: f32) -> Result<Vec<Point>> {
    ensure!(
        valid_room_dimensions(room),
        "Room dimensions must be finite and positive"
    );
    ensure!(
        height.is_finite() && (0.0..=room.height).contains(&height),
        "Wall route height must be within the room"
    );
    room.validate_outline()?;
    let count = room.wall_count();
    ensure!(
        (1..=count).contains(&walls.len()),
        "Select between one and {count} walls"
    );
    let mut selected = 0u16;
    for &wall in walls {
        ensure!(wall < count, "Wall index is outside the room outline");
        let bit = 1u16 << wall;
        ensure!(selected & bit == 0, "Each wall may be selected only once");
        selected |= bit;
    }
    let step = if walls.len() == 1 {
        1
    } else {
        (walls[1] + count - walls[0]) % count
    };
    ensure!(
        step == 1 || step == count - 1,
        "Selected walls must be adjacent"
    );
    ensure!(
        walls
            .windows(2)
            .all(|pair| (pair[0] + step) % count == pair[1]),
        "Selected walls must follow a continuous direction"
    );
    let start = if step == 1 {
        walls[0]
    } else {
        (walls[0] + 1) % count
    };
    let mut path = Vec::with_capacity(walls.len() + 1);
    path.push(room.corner(start, height));
    for &wall in walls {
        path.push(room.corner(if step == 1 { (wall + 1) % count } else { wall }, height));
    }
    Ok(path)
}

fn finite_point(point: Point) -> Option<Point> {
    (point.x.is_finite() && point.y.is_finite() && point.z.is_finite()).then_some(point)
}

/// Upright face corners in top-left, top-right, bottom-right, bottom-left order.
/// Angle is yaw in degrees around world z; aspect ratio is width / true height.
pub fn screen_corners(screen: &Screen) -> [Point; 4] {
    let (sin, cos) = screen.angle.to_radians().sin_cos();
    let half_width = screen.width * 0.5;
    let half_height = half_width / screen.aspect_ratio;
    let horizontal = Vec2::new(cos, sin) * half_width;
    let center = screen.position;
    [
        Point {
            x: center.x - horizontal.x,
            y: center.y - horizontal.y,
            z: center.z + half_height,
        },
        Point {
            x: center.x + horizontal.x,
            y: center.y + horizontal.y,
            z: center.z + half_height,
        },
        Point {
            x: center.x + horizontal.x,
            y: center.y + horizontal.y,
            z: center.z - half_height,
        },
        Point {
            x: center.x - horizontal.x,
            y: center.y - horizontal.y,
            z: center.z - half_height,
        },
    ]
}

/// Closest clamped segment parameter and distance in logical screen points.
/// A zero-length segment selects its single endpoint. Wider intermediates avoid
/// overflow when squaring large but finite screen coordinates.
pub fn closest_on_segment(p: Pos2, a: Pos2, b: Pos2) -> (f32, f32) {
    let dx = f64::from(b.x) - f64::from(a.x);
    let dy = f64::from(b.y) - f64::from(a.y);
    let px = f64::from(p.x) - f64::from(a.x);
    let py = f64::from(p.y) - f64::from(a.y);
    let length_squared = dx * dx + dy * dy;
    let t = if length_squared > 0.0 {
        ((px * dx + py * dy) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (t as f32, (px - t * dx).hypot(py - t * dy) as f32)
}
