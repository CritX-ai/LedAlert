use eframe::egui::{Pos2, Rect, Vec2};
use ledalert::{
    config::{Config, MAX_ROOM_VERTICES, Point, Room},
    footprint::FloorMesh,
    spatial::{
        DEFAULT_PITCH, DEFAULT_YAW, Projection, closest_on_segment, project_screen_galley,
        screen_corners, wall_path,
    },
};

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

fn point_close(actual: Point, expected: Point) {
    close(actual.x, expected.x);
    close(actual.y, expected.y);
    close(actual.z, expected.z);
}

fn viewport() -> Rect {
    Rect::from_min_size(Pos2::new(90.0, 45.0), Vec2::new(900.0, 650.0))
}

fn camera_angles() -> impl Iterator<Item = (f32, f32)> {
    [
        -180.0_f32, -135.0, -90.0, -45.0, 0.0, 45.0, 90.0, 135.0, 180.0,
    ]
    .into_iter()
    .flat_map(|yaw| {
        [15.0_f32, 35.264_39, 75.0]
            .into_iter()
            .map(move |pitch| (yaw.to_radians(), pitch.to_radians()))
    })
}

#[test]
fn both_plane_inverses_round_trip_inside_and_outside_the_room() {
    let room = Config::default().room;
    let projection = Projection::new(&room, viewport(), DEFAULT_YAW, DEFAULT_PITCH).unwrap();
    for point in [
        Point {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        Point {
            x: room.width,
            y: room.depth,
            z: room.height,
        },
        Point {
            x: 1.3,
            y: 2.1,
            z: 0.9,
        },
        Point {
            x: -3.0,
            y: 7.0,
            z: -1.0,
        },
    ] {
        let pixel = projection.project(point);
        point_close(projection.floor_at(pixel, point.z).unwrap(), point);
        point_close(projection.elevation_at(pixel, point.y).unwrap(), point);
    }
    let origin = projection.project(Point {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    });
    let x_axis = projection.project(Point {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    }) - origin;
    let y_axis = projection.project(Point {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    }) - origin;
    let z_axis = projection.project(Point {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    }) - origin;
    assert!(x_axis.x * y_axis.x < 0.0);
    assert!(x_axis.y > 0.0 && y_axis.y > 0.0);
    close(z_axis.x, 0.0);
    assert!(z_axis.y < 0.0);
    close(projection.height_delta(z_axis), 1.0);
    close(projection.height_delta(z_axis + Vec2::new(200.0, 0.0)), 1.0);
}

#[test]
fn translated_frozen_drag_frame_remains_independent_of_room_refitting() {
    let mut room = Config::default().room;
    let frozen = Projection::new(&room, viewport(), DEFAULT_YAW, DEFAULT_PITCH).unwrap();
    let start = Point {
        x: 1.0,
        y: 2.0,
        z: 0.5,
    };
    let end = Point {
        x: 4.0,
        y: 2.0,
        z: 1.8,
    };
    let start_pixel = frozen.project(start);
    let end_pixel = frozen.project(end);
    let pan = Vec2::new(-73.0, 46.0);
    let translated = frozen.translated(pan);
    room.width *= 3.0;
    room.height *= 2.0;
    let refitted = Projection::new(&room, viewport(), DEFAULT_YAW, DEFAULT_PITCH).unwrap();
    assert_ne!(refitted.scale(), frozen.scale());
    assert_eq!(frozen.project(start), start_pixel);
    assert_eq!(translated.project(start), start_pixel + pan);
    point_close(translated.floor_at(end_pixel + pan, end.z).unwrap(), end);
    point_close(
        translated.elevation_at(end_pixel + pan, end.y).unwrap(),
        end,
    );
    point_close(frozen.elevation_at(end_pixel, end.y).unwrap(), end);
    close(translated.scale(), frozen.scale());
    close(
        translated.height_delta(end_pixel - start_pixel),
        frozen.height_delta(end_pixel - start_pixel),
    );
}

#[test]
fn all_eight_corners_fit_with_an_inset_in_wide_and_tall_viewports() {
    let mut room = Config::default().room;
    for (width, depth, height) in [(5.0, 4.0, 2.5), (0.5, 50.0, 0.5), (50.0, 0.5, 50.0)] {
        room.width = width;
        room.depth = depth;
        room.height = height;
        for size in [Vec2::new(1000.0, 250.0), Vec2::new(160.0, 900.0)] {
            let rect = Rect::from_min_size(Pos2::new(-130.0, 75.0), size);
            for (yaw, pitch) in camera_angles() {
                let projection = Projection::new(&room, rect, yaw, pitch).unwrap();
                let mut bounds = Rect::NOTHING;
                for x in [0.0, width] {
                    for y in [0.0, depth] {
                        for z in [0.0, height] {
                            let projected = projection.project(Point { x, y, z });
                            assert!(rect.shrink(27.99).contains(projected));
                            bounds.extend_with(projected);
                        }
                    }
                }
                close(bounds.center().x, rect.center().x);
                close(bounds.center().y, rect.center().y);
                assert!(
                    bounds.width() >= rect.width() - 56.01
                        || bounds.height() >= rect.height() - 56.01
                );
            }
        }
    }
}

#[test]
fn invalid_geometry_and_plane_inputs_are_rejected_without_clamping() {
    let room = Config::default().room;
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
        for axis in 0..3 {
            let mut bad = room.clone();
            match axis {
                0 => bad.width = invalid,
                1 => bad.depth = invalid,
                _ => bad.height = invalid,
            }
            assert!(Projection::new(&bad, viewport(), DEFAULT_YAW, DEFAULT_PITCH).is_none());
        }
    }
    for rect in [
        Rect::from_min_size(Pos2::ZERO, Vec2::new(50.0, 200.0)),
        Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 0.0)),
        Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::ZERO),
        Rect::from_min_max(Pos2::ZERO, Pos2::new(f32::INFINITY, 100.0)),
        Rect::from_min_max(Pos2::new(f32::NAN, 0.0), Pos2::new(100.0, 100.0)),
    ] {
        assert!(Projection::new(&room, rect, DEFAULT_YAW, DEFAULT_PITCH).is_none());
    }
    let mut enormous = room.clone();
    enormous.width = f32::MAX;
    enormous.depth = f32::MAX;
    assert!(Projection::new(&enormous, viewport(), DEFAULT_YAW, DEFAULT_PITCH).is_none());
    let projection = Projection::new(&room, viewport(), DEFAULT_YAW, DEFAULT_PITCH).unwrap();
    assert!(projection.floor_at(Pos2::ZERO, f32::NAN).is_none());
    assert!(projection.elevation_at(Pos2::ZERO, f32::INFINITY).is_none());
    let invalid_pointer = Pos2::new(f32::NAN, 0.0);
    assert!(projection.floor_at(invalid_pointer, 1.0).is_none());
    assert!(projection.elevation_at(invalid_pointer, 1.0).is_none());
    let point = room.screens[0].position;
    assert_eq!(
        projection
            .translated(Vec2::new(f32::NAN, 0.0))
            .project(point),
        projection.project(point)
    );
}

