use ledalert::{
    config::{Config, EffectKind, GradientStop, MAX_RULES, NotificationMode, RangeUnit},
    engine::Engine,
};
use std::time::{Duration, Instant};

fn demo_config(rule_count: usize) -> Config {
    let mut config = Config::default();
    config.device.led_count = MAX_RULES;
    config.brightness = 1.0;
    let mut rule = config.rules[0].clone();
    rule.color = [200, 0, 0];
    rule.duration = 2.0;
    rule.spread = 1.0;
    rule.options.range_unit = RangeUnit::Leds;
    rule.options.intensity = 0.5;
    rule.options.fade = false;
    rule.options.critical_accent = false;
    config.rules = (0..rule_count)
        .map(|index| {
            let mut rule = rule.clone();
            rule.application = format!("demo-{index}");
            rule.options.position = Some(index as f32 / (MAX_RULES - 1) as f32);
            rule
        })
        .collect();
    config
}

#[test]
fn every_exact_rule_including_wildcard_plays_beside_a_full_real_notification_queue() {
    let mut config = demo_config(MAX_RULES);
    config.rules[MAX_RULES / 2].application = "*".into();
    for rule in &mut config.rules {
        rule.options.mode = NotificationMode::Persistent;
    }
    let now = Instant::now();
    let mut engine = Engine::new(&config).unwrap();
    for id in 1..=32 {
        assert!(engine.notify("demo-0", 1, now, now, Some((7, id))));
    }
    for index in 0..MAX_RULES {
        assert!(engine.demo_rule(index, now));
    }
    let frame = engine.render(now, false);
    assert_eq!(frame[0], [255, 0, 0]);
    for (index, pixel) in frame.iter().enumerate().skip(1) {
        assert_eq!(*pixel, [100, 0, 0], "exact demo rule {index}");
    }
    assert_eq!(engine.persistent_count(None), 32);
    assert!(!engine.notify("demo-0", 1, now, now, Some((7, 33))));
    engine.dismiss_persistent(None);
    assert_eq!(engine.render(now, false), vec![[100, 0, 0]; MAX_RULES]);
    assert_eq!(engine.persistent_count(None), 0);
}

#[test]
fn demo_clearing_and_real_dismissals_preserve_the_other_source_on_the_same_rule() {
    let mut config = demo_config(1);
    config.rules[0].options.mode = NotificationMode::Persistent;
    let now = Instant::now();
    let mut engine = Engine::new(&config).unwrap();
    assert!(engine.notify("demo-0", 1, now, now, Some((4, 11))));
    assert!(engine.demo_rule(0, now));
    assert_eq!(engine.render(now, false)[0], [200, 0, 0]);
    assert_eq!(engine.persistent_count(Some(0)), 1);

    engine.clear_demos();
    assert_eq!(engine.render(now, false)[0], [100, 0, 0]);
    assert!(!engine.demo_active(None));
    assert_eq!(engine.persistent_count(None), 1);
    assert!(engine.demo_rule(0, now));
    engine.dismiss((4, 11));
    assert_eq!(engine.render(now, false)[0], [100, 0, 0]);
    assert!(engine.demo_active(Some(0)));
    assert_eq!(engine.persistent_count(None), 0);

    assert!(engine.notify("demo-0", 1, now, now, Some((4, 12))));
    engine.dismiss_persistent(Some(0));
    assert_eq!(engine.render(now, false)[0], [100, 0, 0]);
    assert!(engine.demo_active(None));
    assert!(engine.notify("demo-0", 1, now, now, Some((4, 13))));
    engine.dismiss_persistent(None);
    assert_eq!(engine.render(now, false)[0], [100, 0, 0]);
    assert_eq!(engine.pulse_count(), 0);
}

