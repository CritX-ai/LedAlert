//! Bounded, allocation-free floor geometry shared by validation and room editing.
use crate::config::{MAX_ROOM_VERTICES, Point, Room};

/// A triangulated floor in room coordinates. Only the counted prefixes are populated.
#[derive(Clone, Copy, Debug)]
pub struct FloorMesh {
    pub vertices: [Point; MAX_ROOM_VERTICES],
    pub vertex_count: usize,
    pub triangles: [[usize; 3]; MAX_ROOM_VERTICES - 2],
    pub triangle_count: usize,
}

const ZERO: Point = Point {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};
type Xy = [f64; 2];

impl Room {
    pub fn wall_count(&self) -> usize {
        if self.outline.is_empty() {
            4
        } else {
            self.outline.len()
        }
    }

    /// Cyclic corner lookup. The empty outline keeps the legacy rectangle order.
    pub fn corner(&self, index: usize, z: f32) -> Point {
        let [x, y] = if self.outline.is_empty() {
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]][index % 4]
        } else {
            self.outline[index % self.outline.len()]
        };
        Point {
            x: x * self.width,
            y: y * self.depth,
            z,
        }
    }

    /// Validate topology and physical edges without consulting display or strip placement.
    pub fn validate_outline(&self) -> anyhow::Result<()> {
        Polygon::new(self).map(|_| ()).map_err(anyhow::Error::msg)
    }

    /// Inclusive floor containment; z is deliberately ignored. Invalid outlines fail closed.
    pub fn contains_floor(&self, point: Point) -> bool {
        if !self.floor_bounds_contain(point) {
            return false;
        }
        self.outline.is_empty()
            || Polygon::new(self).is_ok_and(|polygon| polygon.contains(xy(point)))
    }

    /// The entire XY segment must lie in the footprint, including boundary overlaps.
    /// Checking endpoints alone is insufficient for concave rooms. Height is independent.
    pub fn contains_floor_segment(&self, a: Point, b: Point) -> bool {
        if !self.floor_bounds_contain(a) || !self.floor_bounds_contain(b) {
            return false;
        }
        if self.outline.is_empty() {
            return true;
        }
        let Ok(polygon) = Polygon::new(self) else {
            return false;
        };
        polygon.contains_segment(xy(a), xy(b))
    }

    /// Ear clipping never substitutes a triangle fan on failure: cutouts stay unfilled.
    pub fn floor_mesh(&self) -> Option<FloorMesh> {
        let polygon = Polygon::new(self).ok()?;
        let mut mesh = FloorMesh {
            vertices: [ZERO; MAX_ROOM_VERTICES],
            vertex_count: polygon.count,
            triangles: [[0; 3]; MAX_ROOM_VERTICES - 2],
            triangle_count: 0,
        };
        for (vertex, &[x, y]) in mesh.vertices.iter_mut().zip(polygon.points()) {
            *vertex = Point {
                x: x as f32,
                y: y as f32,
                z: 0.0,
            };
        }
        let mut active: [usize; MAX_ROOM_VERTICES] = std::array::from_fn(|index| index);
        let mut count = polygon.count;
        while count > 3 {
            let ear = (0..count).find(|&index| {
                let previous = active[(index + count - 1) % count];
                let current = active[index];
                let next = active[(index + 1) % count];
                let [a, b, c] = [previous, current, next].map(|i| polygon.vertices[i]);
                turn(a, b, c) > 0
                    && active[..count].iter().all(|&other| {
                        other == previous
                            || other == current
                            || other == next
                            || !in_triangle(polygon.vertices[other], a, b, c)
                    })
            })?;
            mesh.triangles[mesh.triangle_count] = [
                active[(ear + count - 1) % count],
                active[ear],
                active[(ear + 1) % count],
            ];
            mesh.triangle_count += 1;
            active.copy_within(ear + 1..count, ear);
            count -= 1;
        }
        let final_triangle = [active[0], active[1], active[2]];
        let [a, b, c] = final_triangle.map(|index| polygon.vertices[index]);
        if turn(a, b, c) <= 0 {
            return None;
        }
        mesh.triangles[mesh.triangle_count] = final_triangle;
        mesh.triangle_count += 1;
        Some(mesh)
    }

    fn floor_bounds_contain(&self, point: Point) -> bool {
        valid_dimensions(self)
            && point.x.is_finite()
            && point.y.is_finite()
            && (0.0..=self.width).contains(&point.x)
            && (0.0..=self.depth).contains(&point.y)
    }
}