#[test]
fn default_camera_reproduces_the_incumbent_projection() {
    let room = Config::default().room;
    let rect = viewport();
    let projection = Projection::new(&room, rect, DEFAULT_YAW, DEFAULT_PITCH).unwrap();
    let horizontal = std::f32::consts::FRAC_1_SQRT_2;
    let receding = 0.408_248_3;
    let upright = 0.816_496_6;
    let min = Vec2::new(-horizontal * room.depth, -upright * room.height);
    let max = Vec2::new(
        horizontal * room.width,
        receding * (room.width + room.depth),
    );
    let available = rect.shrink(28.0);
    let extent = max - min;
    let scale = (available.width() / extent.x).min(available.height() / extent.y);
    let origin = available.center() - (min + extent * 0.5) * scale;
    close(projection.scale(), scale);
    for x in [0.0, 1.3, room.width] {
        for y in [0.0, 2.1, room.depth] {
            for z in [0.0, 0.9, room.height] {
                let expected = origin
                    + Vec2::new(
                        horizontal * (x - y),
                        receding * x + receding * y - upright * z,
                    ) * scale;
                assert!(projection.project(Point { x, y, z }).distance(expected) < 0.001);
            }
        }
    }
}

#[test]
fn orbit_preserves_plane_inverses_height_edits_and_translated_frames() {
    let room = Config::default().room;
    let pan = Vec2::new(-73.0, 46.0);
    for (yaw, pitch) in camera_angles() {
        let projection = Projection::new(&room, viewport(), yaw, pitch).unwrap();
        let translated = projection.translated(pan);
        for point in [
            Point {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            Point {
                x: room.width,
                y: room.depth,
                z: room.height,
            },
            Point {
                x: 1.3,
                y: 2.1,
                z: 0.9,
            },
            Point {
                x: -3.0,
                y: 7.0,
                z: -1.0,
            },
        ] {
            let pixel = projection.project(point);
            point_close(projection.floor_at(pixel, point.z).unwrap(), point);
            point_close(translated.floor_at(pixel + pan, point.z).unwrap(), point);
            if yaw.cos().abs() < 1e-6 {
                assert!(projection.elevation_at(pixel, point.y).is_none());
                assert!(translated.elevation_at(pixel + pan, point.y).is_none());
            } else {
                point_close(projection.elevation_at(pixel, point.y).unwrap(), point);
                point_close(
                    translated.elevation_at(pixel + pan, point.y).unwrap(),
                    point,
                );
            }
            let raised = Point {
                z: point.z + 1.0,
                ..point
            };
            let delta = projection.project(raised) - pixel;
            close(delta.x, 0.0);
            close(projection.height_delta(delta), 1.0);
            close(translated.height_delta(delta + Vec2::new(200.0, 0.0)), 1.0);
            close(translated.depth(point), projection.depth(point));
        }
    }
}

#[test]
fn camera_rows_are_orthonormal_and_depth_increases_toward_the_camera() {
    let room = Config::default().room;
    let origin = Point {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let axes = [
        Point {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        },
        Point {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        },
        Point {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
    ];
    for (yaw, pitch) in camera_angles() {
        let projection = Projection::new(&room, viewport(), yaw, pitch).unwrap();
        let pixel = projection.project(origin);
        let columns = axes.map(|axis| {
            let delta = (projection.project(axis) - pixel) / projection.scale();
            [delta.x, delta.y, projection.depth(axis)]
        });
        for a in 0..3 {
            for b in 0..3 {
                let product: f32 = columns.iter().map(|column| column[a] * column[b]).sum();
                close(product, if a == b { 1.0 } else { 0.0 });
            }
        }
        let toward_camera = projection.camera_facing();
        close(projection.depth(toward_camera), 1.0);
        assert!(projection.project(toward_camera).distance(pixel) < 0.001);
        let wrapped =
            Projection::new(&room, viewport(), yaw + std::f32::consts::TAU, pitch).unwrap();
        for axis in axes {
            assert!(projection.project(axis).distance(wrapped.project(axis)) < 0.001);
            close(projection.depth(axis), wrapped.depth(axis));
        }
    }
}

#[test]
fn singular_and_nonfinite_camera_angles_are_rejected() {
    let room = Config::default().room;
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(Projection::new(&room, viewport(), invalid, DEFAULT_PITCH).is_none());
        assert!(Projection::new(&room, viewport(), DEFAULT_YAW, invalid).is_none());
    }
    for pitch in [0.0, 1e-8, std::f32::consts::FRAC_PI_2, std::f32::consts::PI] {
        assert!(Projection::new(&room, viewport(), DEFAULT_YAW, pitch).is_none());
    }
}

#[test]
fn wall_routes_follow_every_start_length_and_direction_without_duplicate_segments() {
    let room = Config::default().room;
    let height = room.height * 0.6;
    let corners = [
        Point {
            x: 0.0,
            y: 0.0,
            z: height,
        },
        Point {
            x: room.width,
            y: 0.0,
            z: height,
        },
        Point {
            x: room.width,
            y: room.depth,
            z: height,
        },
        Point {
            x: 0.0,
            y: room.depth,
            z: height,
        },
    ];
    for start in 0..4 {
        for step in [1, 3] {
            for count in 1..=4 {
                let walls: Vec<usize> = (0..count)
                    .map(|offset| (start + step * offset) % 4)
                    .collect();
                let path = wall_path(&room, &walls, height).unwrap();
                assert_eq!(path.len(), count + 1);
                let forward = count == 1 || step == 1;
                for (segment, &wall) in path.windows(2).zip(&walls) {
                    let endpoints = [corners[wall], corners[(wall + 1) % 4]];
                    point_close(segment[0], endpoints[usize::from(!forward)]);
                    point_close(segment[1], endpoints[usize::from(forward)]);
                    assert!(segment[0].distance(segment[1]) > 0.0);
                }
                for a in 0..path.len() {
                    for b in a + 1..path.len() {
                        if count == 4 && a == 0 && b == count {
                            point_close(path[a], path[b]);
                        } else {
                            assert!(path[a].distance(path[b]) > 0.0);
                        }
                    }
                }
            }
        }
    }
    for height in [0.0, room.height] {
        for point in wall_path(&room, &[0, 3, 2, 1], height).unwrap() {
            close(point.z, height);
        }
    }
}

#[test]
fn wall_routes_reject_invalid_selections_heights_and_dimensions() {
    let room = Config::default().room;
    for walls in [
        &[][..],
        &[4],
        &[usize::MAX],
        &[0, 0],
        &[0, 2],
        &[0, 1, 3],
        &[0, 1, 0],
        &[0, 1, 2, 0],
        &[0, 1, 2, 3, 0],
    ] {
        assert!(wall_path(&room, walls, 1.0).is_err(), "{walls:?}");
    }
    for height in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -0.1,
        room.height + 0.1,
    ] {
        assert!(wall_path(&room, &[0, 1], height).is_err());
    }
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
        for axis in 0..3 {
            let mut bad = room.clone();
            match axis {
                0 => bad.width = invalid,
                1 => bad.depth = invalid,
                _ => bad.height = invalid,
            }
            assert!(wall_path(&bad, &[0], 0.0).is_err());
        }
    }
}