#[test]
fn repeated_one_off_demo_restarts_without_stacking_or_restarting_real_notifications() {
    let config = demo_config(1);
    let now = Instant::now();
    let restart = now + Duration::from_secs(1);
    let mut engine = Engine::new(&config).unwrap();
    assert!(engine.notify("demo-0", 1, now, now, Some((2, 1))));
    assert!(engine.demo_rule(0, now));
    assert!(engine.demo_rule(0, restart));
    assert_eq!(engine.render(restart, false)[0], [200, 0, 0]);
    assert_eq!(
        engine.render(now + Duration::from_millis(2500), false)[0],
        [100, 0, 0]
    );
    assert_eq!(engine.pulse_count(), 0);
    assert!(engine.demo_active(Some(0)));
    assert_eq!(
        engine.render(now + Duration::from_secs(3), false)[0],
        [0; 3]
    );
    assert!(!engine.demo_active(None));
}

#[test]
fn demo_lifetime_follows_persistent_and_one_off_rule_settings() {
    let mut config = demo_config(1);
    config.rules[0].options.mode = NotificationMode::Persistent;
    config.rules[0].options.effect = EffectKind::Ripple;
    let now = Instant::now();
    let later = now + Duration::from_secs(60);
    let mut engine = Engine::new(&config).unwrap();
    assert!(engine.demo_rule(0, now));
    assert_eq!(engine.render(later, false)[0], [100, 0, 0]);
    assert!(engine.demo_active(None));
    assert_eq!(engine.persistent_count(None), 0);

    config.rules[0].options.mode = NotificationMode::OneOff;
    engine.configure(&config).unwrap();
    assert!(engine.demo_rule(0, later));
    assert_eq!(engine.render(later, false)[0], [0; 3]);
    assert!(engine.render(later + Duration::from_millis(300), false)[0][0] > 0);
    assert!(
        engine
            .render(later + Duration::from_secs(2), false)
            .iter()
            .all(|pixel| *pixel == [0; 3])
    );
    assert!(!engine.demo_active(None));
}

#[test]
fn real_demo_and_media_overlaps_mix_before_the_shared_brightness_cap() {
    let mut config = demo_config(3);
    config.brightness = 0.5;
    for rule in &mut config.rules {
        rule.options.position = Some(0.0);
        rule.options.intensity = 1.0;
    }
    config.rules[0].color = [240, 40, 0];
    config.rules[1].color = [0, 40, 240];
    config.rules[2].color = [0, 200, 0];
    let now = Instant::now();
    let mut engine = Engine::new(&config).unwrap();
    assert!(engine.notify("demo-0", 1, now, now, None));
    assert!(engine.demo_rule(0, now));
    assert!(engine.demo_rule(1, now));
    engine.set_playing(&["demo-2".into()]);
    assert_eq!(engine.render(now, false)[0], [127, 42, 63]);
    assert!(
        engine
            .frame()
            .iter()
            .flatten()
            .all(|channel| *channel <= 127)
    );

    engine.clear_demos();
    assert_eq!(engine.render(now, false)[0], [120, 40, 0]);
}

#[test]
fn demo_uses_selected_critical_gradient_ripple_range_and_extended_duration() {
    let mut config = demo_config(2);
    config.brightness = 0.5;
    let rule = &mut config.rules[1];
    rule.duration = 7.0;
    rule.spread = 100.0;
    rule.options.position = Some(0.0);
    rule.options.effect = EffectKind::Ripple;
    rule.options.minimum_urgency = 2;
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
    let mut engine = Engine::new(&config).unwrap();
    assert!(engine.demo_rule(1, now));
    let middle = engine.render(now + Duration::from_millis(4375), false);
    assert_eq!(middle[0], [0; 3]);
    for (actual, expected) in middle[50].into_iter().zip([55_u8, 25, 18]) {
        assert!(actual.abs_diff(expected) <= 1);
    }
    assert!(middle[101..].iter().all(|pixel| *pixel == [0; 3]));
    assert!(
        engine
            .render(now + Duration::from_secs(7), false)
            .iter()
            .any(|pixel| *pixel != [0; 3])
    );
    assert!(
        engine
            .render(now + Duration::from_millis(8750), false)
            .iter()
            .all(|pixel| *pixel == [0; 3])
    );
    assert!(!engine.demo_active(Some(1)));
}

