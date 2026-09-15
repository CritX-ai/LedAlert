use ledalert::{
    config::{Config, EffectKind, GradientStop, MAX_LEDS, NotificationMode, Point, RangeUnit},
    engine::Engine,
};
use std::time::{Duration, Instant};

fn line_config(unit: RangeUnit) -> Config {
    let mut config = Config::default();
    config.device.led_count = 101;
    config.room.width = 10.0;
    config.room.strip = vec![
        Point {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
        Point {
            x: 10.0,
            y: 0.0,
            z: 1.0,
        },
    ];
    config.room.screens[0].position = Point {
        x: 0.0,
        y: 0.2,
        z: 1.0,
    };
    config.brightness = 1.0;
    let rule = &mut config.rules[0];
    rule.color = [255, 0, 0];
    rule.duration = 7.0;
    rule.spread = match unit {
        RangeUnit::Room => 10.0,
        RangeUnit::Leds => 100.0,
    };
    rule.options.range_unit = unit;
    rule.options.fade = false;
    rule.options.critical_accent = false;
    config
}

fn allocated_config(unit: RangeUnit) -> Config {
    let mut config = line_config(unit);
    config.device.led_count = 11;
    config.room.strip = vec![
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
            y: 0.0,
            z: 2.0,
        },
    ];
    config.room.led_anchors = vec![0, 2, 10];
    config.room.screens[0].position = Point {
        x: 4.0,
        y: 0.2,
        z: 0.0,
    };
    config.rules[0].spread = 2.0;
    config
}

fn assert_color_near(actual: [u8; 3], expected: [u8; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "channel {actual}, expected {expected}"
        );
    }
}

#[test]
fn ripples_cross_the_selected_radius_once_and_expire_in_both_units() {
    for unit in [RangeUnit::Room, RangeUnit::Leds] {
        let mut config = line_config(unit);
        config.rules[0].options.effect = EffectKind::Ripple;
        let mut engine = Engine::new(&config).unwrap();
        let now = Instant::now();
        assert!(engine.notify("mail", 1, now, now, None));
        assert!(
            engine
                .render(now, false)
                .iter()
                .all(|pixel| *pixel == [0; 3])
        );
        let near = engine.render(now + Duration::from_secs(1), false);
        assert_color_near(near[0], [255, 0, 0]);
        assert!(near[10][0] > 0 && near[10][0] < near[0][0]);
        assert_eq!(near[50], [0; 3]);
        let middle = engine.render(now + Duration::from_millis(3500), false);
        assert_eq!(middle[0], [0; 3]);
        assert_color_near(middle[50], [255, 0, 0]);
        assert_eq!(middle[100], [0; 3]);
        let edge = engine.render(now + Duration::from_secs(6), false);
        assert_eq!(edge[0], [0; 3]);
        assert_eq!(edge[50], [0; 3]);
        assert_color_near(edge[100], [255, 0, 0]);
        assert!(
            engine
                .render(now + Duration::from_secs(7), false)
                .iter()
                .all(|pixel| *pixel == [0; 3])
        );
        assert_eq!(engine.pulse_count(), 0);
        assert!(!engine.is_active());
        assert!(
            engine
                .render(now + Duration::from_secs(8), false)
                .iter()
                .all(|pixel| *pixel == [0; 3])
        );
    }
}

#[test]
fn ripple_band_edges_cannot_escape_the_selected_range() {
    for unit in [RangeUnit::Room, RangeUnit::Leds] {
        let mut config = line_config(unit);
        config.rules[0].spread *= 0.5;
        config.rules[0].options.effect = EffectKind::Ripple;
        config.rules[0].options.gradient = vec![
            GradientStop {
                position: 0.0,
                color: [240, 0, 0],
            },
            GradientStop {
                position: 1.0,
                color: [0, 0, 240],
            },
        ];
        let mut engine = Engine::new(&config).unwrap();
        let now = Instant::now();
        assert!(engine.notify("mail", 1, now, now, None));
        let at_edge = engine.render(now + Duration::from_secs(6), false);
        assert_color_near(at_edge[50], [0, 0, 240]);
        assert!(at_edge[51..].iter().all(|pixel| *pixel == [0; 3]));
    }
}

#[test]
fn coalesced_ripples_do_not_restart_or_move_inward() {
    let mut config = line_config(RangeUnit::Leds);
    config.rules[0].options.effect = EffectKind::Ripple;
    let mut engine = Engine::new(&config).unwrap();
    let now = Instant::now();
    assert!(engine.notify("mail", 1, now, now, None));
    let halfway = now + Duration::from_millis(3500);
    let before = engine.render(halfway, false).to_vec();
    assert!(engine.notify("mail", 1, halfway, halfway, None));
    assert_eq!(engine.pulse_count(), 1);
    assert_eq!(engine.render(halfway, false), before);
    let advanced = engine.render(now + Duration::from_secs(7), false);
    assert_eq!(advanced[50], [0; 3]);
    assert_color_near(advanced[85], [255, 0, 0]);
    assert!(
        engine
            .render(now + Duration::from_millis(10500), false)
            .iter()
            .all(|pixel| *pixel == [0; 3])
    );
    assert!(!engine.is_active());
}