#[test]
fn monitor_yaw_preserves_upright_height_and_true_aspect() {
    let mut screen = Config::default().room.screens.remove(0);
    screen.width = 1.6;
    for aspect in [16.0 / 9.0, 9.0 / 16.0] {
        screen.aspect_ratio = aspect;
        for angle in [0.0, 35.0, 90.0, -135.0, 180.0] {
            screen.angle = angle;
            let [tl, tr, br, bl] = screen_corners(&screen);
            close(tl.distance(tr), screen.width);
            close(tr.distance(br), screen.width / aspect);
            close(tl.z, tr.z);
            close(bl.z, br.z);
            close(tl.x, bl.x);
            close(tl.y, bl.y);
            close(tr.x, br.x);
            close(tr.y, br.y);
            assert!(tl.z > bl.z);
            point_close(tl.lerp(br, 0.5), screen.position);
            point_close(tr.lerp(bl, 0.5), screen.position);
            close(tr.x - tl.x, screen.width * angle.to_radians().cos());
            close(tr.y - tl.y, screen.width * angle.to_radians().sin());
        }
    }
}

#[test]
fn display_number_uses_the_font_face_and_mirrors_horizontally_from_the_rear() {
    let ctx = eframe::egui::Context::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let source = ui.painter().layout_no_wrap(
            "12".into(),
            eframe::egui::FontId::proportional(32.0),
            eframe::egui::Color32::WHITE,
        );
        let room = Config::default().room;
        let mut screen = room.screens[0].clone();
        for angle in [-135.0_f32, 0.0, 37.0, 90.0] {
            screen.angle = angle;
            for pitch in [15.0_f32, 35.0, 75.0] {
                for aspect in [16.0 / 9.0, 9.0 / 16.0] {
                    screen.aspect_ratio = aspect;
                    let front =
                        Projection::new(&room, viewport(), -angle.to_radians(), pitch.to_radians())
                            .unwrap();
                    let rear = Projection::new(
                        &room,
                        viewport(),
                        std::f32::consts::PI - angle.to_radians(),
                        pitch.to_radians(),
                    )
                    .unwrap();
                    let mut front_number = (*source).clone();
                    let mut rear_number = (*source).clone();
                    project_screen_galley(front, &screen, &mut front_number);
                    project_screen_galley(rear, &screen, &mut rear_number);
                    let height = screen.width / aspect;
                    let scale = (screen.width * 0.65 / source.mesh_bounds.width())
                        .min(height * 0.58 / source.mesh_bounds.height());
                    for ((original, front_row), rear_row) in source
                        .rows
                        .iter()
                        .zip(&front_number.rows)
                        .zip(&rear_number.rows)
                    {
                        for ((glyph, front_vertex), rear_vertex) in original
                            .visuals
                            .mesh
                            .vertices
                            .iter()
                            .zip(&front_row.visuals.mesh.vertices)
                            .zip(&rear_row.visuals.mesh.vertices)
                        {
                            let local = (glyph.pos + original.pos.to_vec2()
                                - source.mesh_bounds.center())
                                * scale;
                            let front_local =
                                (front_vertex.pos - front.project(screen.position)) / front.scale();
                            let rear_local =
                                (rear_vertex.pos - rear.project(screen.position)) / rear.scale();
                            close(front_local.x, local.x);
                            close(rear_local.x, -local.x);
                            close(front_local.y, local.y * pitch.to_radians().cos());
                            close(rear_local.y, front_local.y);
                            // Reflection changes geometry, never the sampled font glyph.
                            assert_eq!(front_vertex.uv, glyph.uv);
                            assert_eq!(rear_vertex.uv, glyph.uv);
                        }
                    }
                }
            }
        }
    });
    // This geometry-only frame has no renderer to consume the font atlas upload.
    output.textures_delta.clear();
}

