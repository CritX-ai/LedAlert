use ledalert::config::Config;
use ledalert::displays::import_displays;
use ledalert::displays::macos::{DisplayBounds, from_bounds};

fn display(uuid: &str, x: f32, y: f32, width: f32, height: f32, main: bool) -> DisplayBounds {
    DisplayBounds {
        uuid: uuid.into(),
        name: format!("Name {uuid}"),
        x,
        y,
        width,
        height,
        main,
    }
}

fn laptop() -> DisplayBounds {
    // A 3024×1964-pixel retina panel at 2× scale reports 1512×982 points.
    display(
        "A1B2C3D4-0011-2233-4455-667788990000",
        0.0,
        0.0,
        1512.0,
        982.0,
        true,
    )
}

fn external() -> DisplayBounds {
    // A 4320×3840-pixel panel at 4× scale reports 1080×960 points.
    display(
        "D4C3B2A1-9988-7766-5544-332211000011",
        -1080.0,
        22.0,
        1080.0,
        960.0,
        false,
    )
}

#[test]
fn mixed_dpi_bounds_share_one_coordinate_space_and_keep_adjacency() {
    let displays = from_bounds(vec![external(), laptop()]).unwrap();
    assert_eq!(displays.len(), 2);
    assert!(displays[0].primary);
    assert!(!displays[1].primary);
    assert_eq!(displays[0].x, 0.0);
    assert_eq!(displays[0].width, 1512.0);
    // The external display sits to the left in the same point space, without
    // any per-display scale division.
    assert_eq!(displays[1].x, -1080.0);
    assert_eq!(displays[1].x + displays[1].width, displays[0].x);
    let mut config = Config::default();
    import_displays(&mut config, &displays, true).unwrap();
    let external_screen = config
        .room
        .screens
        .iter()
        .find(|screen| screen.connector.as_deref() == Some(displays[1].connector.as_str()))
        .unwrap();
    assert_eq!(external_screen.aspect_ratio, 1080.0 / 960.0);
    let main_screen = config
        .room
        .screens
        .iter()
        .find(|screen| screen.connector.as_deref() == Some(displays[0].connector.as_str()))
        .unwrap();
    assert!(main_screen.position.x > external_screen.position.x);
}

#[test]
fn uuid_identities_retain_saved_edits_when_names_and_geometry_change() {
    let initial = from_bounds(vec![external(), laptop()]).unwrap();
    let mut config = Config::default();
    import_displays(&mut config, &initial, true).unwrap();
    config.room.screens[1].name = "My left screen".into();
    config.room.screens[1].width = 0.9;
    let saved = config.room.screens[1].clone();
    let renamed = DisplayBounds {
        name: "Renamed by the system".into(),
        ..external()
    };
    let refreshed = from_bounds(vec![laptop(), renamed]).unwrap();
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
    assert_eq!(screen.aspect_ratio, 1080.0 / 960.0);
}

#[test]
fn exactly_one_main_display_is_required() {
    assert!(from_bounds(Vec::new()).is_err());
    // Zero mains and two mains are both malformed snapshots.
    assert!(from_bounds(vec![display("X", 0.0, 0.0, 100.0, 100.0, false)]).is_err());
    assert!(from_bounds(vec![laptop(), display("X", 0.0, 0.0, 100.0, 100.0, true)]).is_err());
}

#[test]
fn malformed_native_geometry_fails_instead_of_inventing_displays() {
    let cases: Vec<(DisplayBounds, &str)> = vec![
        (display("Z", 0.0, 0.0, 0.0, 982.0, true), "zero width"),
        (display("Z", 0.0, 0.0, 1512.0, 0.0, true), "zero height"),
        (
            display("Z", f32::NAN, 0.0, 1512.0, 982.0, true),
            "NaN origin",
        ),
        (
            display("Z", 200_000.0, 0.0, 1512.0, 982.0, true),
            "coordinate bound",
        ),
        (
            display("Z", 0.0, 0.0, 1512.0, 16_000.0, true),
            "aspect bound",
        ),
        (
            display("", 0.0, 0.0, 1512.0, 982.0, true),
            "empty connector",
        ),
        (
            display("bad\nidentity", 0.0, 0.0, 1512.0, 982.0, true),
            "control connector",
        ),
        (
            display("Z", 0.0, 0.0, 1512.0, 982.0, true),
            "duplicate connector",
        ),
    ];
    for (bound, case) in &cases {
        let mut bounds = vec![laptop(), bound.clone()];
        if *case == "duplicate connector" {
            bounds[1] = DisplayBounds {
                main: false,
                ..laptop()
            };
        }
        assert!(from_bounds(bounds).is_err(), "expected failure: {case}");
    }
    let many: Vec<DisplayBounds> = (0..129)
        .map(|index| {
            display(
                &format!("UUID{index:03}"),
                index as f32 * 2000.0,
                0.0,
                1512.0,
                982.0,
                index == 0,
            )
        })
        .collect();
    assert!(from_bounds(many).is_err());
}