#[test]
fn physical_and_index_ranges_follow_the_same_anchor_through_allocation_and_reversal() {
    let now = Instant::now();
    let mut frames = Vec::new();
    for unit in [RangeUnit::Room, RangeUnit::Leds] {
        let mut config = allocated_config(unit);
        let mut engine = Engine::new(&config).unwrap();
        assert!(engine.notify("mail", 1, now, now, None));
        let forward = engine.render(now, false).to_vec();
        assert_eq!(forward[2], [255, 0, 0]);
        config.room.reverse = true;
        engine.configure(&config).unwrap();
        assert!(engine.notify("mail", 1, now, now, None));
        assert!(engine.render(now, false).iter().eq(forward.iter().rev()));
        frames.push(forward);
    }
    // LED 1 is two metres but one device index from the shared anchor at LED 2.
    assert_eq!(frames[0][1], [0; 3]);
    assert_eq!(frames[1][1], [63, 0, 0]);
    // LED 5 is only 75 cm up the vertical segment, but three device indices away.
    assert_eq!(frames[0][5], [99, 0, 0]);
    assert_eq!(frames[1][5], [0; 3]);
}

#[test]
fn gradients_use_radial_positions_including_endpoints_and_uneven_stop_interpolation() {
    for unit in [RangeUnit::Room, RangeUnit::Leds] {
        let mut config = line_config(unit);
        config.device.led_count = 201;
        let rule = &mut config.rules[0];
        if unit == RangeUnit::Leds {
            rule.spread = 200.0;
        }
        rule.options.effect = EffectKind::Ripple;
        rule.options.gradient = vec![
            GradientStop {
                position: 0.0,
                color: [240, 0, 0],
            },
            GradientStop {
                position: 0.25,
                color: [0, 240, 0],
            },
            GradientStop {
                position: 1.0,
                color: [0, 0, 240],
            },
        ];
        let mut engine = Engine::new(&config).unwrap();
        let now = Instant::now();
        assert!(engine.notify("mail", 1, now, now, None));
        assert_color_near(
            engine.render(now + Duration::from_secs(1), false)[0],
            [240, 0, 0],
        );
        assert_color_near(
            engine.render(now + Duration::from_millis(2250), false)[50],
            [0, 240, 0],
        );
        assert_color_near(
            engine.render(now + Duration::from_millis(4125), false)[125],
            [0, 120, 120],
        );
        assert_color_near(
            engine.render(now + Duration::from_secs(6), false)[200],
            [0, 0, 240],
        );
    }
}

#[test]
fn critical_gradients_accent_the_interpolated_color_for_the_extended_duration() {
    let mut config = line_config(RangeUnit::Leds);
    config.brightness = 0.5;
    let rule = &mut config.rules[0];
    rule.options.effect = EffectKind::Ripple;
    rule.options.intensity = 0.5;
    rule.options.critical_accent = true;
    rule.options.gradient = vec![
        GradientStop {
            position: 0.0,
            color: [0, 200, 200],
        },
        GradientStop {
            position: 1.0,
            color: [240, 0, 0],
        },
    ];
    let now = Instant::now();
    let sample = now + Duration::from_millis(4375);
    let mut critical = Engine::new(&config).unwrap();
    assert!(critical.notify("mail", 2, now, now, None));
    let accented = critical.render(sample, false).to_vec();
    assert_color_near(accented[50], [55, 25, 18]);
    assert!(
        critical
            .render(now + Duration::from_secs(7), false)
            .iter()
            .any(|pixel| *pixel != [0; 3])
    );
    assert!(
        critical
            .render(now + Duration::from_millis(8750), false)
            .iter()
            .all(|pixel| *pixel == [0; 3])
    );
}

#[test]
fn reduced_motion_overrides_ripple_and_fade_with_a_finite_steady_gradient_glow() {
    for unit in [RangeUnit::Room, RangeUnit::Leds] {
        let mut config = line_config(unit);
        config.reduced_motion = true;
        config.rules[0].options.effect = EffectKind::Ripple;
        config.rules[0].options.fade = true;
        config.rules[0].options.gradient = vec![
            GradientStop {
                position: 0.0,
                color: [240, 0, 0],
            },
            GradientStop {
                position: 1.0,
                color: [0, 0, 240],
            },
        ];
        let mut engine = Engine::new(&config).unwrap();
        let now = Instant::now();
        assert!(engine.notify("mail", 1, now, now, None));
        let initial = engine.render(now, false).to_vec();
        assert_eq!(initial[0], [240, 0, 0]);
        assert_eq!(initial[50], [30, 0, 30]);
        assert_eq!(engine.render(now + Duration::from_secs(3), false), initial);
        assert_eq!(
            engine.render(now + Duration::from_millis(6900), false),
            initial
        );
        assert!(
            engine
                .render(now + Duration::from_secs(7), false)
                .iter()
                .all(|pixel| *pixel == [0; 3])
        );
        assert!(!engine.is_active());
    }
}