struct Polygon {
    vertices: [Xy; MAX_ROOM_VERTICES],
    count: usize,
}

impl Polygon {
    // Static errors keep failed per-frame queries allocation-free. Only the explicit
    // validation API converts an error to anyhow for presentation to the user.
    fn new(room: &Room) -> Result<Self, &'static str> {
        if !valid_dimensions(room) {
            return Err("Room width and depth must be finite and positive");
        }
        let count = room.wall_count();
        if !(3..=MAX_ROOM_VERTICES).contains(&count) {
            return Err("A room outline needs 3–12 corners");
        }
        if !room
            .outline
            .iter()
            .flatten()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        {
            return Err("Room outline coordinates must be finite and within 0–1");
        }
        let mut polygon = Self {
            vertices: [[0.0; 2]; MAX_ROOM_VERTICES],
            count,
        };
        for (index, vertex) in polygon.vertices[..count].iter_mut().enumerate() {
            // Match the physical f32 corners used by walls and strip routing exactly,
            // then use f64 predicates rather than accumulating f32 geometry errors.
            *vertex = xy(room.corner(index, 0.0));
        }
        if room.outline.is_empty() {
            return Ok(polygon);
        }
        for index in 0..count {
            let a = polygon.vertices[index];
            let b = polygon.vertices[(index + 1) % count];
            let c = polygon.vertices[(index + 2) % count];
            let points = [a, b].map(|[x, y]| Point {
                x: x as f32,
                y: y as f32,
                z: 0.0,
            });
            if !points[0].separated_from(points[1]) {
                return Err("Room outline edges must be at least 1 cm long");
            }
            // Straight-through collinear corners are useful when inserting a wall
            // corner, but reversing along the same edge is an overlapping boundary.
            if turn(a, b, c) == 0 && dot(subtract(a, b), subtract(c, b)) > 0.0 {
                return Err("Adjacent room outline edges cannot overlap");
            }
            for other in index + 1..count {
                if other == index + 1 || (index == 0 && other == count - 1) {
                    continue;
                }
                if segments_intersect(
                    a,
                    b,
                    polygon.vertices[other],
                    polygon.vertices[(other + 1) % count],
                ) {
                    return Err("Room outline edges cannot intersect or touch");
                }
            }
        }
        let origin = polygon.vertices[0];
        let area: f64 = (1..count - 1)
            .map(|index| {
                cross(
                    subtract(polygon.vertices[index], origin),
                    subtract(polygon.vertices[index + 1], origin),
                )
            })
            .sum();
        if area <= 0.0 {
            return Err("Room outline must enclose an area in counterclockwise order");
        }
        Ok(polygon)
    }

    fn points(&self) -> &[Xy] {
        &self.vertices[..self.count]
    }

    fn contains(&self, point: Xy) -> bool {
        self.contains_with_boundary(point, true)
    }

    fn contains_with_boundary(&self, point: Xy, rounded: bool) -> bool {
        let mut inside = false;
        for index in 0..self.count {
            let a = self.vertices[index];
            let b = self.vertices[(index + 1) % self.count];
            if on_segment(point, a, b) || (rounded && on_input_boundary(point, a, b)) {
                return true;
            }
            if (a[1] > point[1]) != (b[1] > point[1]) && (turn(a, b, point) > 0) == (b[1] > a[1]) {
                inside = !inside;
            }
        }
        inside
    }

    fn contains_segment(&self, a: Xy, b: Xy) -> bool {
        if !self.contains(a) || !self.contains(b) {
            return false;
        }
        let direction = subtract(b, a);
        let length_squared = dot(direction, direction);
        if length_squared == 0.0 {
            return true;
        }
        // Every edge contributes at most two split points (a collinear overlap).
        // Testing each open interval handles tangencies, reflex vertices, narrow
        // cutouts and a segment entering/leaving along an existing boundary.
        let mut cuts = [0.0; 2 * MAX_ROOM_VERTICES + 2];
        cuts[1] = 1.0;
        let mut count = 2;
        let mut overlaps = [[0.0; 2]; MAX_ROOM_VERTICES];
        let mut overlap_count = 0;
        for index in 0..self.count {
            let c = self.vertices[index];
            let d = self.vertices[(index + 1) % self.count];
            if !segments_intersect(a, b, c, d) {
                continue;
            }
            let edge = subtract(d, c);
            if determinant_sign(direction, edge) == 0 {
                let mut overlap = [0.0; 2];
                for (slot, point) in overlap.iter_mut().zip([c, d]) {
                    *slot = (dot(subtract(point, a), direction) / length_squared).clamp(0.0, 1.0);
                    cuts[count] = *slot;
                    count += 1;
                }
                overlap.sort_unstable_by(f64::total_cmp);
                overlaps[overlap_count] = overlap;
                overlap_count += 1;
            } else {
                cuts[count] =
                    (cross(subtract(c, a), edge) / cross(direction, edge)).clamp(0.0, 1.0);
                count += 1;
            }
        }
        cuts[..count].sort_unstable_by(f64::total_cmp);
        cuts[..count].windows(2).all(|pair| {
            if pair[0] == pair[1]
                || overlaps[..overlap_count]
                    .iter()
                    .any(|overlap| overlap[0] <= pair[0] && pair[1] <= overlap[1])
            {
                return true;
            }
            let t = pair[0] + (pair[1] - pair[0]) * 0.5;
            let point = [a[0] + direction[0] * t, a[1] + direction[1] * t];
            self.contains_with_boundary(point, false)
                || (0..self.count).any(|i| {
                    let c = self.vertices[i];
                    let d = self.vertices[(i + 1) % self.count];
                    // Permit f32-rounded wall interpolation, not an arbitrary
                    // shortcut through a thin concave tip near several walls.
                    on_input_boundary(point, c, d)
                        && on_input_line(a, c, d)
                        && on_input_line(b, c, d)
                })
        })
    }
}

