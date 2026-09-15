use ledalert::{
    config::{Config, MAX_SCREENS, Point},
    displays::{DetectedDisplay, arrange_displays, import_displays, parse_kscreen},
    spatial::screen_corners,
};
use serde_json::{Value, json};

fn desktop() -> Value {
    json!({
        "features": 4,
        "outputs": [
            {
                "name": "HDMI-A-1", "connected": true, "enabled": true,
                "pos": {"x": -3072, "y": 335},
                "size": {"width": 3840, "height": 2160},
                "scale": 1.25, "rotation": 1, "priority": 1,
                "currentModeId": "1", "replicationSource": 0,
                "modes": [{"id": "1", "size": {"width": 3840, "height": 2160}}]
            },
            {
                "name": "DP-3", "connected": true, "enabled": true,
                "pos": {"x": 0, "y": 0},
                "size": {"width": 2560, "height": 2880},
                "scale": 1.25, "rotation": 2, "priority": 2,
                "currentModeId": "7", "replicationSource": 0,
                "modes": [{"id": "7", "size": {"width": 2880, "height": 2560}}]
            }
        ],
        "screen": {"currentSize": {"width": 5120, "height": 2304}}
    })
}

fn parse(value: &Value) -> anyhow::Result<Vec<DetectedDisplay>> {
    parse_kscreen(&serde_json::to_vec(value).unwrap())
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

#[test]
fn logical_geometry_keeps_negative_origins_and_does_not_rotate_portrait_twice() {
    let displays = parse(&desktop()).unwrap();
    assert_eq!(
        displays,
        vec![
            DetectedDisplay {
                connector: "HDMI-A-1".into(),
                name: "HDMI-A-1".into(),
                x: -3072.0,
                y: 335.0,
                width: 3072.0,
                height: 1728.0,
                primary: true,
            },
            DetectedDisplay {
                connector: "DP-3".into(),
                name: "DP-3".into(),
                x: 0.0,
                y: 0.0,
                width: 2048.0,
                height: 2304.0,
                primary: false,
            },
        ]
    );
    let mut current = desktop();
    current.as_object_mut().unwrap().remove("features");
    assert_eq!(parse(&current).unwrap(), displays);
    current["features"] = json!(0);
    let unscaled = parse(&current).unwrap();
    assert_eq!((unscaled[1].width, unscaled[1].height), (2560.0, 2880.0));
}

#[test]
fn disconnected_and_disabled_outputs_do_not_need_usable_geometry() {
    let mut value = desktop();
    value["outputs"][0] = json!({"connected": false});
    value["outputs"][1] = json!({"connected": true, "enabled": false, "size": null});
    assert_eq!(parse(&value).unwrap(), Vec::new());
}

#[test]
fn malformed_active_geometry_and_unsafe_connector_labels_fail_the_snapshot() {
    for (pointer, value) in [
        ("/outputs/1/size/width", json!(0)),
        ("/outputs/1/size/height", json!(1_000_000)),
        ("/outputs/1/pos/x", json!("NaN")),
        ("/outputs/1/pos/y", json!(-1_000_000)),
        ("/outputs/1/scale", json!(0)),
        ("/outputs/1/rotation", json!(90)),
        ("/outputs/1/enabled", Value::Null),
        ("/outputs/1/name", json!("HDMI-A-1")),
        ("/outputs/1/name", json!("DP-3\nspoof")),
        ("/outputs/1/name", json!("DP-\u{202e}3")),
        ("/outputs/1/name", json!("D".repeat(129))),
    ] {
        let mut malformed = desktop();
        *malformed.pointer_mut(pointer).unwrap() = value;
        assert!(parse(&malformed).is_err(), "accepted {pointer}");
    }
    let mut missing = desktop();
    missing["outputs"][1]
        .as_object_mut()
        .unwrap()
        .remove("size");
    assert!(parse(&missing).is_err());
}

#[test]
fn input_size_and_active_and_total_output_counts_are_bounded() {
    assert!(parse_kscreen(&vec![b' '; 1024 * 1024 + 1]).is_err());
    assert!(parse_kscreen(br#"{"outputs":["#).is_err());
    assert!(parse_kscreen(br#"{"outputs":{}}"#).is_err());
    let mut value = desktop();
    let template = value["outputs"][0].clone();
    value["outputs"] = Value::Array(
        (0..=MAX_SCREENS)
            .map(|index| {
                let mut output = template.clone();
                output["name"] = json!(format!("DP-{index}"));
                output
            })
            .collect(),
    );
    assert!(parse(&value).is_err());
    value["outputs"].as_array_mut().unwrap().pop();
    let at_limit = parse(&value).unwrap();
    let mut config = Config::default();
    assert_eq!(
        import_displays(&mut config, &at_limit, true).unwrap(),
        MAX_SCREENS
    );
    assert_eq!(config.room.screens.len(), MAX_SCREENS);
    config.validate().unwrap();
    value["outputs"] = json!(vec![json!({"connected": false}); 129]);
    assert!(parse(&value).is_err());
}

#[test]
fn pristine_example_is_replaced_by_a_centered_proportional_group_with_valid_rules() {
    let displays = parse(&desktop()).unwrap();
    let mut config = Config::default();
    let original_id = config.room.screens[0].id;
    let rules = config.rules.clone();
    assert_eq!(import_displays(&mut config, &displays, true).unwrap(), 2);
    config.validate().unwrap();
    assert_eq!(config.rules, rules);
    assert_eq!(config.room.screens.len(), 2);
    assert_eq!(config.room.screens[0].id, original_id);
    assert_eq!(
        config.room.screens[0].connector.as_deref(),
        Some("HDMI-A-1")
    );
    let a = &config.room.screens[0];
    let b = &config.room.screens[1];
    let scale = a.width / displays[0].width;
    close(b.width, displays[1].width * scale);
    let desktop_dx =
        displays[1].x + displays[1].width / 2.0 - displays[0].x - displays[0].width / 2.0;
    let desktop_dy =
        displays[1].y + displays[1].height / 2.0 - displays[0].y - displays[0].height / 2.0;
    close(b.position.x - a.position.x, desktop_dx * scale);
    close(b.position.z - a.position.z, -desktop_dy * scale);
    close(a.position.y, config.room.depth * 0.35);
    close(b.position.y, a.position.y);
    let left = a.position.x - a.width / 2.0;
    let right = b.position.x + b.width / 2.0;
    let top = b.position.z + b.width / b.aspect_ratio / 2.0;
    let bottom = b.position.z - b.width / b.aspect_ratio / 2.0;
    close((left + right) / 2.0, config.room.width / 2.0);
    close((top + bottom) / 2.0, config.room.height / 2.0);
    assert!(left > 0.0 && right < config.room.width);
    assert!(bottom > 0.0 && top < config.room.height);
    assert!(right - left <= config.room.width * 0.65 + 0.0001);
    assert!(top - bottom <= config.room.height * 0.65 + 0.0001);
    assert!(b.aspect_ratio < 1.0);
}

#[test]
fn repeat_import_refreshes_aspect_without_moving_or_renaming_saved_screens() {
    let mut displays = parse(&desktop()).unwrap();
    let mut config = Config::default();
    import_displays(&mut config, &displays, true).unwrap();
    config.room.screens[0].name = "My edited workspace".into();
    config.room.screens[0].position = Point {
        x: 1.1,
        y: 2.7,
        z: 0.9,
    };
    config.room.screens[0].width = 0.8;
    config.room.screens[0].angle = 35.0;
    config.rules[0].screen_id = config.room.screens[1].id;
    let before = config.clone();
    displays[0].name = "New discovery label".into();
    displays[0].x = 9000.0;
    displays[0].height = displays[0].width;
    displays.reverse();
    assert_eq!(import_displays(&mut config, &displays, true).unwrap(), 0);
    let mut expected = before;
    expected.room.screens[0].aspect_ratio = 1.0;
    assert_eq!(config, expected);
    config.validate().unwrap();
    // Disconnecting a display must not remove its room placement or routing.
    displays.retain(|display| display.connector == "HDMI-A-1");
    assert_eq!(import_displays(&mut config, &displays, true).unwrap(), 0);
    assert_eq!(config, expected);
}

#[test]
fn replacement_requires_an_untouched_example_and_explicit_permission() {
    let displays = parse(&desktop()).unwrap();
    let mut edited = Config::default();
    edited.room.screens[0].position.x = 1.0;
    let screen = edited.room.screens[0].clone();
    import_displays(&mut edited, &displays, true).unwrap();
    assert_eq!(edited.room.screens.len(), 3);
    assert_eq!(edited.room.screens[0], screen);
    edited.validate().unwrap();

    let mut retained = Config::default();
    let original = retained.room.screens[0].clone();
    import_displays(&mut retained, &displays, false).unwrap();
    assert_eq!(retained.room.screens[0], original);
    assert_eq!(retained.room.screens.len(), 3);
    retained.validate().unwrap();
}

#[test]
fn imports_use_free_ids_even_when_the_largest_existing_id_cannot_increment() {
    let displays = parse(&desktop()).unwrap();
    let mut config = Config::default();
    config.room.screens[0].id = u32::MAX;
    config.rules[0].screen_id = u32::MAX;
    assert_eq!(import_displays(&mut config, &displays, true).unwrap(), 2);
    config.validate().unwrap();
    assert_eq!(config.room.screens[0].id, u32::MAX);
    assert_eq!(config.rules[0].screen_id, u32::MAX);
}

#[test]
fn capacity_failure_is_atomic_and_refresh_at_capacity_remains_possible() {
    let mut config = Config::default();
    let template = config.room.screens[0].clone();
    config.room.screens = (1..=MAX_SCREENS as u32)
        .map(|id| {
            let mut screen = template.clone();
            screen.id = id;
            screen.connector = Some(format!("DP-{id}"));
            screen
        })
        .collect();
    let mut displays = parse(&desktop()).unwrap();
    displays[0].connector = "DP-1".into();
    displays[0].height = displays[0].width;
    displays[1].connector = "new-connector".into();
    let before = config.clone();
    assert!(import_displays(&mut config, &displays, true).is_err());
    assert_eq!(config, before);
    assert!(arrange_displays(&mut config, &displays).is_err());
    assert_eq!(config, before);
    assert_eq!(
        import_displays(&mut config, &displays[..1], true).unwrap(),
        0
    );
    close(config.room.screens[0].aspect_ratio, 1.0);
    config.validate().unwrap();
}

#[test]
fn invalid_detected_inputs_and_unrepresentable_layouts_do_not_partially_import() {
    let good = parse(&desktop()).unwrap();
    let mut invalid = good.clone();
    invalid[1].x = f32::NAN;
    let mut duplicate = good.clone();
    duplicate[1].connector = duplicate[0].connector.clone();
    let mut impossible = good.clone();
    impossible[1].width = 1.0;
    impossible[1].height = 1.0;
    let mut oversized = Vec::new();
    for index in 0..=MAX_SCREENS {
        let mut display = good[0].clone();
        display.connector = format!("DP-{index}");
        oversized.push(display);
    }
    for displays in [invalid, duplicate, impossible, oversized] {
        let mut config = Config::default();
        let before = config.clone();
        assert!(import_displays(&mut config, &displays, true).is_err());
        assert_eq!(config, before);
        assert!(arrange_displays(&mut config, &displays).is_err());
        assert_eq!(config, before);
    }
    let mut config = Config::default();
    let before = config.clone();
    assert_eq!(import_displays(&mut config, &[], true).unwrap(), 0);
    assert_eq!(config, before);
}

#[test]
fn vertical_desktop_groups_fit_room_height_not_depth() {
    let mut displays = parse(&desktop()).unwrap();
    displays[0].x = -200.0;
    displays[0].y = -3000.0;
    displays[1].x = -200.0;
    displays[1].y = displays[0].y + displays[0].height;
    let mut config = Config::default();
    import_displays(&mut config, &displays, true).unwrap();
    let upper = &config.room.screens[0];
    let lower = &config.room.screens[1];
    assert!(upper.position.z > lower.position.z);
    let upper_corners = screen_corners(upper);
    let lower_corners = screen_corners(lower);
    close(upper_corners[3].z, lower_corners[0].z);
    close(upper_corners[0].x, lower_corners[0].x);
    close(
        upper_corners[0].z - lower_corners[3].z,
        config.room.height * 0.65,
    );
    for screen in &config.room.screens {
        close(screen.position.y, config.room.depth * 0.35);
        for corner in screen_corners(screen) {
            assert!((0.0..=config.room.width).contains(&corner.x));
            assert!((0.0..=config.room.height).contains(&corner.z));
        }
    }
}

#[test]
fn explicit_arrangement_moves_detected_screens_and_preserves_saved_metadata() {
    let displays = parse(&desktop()).unwrap();
    let mut config = Config::default();
    // Keep a manual screen, import only one connector, and retain another offline one.
    import_displays(&mut config, &displays[..1], false).unwrap();
    let manual = config.room.screens[0].clone();
    let mut offline = manual.clone();
    offline.id = 99;
    offline.connector = Some("offline-DP".into());
    config.room.screens.push(offline.clone());
    let saved = &mut config.room.screens[1];
    saved.name = "My working display".into();
    saved.position = Point {
        x: 0.5,
        y: 3.0,
        z: 0.5,
    };
    saved.width = 0.2;
    saved.angle = 45.0;
    let saved_id = saved.id;
    config.rules[0].screen_id = saved_id;
    let rules = config.rules.clone();

    assert_eq!(arrange_displays(&mut config, &displays).unwrap(), 2);
    config.validate().unwrap();
    assert_eq!(config.room.screens[0], manual);
    assert_eq!(config.room.screens[2], offline);
    assert_eq!(config.rules, rules);
    let saved = &config.room.screens[1];
    assert_eq!(saved.id, saved_id);
    assert_eq!(saved.name, "My working display");
    assert_eq!(
        saved.connector.as_deref(),
        Some(displays[0].connector.as_str())
    );
    assert_eq!(saved.angle, 0.0);
    assert_ne!(saved.position.y, 3.0);
    let mut fresh = Config::default();
    import_displays(&mut fresh, &displays, true).unwrap();
    for expected in &fresh.room.screens {
        let actual = config
            .room
            .screens
            .iter()
            .find(|screen| screen.connector == expected.connector)
            .unwrap();
        assert_eq!(actual.position, expected.position);
        close(actual.width, expected.width);
        close(actual.aspect_ratio, expected.aspect_ratio);
    }
    let arranged = config.clone();
    assert_eq!(arrange_displays(&mut config, &displays).unwrap(), 2);
    assert_eq!(config, arranged);
    assert_eq!(arrange_displays(&mut config, &[]).unwrap(), 0);
    assert_eq!(config, arranged);
}

#[test]
fn failed_rearrangement_rolls_back_existing_screen_changes_and_missing_imports() {
    let mut displays = parse(&desktop()).unwrap();
    let mut config = Config::default();
    import_displays(&mut config, &displays[..1], true).unwrap();
    config.room.screens[0].angle = -30.0;
    config.room.screens[0].position.y = 2.0;
    let before = config.clone();
    // First connector can be rearranged, but the second would fall below the
    // supported physical width under the shared proportional scale.
    displays[1].width = 1.0;
    displays[1].height = 1.0;
    assert!(arrange_displays(&mut config, &displays).is_err());
    assert_eq!(config, before);
}

fn shaped_example(width: f32, depth: f32, height: f32) -> Config {
    let mut config = Config::default();
    let ratios = (
        width / config.room.width,
        depth / config.room.depth,
        height / config.room.height,
    );
    let scale_point = |point: &mut Point| {
        point.x *= ratios.0;
        point.y *= ratios.1;
        point.z *= ratios.2;
    };
    for point in &mut config.room.strip {
        scale_point(point);
    }
    scale_point(&mut config.room.screens[0].position);
    config.room.screens[0].width = (config.room.screens[0].width * ratios.0).clamp(0.1, 5.0);
    config.rules[0].spread = (config.rules[0].spread * ratios.0).clamp(0.1, 20.0);
    config.room.width = width;
    config.room.depth = depth;
    config.room.height = height;
    config
}

#[test]
fn first_use_room_shaping_still_allows_replacing_the_untouched_example() {
    let displays = parse(&desktop()).unwrap();
    for (width, depth, height) in [(8.0, 6.0, 3.0), (0.5, 1.0, 1.0)] {
        let mut config = shaped_example(width, depth, height);
        let rules = config.rules.clone();
        let example_id = config.room.screens[0].id;
        assert_eq!(import_displays(&mut config, &displays, true).unwrap(), 2);
        assert_eq!(config.room.screens.len(), 2);
        assert_eq!(config.room.screens[0].id, example_id);
        assert_eq!(config.rules, rules);
        config.validate().unwrap();
    }
}

#[test]
fn shaped_example_replacement_requires_permission_and_untouched_metadata() {
    let displays = parse(&desktop()).unwrap();
    let original = shaped_example(8.0, 6.0, 3.0);
    let mut no_permission = original.clone();
    import_displays(&mut no_permission, &displays, false).unwrap();
    assert_eq!(no_permission.room.screens[0], original.room.screens[0]);
    for edit in 0..7 {
        let mut config = original.clone();
        match edit {
            0 => config.room.screens[0].name = "My named screen".into(),
            1 => config.room.screens[0].angle = 20.0,
            2 => config.room.screens[0].connector = Some("saved-connector".into()),
            3 => config.room.screens[0].aspect_ratio = 1.0,
            4 => {
                config.room.screens[0].id = 20;
                config.rules[0].screen_id = 20;
            }
            5 => config.rules[0].enabled = false,
            _ => config.room.screens[0].position.x += 0.1,
        }
        let screen = config.room.screens[0].clone();
        let rules = config.rules.clone();
        import_displays(&mut config, &displays, true).unwrap();
        assert_eq!(config.room.screens.len(), 3);
        assert_eq!(config.room.screens[0], screen);
        assert_eq!(config.rules, rules);
    }
}