#[test]
fn disabled_and_filtered_rules_do_not_fall_through_to_an_eligible_wildcard() {
    let mut config = demo_config(4);
    config.rules[0].enabled = false;
    config.rules[1].options.notifications = false;
    config.rules[2].options.intensity = 0.0;
    config.rules[3].application = "*".into();
    let now = Instant::now();
    let mut engine = Engine::new(&config).unwrap();
    for index in [0, 1, 2, config.rules.len(), usize::MAX] {
        assert!(!engine.demo_eligible(index));
        assert!(!engine.demo_rule(index, now));
    }
    assert!(
        engine
            .render(now, false)
            .iter()
            .all(|pixel| *pixel == [0; 3])
    );
    assert!(engine.demo_eligible(3));
    assert!(engine.demo_rule(3, now));
    assert_eq!(engine.render(now, false)[3], [100, 0, 0]);

    config.notifications_enabled = false;
    engine.configure(&config).unwrap();
    for index in 0..config.rules.len() {
        assert!(!engine.demo_eligible(index));
        assert!(!engine.demo_rule(index, now));
    }
    assert!(
        engine
            .render(now, false)
            .iter()
            .all(|pixel| *pixel == [0; 3])
    );
}

#[test]
fn configuration_changes_drop_demo_indices_instead_of_retargeting_or_dangling() {
    let mut config = demo_config(3);
    for rule in &mut config.rules {
        rule.options.mode = NotificationMode::Persistent;
    }
    let now = Instant::now();
    let mut engine = Engine::new(&config).unwrap();
    assert!(engine.demo_rule(1, now));
    assert!(engine.notify("demo-2", 1, now, now, Some((3, 1))));
    engine.configure(&config).unwrap();
    assert_eq!(engine.render(now, false)[1], [100, 0, 0]);

    config.rules.remove(0);
    engine.configure(&config).unwrap();
    assert!(!engine.demo_active(None));
    assert_eq!(engine.render(now, false)[1], [0; 3]);
    assert_eq!(engine.frame()[2], [100, 0, 0]);
    assert_eq!(engine.persistent_count(Some(1)), 1);
    assert!(engine.demo_rule(1, now));
    config.rules.truncate(1);
    engine.configure(&config).unwrap();
    assert!(!engine.demo_active(None));
    assert!(
        engine
            .render(now, false)
            .iter()
            .all(|pixel| *pixel == [0; 3])
    );
}

#[test]
fn inhibition_discards_even_persistent_demos_and_clear_discards_all_sources() {
    let mut config = demo_config(1);
    config.rules[0].options.mode = NotificationMode::Persistent;
    let now = Instant::now();
    let mut engine = Engine::new(&config).unwrap();
    assert!(engine.notify("demo-0", 1, now, now, Some((1, 1))));
    assert!(engine.demo_rule(0, now));
    engine.clear_transients();
    assert!(!engine.demo_active(None));
    assert_eq!(engine.render(now, false)[0], [100, 0, 0]);
    assert!(engine.demo_rule(0, now));
    assert!(
        engine
            .render(now, true)
            .iter()
            .all(|pixel| *pixel == [0; 3])
    );
    assert!(!engine.demo_active(None));
    assert_eq!(engine.render(now, false)[0], [100, 0, 0]);

    assert!(engine.demo_rule(0, now));
    engine.set_playing(&["demo-0".into()]);
    engine.clear();
    assert!(!engine.demo_active(None));
    assert_eq!(engine.persistent_count(None), 0);
    assert!(
        engine
            .render(now, false)
            .iter()
            .all(|pixel| *pixel == [0; 3])
    );
    assert!(!engine.is_active());
}