#[test]
fn unequal_room_dimensions_remain_on_their_own_axes_after_refitting() {
    let mut room = Config::default().room;
    for (width, depth, height) in [(3.0, 7.0, 2.0), (3.0, 11.0, 2.0), (3.0, 7.0, 5.0)] {
        room.width = width;
        room.depth = depth;
        room.height = height;
        for (yaw, pitch) in camera_angles() {
            let projection = Projection::new(&room, viewport(), yaw, pitch).unwrap();
            let origin = projection.project(Point {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            });
            let x = (projection.project(Point {
                x: width,
                y: 0.0,
                z: 0.0,
            }) - origin)
                / projection.scale();
            let y = (projection.project(Point {
                x: 0.0,
                y: depth,
                z: 0.0,
            }) - origin)
                / projection.scale();
            let z = (projection.project(Point {
                x: 0.0,
                y: 0.0,
                z: height,
            }) - origin)
                / projection.scale();
            close(x.x, width * yaw.cos());
            close(x.y, width * pitch.sin() * yaw.sin());
            close(y.x, -depth * yaw.sin());
            close(y.y, depth * pitch.sin() * yaw.cos());
            close(z.x, 0.0);
            close(z.y, -height * pitch.cos());
        }
    }
}

#[test]
fn segment_selection_clamps_to_endpoints_and_handles_degenerate_segments() {
    let a = Pos2::new(1.0, 2.0);
    let b = Pos2::new(5.0, 2.0);
    for (pointer, expected_t, expected_distance) in [
        (Pos2::new(3.0, 5.0), 0.5, 3.0),
        (Pos2::new(-2.0, 6.0), 0.0, 5.0),
        (Pos2::new(8.0, 6.0), 1.0, 5.0),
    ] {
        let (t, distance) = closest_on_segment(pointer, a, b);
        close(t, expected_t);
        close(distance, expected_distance);
    }
    let (t, distance) = closest_on_segment(Pos2::new(4.0, 6.0), a, a);
    close(t, 0.0);
    close(distance, 5.0);
    let (t, distance) = closest_on_segment(Pos2::ZERO, Pos2::new(-1e30, 0.0), Pos2::new(1e30, 0.0));
    close(t, 0.5);
    close(distance, 0.0);
}