#[test]
fn media_remains_quiet_and_steady_with_ripple_rules_and_radial_gradients() {
    for unit in [RangeUnit::Room, RangeUnit::Leds] {
        let mut config = line_config(unit);
        config.rules[0].options.effect = EffectKind::Ripple;
        config.rules[0].options.gradient = vec![
            GradientStop {
                position: 0.0,
                color: [240, 0, 0],
            },
            GradientStop {
                position: 1.0,
                color: [0, 0, 240],
            },
        ];
        let mut engine = Engine::new(&config).unwrap();
        let now = Instant::now();
        engine.set_playing(&["player".into()]);
        let first = engine.render(now, false).to_vec();
        assert_eq!(first[0], [48, 0, 0]);
        assert_color_near(first[30], [6, 0, 6]);
        assert_eq!(first[61], [0; 3]);
        assert_eq!(engine.render(now + Duration::from_secs(60), false), first);
        assert_eq!(engine.pulse_count(), 0);
        engine.set_playing(&[]);
        assert!(
            engine
                .render(now, false)
                .iter()
                .all(|pixel| *pixel == [0; 3])
        );
        assert!(!engine.is_active());
    }
}

#[test]
fn invalid_gradients_are_rejected_before_rendering_or_saving() {
    let invalid = [
        vec![0.0],
        (0..9).map(|index| index as f32 / 8.0).collect(),
        vec![0.1, 1.0],
        vec![0.0, 0.9],
        vec![0.0, 0.5, 0.5, 1.0],
        vec![0.0, 0.75, 0.25, 1.0],
        vec![0.0, f32::NAN, 1.0],
        vec![0.0, f32::INFINITY, 1.0],
        vec![0.0, -0.1, 1.0],
        vec![0.0, 1.1, 1.0],
    ];
    for positions in invalid {
        let mut config = Config::default();
        config.rules[0].options.gradient = positions
            .into_iter()
            .map(|position| GradientStop {
                position,
                color: [255, 0, 0],
            })
            .collect();
        assert!(config.validate().is_err());
    }
}

#[test]
fn led_ranges_remain_valid_after_device_reduction_and_effect_settings_roundtrip() {
    let mut config = Config::default();
    config.rules[0].options.effect = EffectKind::Ripple;
    config.rules[0].options.range_unit = RangeUnit::Leds;
    config.rules[0].spread = MAX_LEDS as f32;
    config.rules[0].options.gradient = (0..8)
        .map(|index| GradientStop {
            position: index as f32 / 7.0,
            color: [index * 30, 0, 240 - index * 30],
        })
        .collect();
    config.validate().unwrap();
    config.device.led_count = 10;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("effects.json");
    config.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap(), config);
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(saved["rules"][0]["options"]["effect"], "ripple");
    assert_eq!(saved["rules"][0]["options"]["range_unit"], "leds");
    for invalid in [0.0, 1.5, MAX_LEDS as f32 + 1.0, f32::NAN, f32::INFINITY] {
        config.rules[0].spread = invalid;
        assert!(config.validate().is_err());
    }
    config.rules[0].spread = 21.0;
    config.validate().unwrap();
    config.rules[0].options.range_unit = RangeUnit::Room;
    assert!(config.validate().is_err());
    config.rules[0].spread = 0.1;
    config.validate().unwrap();
}

