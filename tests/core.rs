use ledalert::{
    config::{Config, Point, Rule, Screen},
    engine::{Engine, sample_strip},
};
use std::time::{Duration, Instant};

#[test]
fn physical_spacing_follows_bends_height_and_reversal() {
    let a = Point {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let b = Point {
        x: 2.0,
        y: 0.0,
        z: 0.0,
    };
    let c = Point {
        x: 2.0,
        y: 0.0,
        z: 2.0,
    };
    let forward = sample_strip(&[a, b, c], 5, false);
    assert_eq!(
        forward,
        vec![
            a,
            Point {
                x: 1.0,
                y: 0.0,
                z: 0.0
            },
            b,
            Point {
                x: 2.0,
                y: 0.0,
                z: 1.0
            },
            c
        ]
    );
    let mut backward = forward.clone();
    backward.reverse();
    assert_eq!(sample_strip(&[a, b, c], 5, true), backward);
    assert_eq!(sample_strip(&[a, b], 1, false), vec![a]);
}

#[test]
fn invalid_save_preserves_previous_config_and_roundtrip_is_exact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let mut config = Config::default();
    config.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap(), config);
    let original = std::fs::read(&path).unwrap();
    config.rules[0].screen_id = 999;
    assert!(config.save(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let mut future = serde_json::to_value(Config::default()).unwrap();
    future["version"] = serde_json::json!(3);
    std::fs::write(&path, serde_json::to_vec(&future).unwrap()).unwrap();
    assert!(Config::load(&path).is_err());
}

#[test]
fn invalid_geometry_and_duplicate_application_rules_are_rejected() {
    let mut config = Config::default();
    config.room.strip[1] = config.room.strip[0];
    assert!(config.validate().is_err());
    config = Config::default();
    config.room.screens[0].position.z = f32::NAN;
    assert!(config.validate().is_err());
    config = Config::default();
    config.rules.push(config.rules[0].clone());
    assert!(config.validate().is_err());
    config = Config::default();
    config.device.led_count = usize::MAX;
    assert!(config.validate().is_err());
}

fn two_screen_config() -> Config {
    let mut config = Config::default();
    config.device.led_count = 51;
    config.room.width = 5.0;
    config.room.strip = vec![
        Point {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
        Point {
            x: 5.0,
            y: 0.0,
            z: 1.0,
        },
    ];
    config.room.screens[0].position = Point {
        x: 0.0,
        y: 0.2,
        z: 1.0,
    };
    config.room.screens.push(Screen {
        id: 2,
        name: "Right".into(),
        position: Point {
            x: 5.0,
            y: 0.2,
            z: 1.0,
        },
        width: 0.7,
        angle: 0.0,
        connector: None,
        aspect_ratio: 16.0 / 9.0,
    });
    config.rules = vec![
        Rule {
            application: "mail".into(),
            screen_id: 2,
            color: [255, 80, 0],
            duration: 1.0,
            spread: 1.0,
            enabled: true,
            options: Default::default(),
        },
        Rule {
            application: "*".into(),
            screen_id: 1,
            color: [0, 80, 255],
            duration: 1.0,
            spread: 1.0,
            enabled: true,
            options: Default::default(),
        },
    ];
    config.brightness = 0.1;
    config
}

#[test]
fn application_routing_precedence_and_channel_cap_are_observable() {
    let config = two_screen_config();
    let mut engine = Engine::new(&config).unwrap();
    let now = Instant::now();
    assert!(engine.notify("MAIL", 1, now, now, None));
    let pixels = engine.render(now + Duration::from_millis(300), false);
    assert_eq!(pixels[0], [0, 0, 0]);
    assert!(pixels[50][0] > 0);
    assert!(pixels.iter().flatten().all(|v| *v <= 25));
    engine.clear();
    engine.notify("unknown", 1, now, now, None);
    let pixels = engine.render(now + Duration::from_millis(300), false);
    assert!(pixels[0][2] > 0);
    assert_eq!(pixels[50], [0, 0, 0]);
}

#[test]
fn disabled_rule_suppresses_fallback_and_stale_notifications_do_not_replay() {
    let mut config = two_screen_config();
    config.rules[0].enabled = false;
    let mut engine = Engine::new(&config).unwrap();
    let now = Instant::now();
    assert!(!engine.notify("mail", 1, now, now, None));
    assert!(!engine.notify("other", 1, now, now + Duration::from_secs(2), None));
    engine.render(now, false);
    assert!(!engine.is_active());
}

#[test]
fn lock_clears_notifications_and_media_instead_of_replaying_on_unlock() {
    let mut engine = Engine::new(&two_screen_config()).unwrap();
    let now = Instant::now();
    engine.notify("mail", 2, now, now, None);
    engine.set_playing(&["mail".into()]);
    assert!(
        engine
            .render(now + Duration::from_millis(300), false)
            .iter()
            .flatten()
            .any(|v| *v > 0)
    );
    assert!(
        engine
            .render(now + Duration::from_millis(400), true)
            .iter()
            .flatten()
            .all(|v| *v == 0)
    );
    assert!(
        engine
            .render(now + Duration::from_millis(500), false)
            .iter()
            .flatten()
            .all(|v| *v == 0)
    );
    assert!(!engine.is_active());
}

#[test]
fn bursts_are_bounded_and_expire_even_with_reduced_motion() {
    let mut config = two_screen_config();
    config.reduced_motion = true;
    let mut engine = Engine::new(&config).unwrap();
    let now = Instant::now();
    for n in 0..100 {
        engine.notify(&format!("source{n}"), 1, now, now, None);
    }
    assert_eq!(engine.pulse_count(), 32);
    assert!(engine.render(now, false).iter().flatten().any(|v| *v > 0));
    assert!(
        engine
            .render(now + Duration::from_secs(2), false)
            .iter()
            .flatten()
            .all(|v| *v == 0)
    );
    assert!(!engine.is_active());
}

#[test]
fn explicit_led_allocations_preserve_bends_and_reverse_exactly() {
    use ledalert::engine::sample_allocated_strip;
    let points = [
        Point {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        Point {
            x: 4.0,
            y: 0.0,
            z: 0.0,
        },
        Point {
            x: 4.0,
            y: 4.0,
            z: 2.0,
        },
    ];
    let forward = sample_allocated_strip(&points, 11, false, &[0, 2, 10]);
    assert_eq!(forward[2], points[1]);
    assert_eq!(
        forward[6],
        Point {
            x: 4.0,
            y: 2.0,
            z: 1.0
        }
    );
    let reversed = sample_allocated_strip(&points, 11, true, &[0, 2, 10]);
    assert!(forward.iter().eq(reversed.iter().rev()));
    assert!(sample_allocated_strip(&points, 11, false, &[0, 10, 2]).is_empty());
    let mut config = Config::default();
    config.room.strip = points.to_vec();
    config.room.led_anchors = vec![0, 2, 10];
    config.device.led_count = 11;
    assert!(config.validate().is_ok());
    config.device.led_count = 12;
    assert!(config.validate().is_err());
}

#[test]
fn advanced_rules_filter_triggers_and_bound_steady_notification_output() {
    let mut config = two_screen_config();
    let rule = &mut config.rules[0];
    rule.options.minimum_urgency = 2;
    rule.options.media = false;
    rule.options.fade = false;
    rule.options.critical_accent = false;
    rule.options.intensity = 0.5;
    let mut engine = Engine::new(&config).unwrap();
    let now = Instant::now();
    assert!(!engine.notify("mail", 1, now, now, None));
    engine.set_playing(&["mail".into()]);
    assert!(engine.render(now, false).iter().flatten().all(|v| *v == 0));
    assert!(!engine.is_active());
    assert!(engine.notify("mail", 2, now, now, None));
    assert_eq!(engine.render(now, false)[50], [12, 4, 0]);
    assert_eq!(
        engine.render(now + Duration::from_millis(900), false)[50],
        [12, 4, 0]
    );
    assert!(
        engine
            .render(now + Duration::from_secs(1), false)
            .iter()
            .flatten()
            .all(|v| *v == 0)
    );
    config.rules[0].options.notifications = false;
    config.rules[0].options.media = true;
    engine.configure(&config).unwrap();
    assert!(!engine.notify("mail", 2, now, now, None));
    engine.set_playing(&["mail".into()]);
    assert_eq!(engine.render(now, false)[50], [2, 0, 0]);
}

#[test]
fn saved_first_release_rooms_keep_their_rendering_when_loaded() {
    let config = two_screen_config();
    let mut old = serde_json::to_value(&config).unwrap();
    old["version"] = serde_json::json!(1);
    old["room"].as_object_mut().unwrap().remove("led_anchors");
    for screen in old["room"]["screens"].as_array_mut().unwrap() {
        screen.as_object_mut().unwrap().remove("connector");
        screen.as_object_mut().unwrap().remove("aspect_ratio");
    }
    for rule in old["rules"].as_array_mut().unwrap() {
        rule.as_object_mut().unwrap().remove("options");
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.json");
    let original = serde_json::to_vec(&old).unwrap();
    std::fs::write(&path, &original).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(loaded, config);
    let mut before = Engine::new(&config).unwrap();
    let mut after = Engine::new(&loaded).unwrap();
    let now = Instant::now();
    before.notify("mail", 2, now, now, None);
    after.notify("mail", 2, now, now, None);
    assert_eq!(
        before.render(now + Duration::from_millis(300), false),
        after.render(now + Duration::from_millis(300), false)
    );
    assert!(after.frame()[50][0] > 0);
}

#[test]
fn version_one_migration_preserves_allocations_and_only_explicit_save_upgrades_the_file() {
    let mut expected = Config::default();
    expected.room.reverse = true;
    expected.room.led_anchors = vec![0, 7, 120, 599];
    expected.rules[0].options.position = Some(0.37);
    let mut old = serde_json::to_value(&expected).unwrap();
    old["version"] = serde_json::json!(1);
    let original = serde_json::to_vec(&old).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("setup.json");
    std::fs::write(&path, &original).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded, expected);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(
        Engine::new(&loaded).unwrap().positions(),
        Engine::new(&expected).unwrap().positions()
    );
    loaded.save(&path).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["version"], 2);
    assert!(saved["room"].get("outline").is_none());
    assert_eq!(Config::load(&path).unwrap(), expected);
}

#[test]
fn unsupported_versions_and_version_one_outlines_are_rejected_without_rewriting() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("setup.json");
    let mut value = serde_json::to_value(Config::default()).unwrap();
    for version in [0, 3, u32::MAX] {
        value["version"] = serde_json::json!(version);
        let bytes = serde_json::to_vec(&value).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert!(Config::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    value["version"] = serde_json::json!(1);
    value["room"]["outline"] = serde_json::json!([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    let bytes = serde_json::to_vec(&value).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(Config::load(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    value["room"]["outline"] = serde_json::json!([]);
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(Config::load(&path).unwrap(), Config::default());
}

#[test]
fn concave_config_validation_checks_whole_segments_and_display_anchors_before_saving() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("setup.json");
    let mut config = Config::default();
    config.room.width = 10.0;
    config.room.depth = 10.0;
    config.room.screens[0].position = Point {
        x: 2.0,
        y: 2.0,
        z: 1.0,
    };
    config.room.strip = vec![
        Point {
            x: 8.0,
            y: 2.0,
            z: 1.0,
        },
        Point {
            x: 2.0,
            y: 8.0,
            z: 1.0,
        },
    ];
    config.save(&path).unwrap();
    let rectangle = std::fs::read(&path).unwrap();
    config.room.outline = vec![
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 0.4],
        [0.4, 0.4],
        [0.4, 1.0],
        [0.0, 1.0],
    ];
    assert!(
        config
            .room
            .strip
            .iter()
            .all(|&point| config.room.contains_floor(point))
    );
    assert!(config.save(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), rectangle);
    config.room.strip[1].y = 2.0;
    config.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap(), config);
    let concave = std::fs::read(&path).unwrap();
    config.room.screens[0].position = Point {
        x: 8.0,
        y: 8.0,
        z: 1.0,
    };
    assert!(config.save(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), concave);
    config.room.screens[0].position = Point {
        x: 2.0,
        y: 2.0,
        z: 1.0,
    };
    config.room.outline.reverse();
    assert!(config.save(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), concave);
}

#[test]
fn shape_only_changes_preserve_led_sampling_order_and_application_routing() {
    for reverse in [false, true] {
        for explicit_allocation in [false, true] {
            let mut config = two_screen_config();
            config.room.reverse = reverse;
            config.room.strip.insert(
                1,
                Point {
                    x: 1.0,
                    y: 0.0,
                    z: 1.0,
                },
            );
            if explicit_allocation {
                config.room.led_anchors = vec![0, 7, 50];
            }
            let mut engine = Engine::new(&config).unwrap();
            let positions = engine.positions().to_vec();
            let now = Instant::now();
            assert!(engine.notify("MAIL", 1, now, now, None));
            let frame = engine
                .render(now + Duration::from_millis(300), false)
                .to_vec();
            config.room.outline = vec![
                [0.0, 0.0],
                [1.0, 0.0],
                [1.0, 0.4],
                [0.4, 0.4],
                [0.4, 1.0],
                [0.0, 1.0],
            ];
            engine.configure(&config).unwrap();
            assert_eq!(engine.positions(), positions);
            assert!(engine.notify("MAIL", 1, now, now, None));
            assert_eq!(
                engine.render(now + Duration::from_millis(300), false),
                frame
            );
        }
    }
}