const CONCAVE_OUTLINE: [[f32; 2]; 6] = [
    [0.0, 0.0],
    [1.0, 0.0],
    [1.0, 0.4],
    [0.4, 0.4],
    [0.4, 1.0],
    [0.0, 1.0],
];
const DOUBLE_NOTCH_OUTLINE: [[f32; 2]; 12] = [
    [0.0, 0.0],
    [1.0, 0.0],
    [1.0, 1.0],
    [0.8, 1.0],
    [0.8, 0.4],
    [0.6, 0.4],
    [0.6, 1.0],
    [0.4, 1.0],
    [0.4, 0.4],
    [0.2, 0.4],
    [0.2, 1.0],
    [0.0, 1.0],
];

fn outlined_room(outline: &[[f32; 2]]) -> Room {
    let mut room = Config::default().room;
    room.width = 10.0;
    room.depth = 10.0;
    room.outline = outline.to_vec();
    room
}

fn floor_point(x: f32, y: f32) -> Point {
    Point { x, y, z: 0.0 }
}

#[test]
fn centimetre_outline_walls_remain_valid_when_routed_as_a_strip() {
    let mut config = Config {
        room: outlined_room(&[
            [0.0, 0.0],
            [0.5, 0.0],
            [0.501, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ]),
        ..Config::default()
    };
    config.room.validate_outline().unwrap();
    let walls: Vec<_> = (0..config.room.wall_count()).collect();
    config.room.strip = wall_path(&config.room, &walls, 1.0).unwrap();
    config.validate().unwrap();
}

#[test]
fn concave_floor_includes_boundaries_but_checks_every_interval_of_a_segment() {
    let room = outlined_room(&CONCAVE_OUTLINE);
    for (point, inside) in [
        (floor_point(2.0, 8.0), true),
        (floor_point(8.0, 2.0), true),
        (floor_point(4.0, 4.0), true),
        (floor_point(4.0, 10.0), true),
        (floor_point(8.0, 8.0), false),
        (floor_point(-0.001, 2.0), false),
        (floor_point(f32::NAN, 2.0), false),
    ] {
        assert_eq!(room.contains_floor(point), inside, "{point:?}");
    }
    for (a, b, contained) in [
        ((8.0, 2.0), (2.0, 8.0), false),
        ((10.0, 4.0), (4.0, 10.0), false),
        ((8.0, 0.0), (0.0, 8.0), true), // tangent at the reflex corner
        ((10.0, 4.0), (0.0, 4.0), true), // boundary followed by interior
        ((4.0, 4.0), (4.0, 10.0), true),
        ((4.0, 4.0), (4.0, 4.0), true),
        ((8.0, 8.0), (8.0, 8.0), false),
    ] {
        let a = floor_point(a.0, a.1);
        let b = floor_point(b.0, b.1);
        assert_eq!(room.contains_floor_segment(a, b), contained, "{a:?}–{b:?}");
        assert_eq!(room.contains_floor_segment(b, a), contained, "{b:?}–{a:?}");
    }
    let notches = outlined_room(&DOUBLE_NOTCH_OUTLINE);
    let a = floor_point(1.0, 8.0);
    let b = floor_point(9.0, 8.0);
    assert!(notches.contains_floor(a));
    assert!(notches.contains_floor(b));
    assert!(notches.contains_floor(a.lerp(b, 0.5)));
    assert!(!notches.contains_floor_segment(a, b));
    let narrow_notch = outlined_room(&[
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.501, 1.0],
        [0.501, 0.3],
        [0.5, 0.3],
        [0.5, 1.0],
        [0.0, 1.0],
    ]);
    narrow_notch.validate_outline().unwrap();
    assert!(!narrow_notch.contains_floor(floor_point(5.005, 8.0)));
    assert!(!narrow_notch.contains_floor_segment(a, b));
}

