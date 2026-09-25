use ledalert::{
    config::Config,
    displays::{
        import_displays,
        windows::{DisplayPath, from_paths},
    },
};

fn path(id: &str, x: i32, y: i32, width: u32, height: u32) -> DisplayPath {
    DisplayPath {
        monitor_path: format!(r"\\?\DISPLAY#DEL1234#{id}#{{monitor}}"),
        name: "Dell monitor".into(),
        x,
        y,
        width,
        height,
    }
}

#[test]
fn mixed_dpi_pixel_topology_keeps_negative_origins_and_portrait_dimensions() {
    // CCD reports desktop source pixels, already oriented. Independent DPI
    // division or another rotation would destroy this left-edge adjacency.
    let displays = from_paths(vec![
        path("portrait", -1440, -160, 1440, 2560),
        path("primary", 0, 0, 3840, 2160),
    ])
    .unwrap();
    assert!(displays[0].primary);
    assert!(!displays[1].primary);
    assert_eq!(
        (
            displays[1].x,
            displays[1].y,
            displays[1].width,
            displays[1].height
        ),
        (-1440.0, -160.0, 1440.0, 2560.0)
    );
    assert_eq!(displays[1].x + displays[1].width, displays[0].x);
    let mut config = Config::default();
    import_displays(&mut config, &displays, true).unwrap();
    assert_eq!(config.room.screens[1].aspect_ratio, 1440.0 / 2560.0);
    assert!(config.room.screens[1].position.x < config.room.screens[0].position.x);
}

#[test]
fn monitor_device_identity_retains_edits_when_enumeration_and_names_change() {
    let initial = from_paths(vec![
        path("left", 0, 0, 1920, 1080),
        path("right", 1920, 0, 1920, 1080),
    ])
    .unwrap();
    let mut config = Config::default();
    import_displays(&mut config, &initial, true).unwrap();
    config.room.screens[1].name = "My right screen".into();
    config.room.screens[1].width = 0.9;
    let saved = config.room.screens[1].clone();
    let mut changed = path("right", -1080, 0, 1080, 1920);
    changed.name = "Renamed by driver".into();
    let refreshed = from_paths(vec![changed, path("left", 0, 0, 1920, 1080)]).unwrap();
    assert_eq!(import_displays(&mut config, &refreshed, false).unwrap(), 0);
    let screen = config
        .room
        .screens
        .iter()
        .find(|screen| screen.id == saved.id)
        .unwrap();
    assert_eq!(screen.name, saved.name);
    assert_eq!(screen.position, saved.position);
    assert_eq!(screen.width, saved.width);
    assert_eq!(screen.aspect_ratio, 1080.0 / 1920.0);
}

#[test]
fn clone_targets_have_distinct_saved_identities_even_with_shared_source_geometry() {
    let cloned = from_paths(vec![
        path("one", 0, 0, 1920, 1080),
        path("two", 0, 0, 1920, 1080),
    ])
    .unwrap();
    assert_ne!(cloned[0].connector, cloned[1].connector);
    let mut config = Config::default();
    import_displays(&mut config, &cloned, true).unwrap();
    assert_ne!(config.room.screens[0].id, config.room.screens[1].id);
    assert_eq!(
        config.room.screens[0].position,
        config.room.screens[1].position
    );
    assert!(
        from_paths(vec![
            path("one", 0, 0, 1920, 1080),
            path("one", 1920, 0, 1920, 1080)
        ])
        .is_err()
    );
}

#[test]
fn invalid_native_geometry_fails_without_importing_partial_results() {
    for invalid in [
        path("bad", 0, 0, 0, 1080),
        path("bad", i32::MAX, 0, 1920, 1080),
        path("bad", 0, 0, 1920, u32::MAX),
        path("bad\nidentity", 0, 0, 1920, 1080),
    ] {
        assert!(from_paths(vec![path("good", 0, 0, 1920, 1080), invalid]).is_err());
    }
}