#[test]
fn pre_effect_rule_options_load_with_unchanged_notification_and_media_frames() {
    let mut config = line_config(RangeUnit::Room);
    config.rules[0].options.fade = true;
    config.rules[0].options.critical_accent = true;
    config.rules[0].options.intensity = 0.7;
    let mut old = serde_json::to_value(&config).unwrap();
    for rule in old["rules"].as_array_mut().unwrap() {
        let options = rule["options"].as_object_mut().unwrap();
        options.remove("effect");
        options.remove("range_unit");
        options.remove("gradient");
        options.remove("mode");
        options.remove("position");
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("old-options.json");
    std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    let loaded = Config::load(&path).unwrap();
    let mut before = Engine::new(&config).unwrap();
    let mut after = Engine::new(&loaded).unwrap();
    let now = Instant::now();
    assert!(before.notify("mail", 2, now, now, None));
    assert!(after.notify("mail", 2, now, now, None));
    for millis in [0, 125, 1000, 7000, 8750] {
        let time = now + Duration::from_millis(millis);
        assert_eq!(before.render(time, false), after.render(time, false));
    }
    before.set_playing(&["player".into()]);
    after.set_playing(&["player".into()]);
    assert_eq!(before.render(now, false), after.render(now, false));
}

#[test]
fn persistent_notifications_keep_independent_ids_and_survive_repositioning_and_inhibition() {
    let mut config = line_config(RangeUnit::Leds);
    config.rules[0].options.mode = NotificationMode::Persistent;
    config.rules[0].options.effect = EffectKind::Ripple;
    config.rules[0].options.position = Some(0.2);
    config.rules[0].spread = 2.0;
    let mut engine = Engine::new(&config).unwrap();
    let now = Instant::now();
    assert!(engine.notify("mail", 1, now, now, Some((4, 11))));
    assert!(engine.notify("mail", 1, now, now, Some((4, 12))));
    let later = now + Duration::from_secs(60);
    assert_eq!(engine.render(later, false)[20], [255, 0, 0]);
    engine.dismiss((3, 11));
    assert_eq!(engine.persistent_count(None), 2);
    engine.dismiss((4, 11));
    assert_eq!(engine.render(later, false)[20], [255, 0, 0]);
    assert_eq!(engine.persistent_count(None), 1);
    assert!(engine.notify("mail", 1, later, later, Some((4, 12))));
    assert_eq!(
        engine.render(later, false)[20],
        [255, 0, 0],
        "a settled replacement must not go dark"
    );
    config.rules[0].options.position = Some(0.8);
    engine.configure(&config).unwrap();
    let frame = engine.render(later, false);
    assert_eq!(frame[20], [0; 3]);
    assert_eq!(frame[80], [255, 0, 0]);
    assert!(
        engine
            .render(later, true)
            .iter()
            .all(|pixel| *pixel == [0; 3])
    );
    assert_eq!(engine.render(later, false)[80], [255, 0, 0]);
    engine.dismiss_persistent(Some(0));
    assert!(
        engine
            .render(later, false)
            .iter()
            .all(|pixel| *pixel == [0; 3])
    );
}

#[test]
fn explicit_positions_follow_device_indices_across_allocation_and_reversal() {
    let mut config = allocated_config(RangeUnit::Leds);
    config.rules[0].options.position = Some(0.8);
    config.rules[0].options.mode = NotificationMode::Persistent;
    config.rules[0].spread = 1.0;
    let mut engine = Engine::new(&config).unwrap();
    let now = Instant::now();
    engine.notify("mail", 1, now, now, Some((1, 1)));
    let forward = engine.render(now, false).to_vec();
    assert_eq!(forward[8], [255, 0, 0]);
    assert!(
        forward
            .iter()
            .enumerate()
            .all(|(index, pixel)| index == 8 || *pixel == [0; 3])
    );
    config.room.reverse = true;
    config.room.led_anchors = vec![0, 6, 10];
    engine.configure(&config).unwrap();
    assert_eq!(engine.render(now, false), forward);
    config.device.led_count = 21;
    config.room.led_anchors = vec![0, 12, 20];
    engine.configure(&config).unwrap();
    assert_eq!(engine.render(now, false)[16], [255, 0, 0]);
    for position in [-0.01, 1.01, f32::NAN] {
        config.rules[0].options.position = Some(position);
        assert!(config.validate().is_err());
    }
}

#[test]
fn overlapping_sources_mix_hues_commutatively_without_clipping_the_channel_ratio() {
    let mut config = line_config(RangeUnit::Leds);
    config.brightness = 0.5;
    config.rules[0].application = "red".into();
    config.rules[0].color = [240, 40, 0];
    let mut second = config.rules[0].clone();
    second.application = "blue".into();
    second.color = [0, 40, 240];
    config.rules.push(second);
    let now = Instant::now();
    let mut forward = Engine::new(&config).unwrap();
    forward.notify("red", 1, now, now, None);
    forward.notify("blue", 1, now, now, None);
    let mixed = forward.render(now, false).to_vec();
    assert_eq!(mixed[0], [120, 40, 120]);
    let mut reverse = Engine::new(&config).unwrap();
    reverse.notify("blue", 1, now, now, None);
    reverse.notify("red", 1, now, now, None);
    assert_eq!(reverse.render(now, false), mixed);
    config.rules[1].color = [240, 40, 0];
    let mut saturated = Engine::new(&config).unwrap();
    saturated.notify("red", 1, now, now, None);
    saturated.notify("blue", 1, now, now, None);
    assert_color_near(saturated.render(now, false)[0], [127, 21, 0]);
    assert!(
        saturated
            .frame()
            .iter()
            .flatten()
            .all(|channel| *channel <= 127)
    );
}