#[test]
fn sloping_and_collinear_boundary_paths_are_contained_in_both_directions() {
    let room = outlined_room(&[[0.0, 0.0], [1.0, 0.0], [0.8, 0.2], [0.4, 0.6], [0.0, 1.0]]);
    room.validate_outline().unwrap();
    let a = floor_point(9.0, 1.0);
    let b = floor_point(1.0, 9.0);
    assert!(room.contains_floor_segment(a, b));
    assert!(room.contains_floor_segment(b, a));
    let sloped = outlined_room(&[[0.07, 0.13], [0.91, 0.2], [0.8, 0.8], [0.3, 0.61]]);
    sloped.validate_outline().unwrap();
    for index in 0..sloped.wall_count() {
        let a = sloped.corner(index, 0.0);
        let b = sloped.corner(index + 1, 0.0);
        assert!(sloped.contains_floor_segment(a, b));
        assert!(sloped.contains_floor_segment(b, a));
    }
}

#[test]
fn inserting_a_rounded_bend_on_a_sloping_wall_preserves_a_valid_setup() {
    let mut config = Config::default();
    config.room.width = 1.0;
    config.room.depth = 1.0;
    config.room.outline = vec![[1.0, 1.0], [0.3, 0.0], [1.0, 0.0]];
    config.room.screens[0].position = Point {
        x: 0.9,
        y: 0.5,
        z: 1.0,
    };
    config.room.strip = wall_path(&config.room, &[0, 1, 2], 1.0).unwrap();
    config.validate().unwrap();
    let bend = config.room.strip[0].lerp(config.room.strip[1], 0.5);
    config.room.strip.insert(1, bend);
    config.validate().unwrap();
    assert!(config.room.floor_mesh().is_some());
    assert!(!config.room.contains_floor(Point {
        x: bend.x - 0.001,
        ..bend
    }));
}