fn valid_dimensions(room: &Room) -> bool {
    room.width.is_finite() && room.width > 0.0 && room.depth.is_finite() && room.depth > 0.0
}

fn xy(point: Point) -> Xy {
    [f64::from(point.x), f64::from(point.y)]
}

fn subtract(a: Xy, b: Xy) -> Xy {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: Xy, b: Xy) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: Xy, b: Xy) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn determinant_sign(a: Xy, b: Xy) -> i8 {
    let left = a[0] * b[1];
    let right = a[1] * b[0];
    let determinant = left - right;
    // Relative arithmetic uncertainty only, not a physical tolerance that could
    // bridge a narrow cutout. Input coordinates are widened before subtraction.
    let error = (left.abs() + right.abs()) * (8.0 * f64::EPSILON);
    if determinant > error {
        1
    } else if determinant < -error {
        -1
    } else {
        0
    }
}

fn turn(a: Xy, b: Xy, c: Xy) -> i8 {
    determinant_sign(subtract(b, a), subtract(c, a))
}

fn on_segment(point: Xy, a: Xy, b: Xy) -> bool {
    turn(a, b, point) == 0
        && (a[0].min(b[0])..=a[0].max(b[0])).contains(&point[0])
        && (a[1].min(b[1])..=a[1].max(b[1])).contains(&point[1])
}

// Geometry topology uses widened predicates above. User positions, however, are
// f32 results of interpolation: a rounded wall midpoint need not be exactly on
// its f64 line. Bound only that input-roundoff, not a fixed physical tolerance.
fn input_error(point: Xy, a: Xy, b: Xy) -> Xy {
    std::array::from_fn(|i| {
        2.0 * f64::from(f32::EPSILON) * point[i].abs().max(a[i].abs()).max(b[i].abs())
    })
}

fn on_input_line(point: Xy, a: Xy, b: Xy) -> bool {
    let error = input_error(point, a, b);
    let edge = subtract(b, a);
    cross(edge, subtract(point, a)).abs() <= edge[0].abs() * error[1] + edge[1].abs() * error[0]
}

fn on_input_boundary(point: Xy, a: Xy, b: Xy) -> bool {
    let error = input_error(point, a, b);
    on_input_line(point, a, b)
        && (0..2)
            .all(|i| (a[i].min(b[i]) - error[i]..=a[i].max(b[i]) + error[i]).contains(&point[i]))
}

fn segments_intersect(a: Xy, b: Xy, c: Xy, d: Xy) -> bool {
    let ab_c = turn(a, b, c);
    let ab_d = turn(a, b, d);
    let cd_a = turn(c, d, a);
    let cd_b = turn(c, d, b);
    (ab_c * ab_d < 0 && cd_a * cd_b < 0)
        || (ab_c == 0 && on_segment(c, a, b))
        || (ab_d == 0 && on_segment(d, a, b))
        || (cd_a == 0 && on_segment(a, c, d))
        || (cd_b == 0 && on_segment(b, c, d))
}

fn in_triangle(point: Xy, a: Xy, b: Xy, c: Xy) -> bool {
    turn(a, b, point) >= 0 && turn(b, c, point) >= 0 && turn(c, a, point) >= 0
}