fn triangle_area_twice(a: Point, b: Point, c: Point) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn mesh_contains(mesh: &FloorMesh, point: Point) -> bool {
    mesh.triangles[..mesh.triangle_count]
        .iter()
        .any(|triangle| {
            let [a, b, c] = triangle.map(|index| mesh.vertices[index]);
            triangle_area_twice(a, b, point) >= 0.0
                && triangle_area_twice(b, c, point) >= 0.0
                && triangle_area_twice(c, a, point) >= 0.0
        })
}

#[test]
fn floor_triangles_cover_the_room_without_filling_concave_cutouts() {
    let cases: &[(&[[f32; 2]], f32)] = &[
        (&[], 100.0),
        (&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]], 50.0),
        (
            &[[0.0, 0.0], [0.5, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            100.0,
        ),
        (&CONCAVE_OUTLINE, 64.0),
        (&DOUBLE_NOTCH_OUTLINE, 76.0),
    ];
    for (case, &(outline, expected_area)) in cases.iter().enumerate() {
        let room = outlined_room(outline);
        let mesh = room.floor_mesh().unwrap();
        assert_eq!(mesh.vertex_count, room.wall_count());
        assert!(mesh.triangle_count <= mesh.vertex_count - 2);
        let mut area = 0.0;
        for triangle in &mesh.triangles[..mesh.triangle_count] {
            assert!(triangle.iter().all(|&index| index < mesh.vertex_count));
            let [a, b, c] = triangle.map(|index| mesh.vertices[index]);
            let twice_area = triangle_area_twice(a, b, c);
            assert!(twice_area > 0.0);
            area += twice_area * 0.5;
            for (start, end) in [(a, b), (b, c), (c, a)] {
                assert!(room.contains_floor_segment(start, end));
            }
        }
        close(area, expected_area);
        for x in 0..20 {
            for y in 0..20 {
                let point = floor_point(x as f32 * 0.5 + 0.25, y as f32 * 0.5 + 0.25);
                let expected = match case {
                    1 => point.x + point.y <= 10.0,
                    3 => point.x <= 4.0 || point.y <= 4.0,
                    4 => {
                        point.y <= 4.0
                            || point.x <= 2.0
                            || (4.0..=6.0).contains(&point.x)
                            || point.x >= 8.0
                    }
                    _ => true,
                };
                assert_eq!(mesh_contains(&mesh, point), expected, "{case}: {point:?}");
                assert_eq!(room.contains_floor(point), expected, "{case}: {point:?}");
            }
        }
    }
}

#[test]
fn malformed_outlines_fail_closed_in_validation_queries_meshes_and_routes() {
    let cases: &[&[[f32; 2]]] = &[
        &[[0.0, 0.0], [1.0, 1.0]],
        &[[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]], // clockwise
        &[[0.0, 0.0], [0.5, 0.0], [1.0, 0.0]],             // zero area
        &[[0.0, 0.0], [1.0, 0.0], [1.0, 0.0], [0.0, 1.0]], // duplicate
        &[[0.0, 0.0], [1.0, 1.0], [0.0, 1.0], [1.0, 0.0]], // crossing
        &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.5, 0.0], [0.0, 1.0]], // self-touch
        &[[0.0, 0.0], [1.0, 0.0], [0.5, 0.0], [1.0, 1.0], [0.0, 1.0]], // retrace
        &[[0.0, 0.0], [0.0005, 0.0], [1.0, 0.0], [0.0, 1.0]], // physical 5 mm edge
        &[[-0.1, 0.0], [1.0, 0.0], [0.0, 1.0]],
        &[[0.0, 0.0], [1.1, 0.0], [0.0, 1.0]],
        &[[0.0, 0.0], [f32::NAN, 0.0], [0.0, 1.0]],
        &[[0.0, 0.0], [1.0, 0.0], [0.0, f32::INFINITY]],
    ];
    for &outline in cases {
        let room = outlined_room(outline);
        assert!(room.validate_outline().is_err(), "{outline:?}");
        assert!(room.floor_mesh().is_none(), "{outline:?}");
        assert!(!room.contains_floor(floor_point(1.0, 1.0)), "{outline:?}");
        assert!(!room.contains_floor_segment(floor_point(1.0, 1.0), floor_point(2.0, 2.0)));
        assert!(wall_path(&room, &[0], 1.0).is_err());
    }
    let mut room = outlined_room(&[]);
    room.outline = (0..=MAX_ROOM_VERTICES)
        .map(|index| {
            let angle = index as f32 * std::f32::consts::TAU / (MAX_ROOM_VERTICES + 1) as f32;
            [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()]
        })
        .collect();
    assert!(room.validate_outline().is_err());
    assert!(room.floor_mesh().is_none());
    assert!(wall_path(&room, &[0], 1.0).is_err());
}

#[test]
fn normalized_corners_scale_per_axis_and_reject_physically_collapsed_edges() {
    let mut room = outlined_room(&[[0.0, 0.0], [0.01, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    room.width = 0.5;
    assert!(room.validate_outline().is_err()); // normalized 0.01 is only 5 mm here
    room.width = 5.0;
    room.depth = 7.0;
    room.validate_outline().unwrap();
    point_close(
        room.corner(1, 1.5),
        Point {
            x: 0.05,
            y: 0.0,
            z: 1.5,
        },
    );
    point_close(
        room.corner(3, 1.5),
        Point {
            x: 5.0,
            y: 7.0,
            z: 1.5,
        },
    );
    assert!(room.contains_floor_segment(room.corner(0, 0.0), room.corner(2, 0.0)));
}

#[test]
fn polygon_wall_routes_preserve_order_and_direction_through_the_vertex_limit() {
    for outline in [
        &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]][..],
        &CONCAVE_OUTLINE,
        &DOUBLE_NOTCH_OUTLINE,
    ] {
        let room = outlined_room(outline);
        let wall_count = room.wall_count();
        for start in 0..wall_count {
            for step in [1, wall_count - 1] {
                for count in 1..=wall_count {
                    let walls: Vec<usize> = (0..count)
                        .map(|offset| (start + step * offset) % wall_count)
                        .collect();
                    let path = wall_path(&room, &walls, 1.25).unwrap();
                    assert_eq!(path.len(), count + 1);
                    let forward = count == 1 || step == 1;
                    for (segment, &wall) in path.windows(2).zip(&walls) {
                        let a = room.corner(wall + usize::from(!forward), 1.25);
                        let b = room.corner(wall + usize::from(forward), 1.25);
                        point_close(segment[0], a);
                        point_close(segment[1], b);
                        assert!(room.contains_floor_segment(a, b));
                    }
                    if count == wall_count {
                        point_close(path[0], path[count]);
                    }
                }
            }
        }
        for walls in [&[wall_count][..], &[0, 0], &[0, wall_count - 1, 0]] {
            assert!(wall_path(&room, walls, 1.25).is_err());
        }
    }
}
