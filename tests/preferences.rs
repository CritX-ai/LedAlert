use ledalert::{config::Config, preferences::Preferences};

#[test]
fn guide_dismissal_survives_restart_without_saving_editor_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("room.json");
    let config = Config::default();
    config.save(&path).unwrap();
    let original = std::fs::read(&path).unwrap();
    let prefs_path = Preferences::path_for(&path).unwrap();
    assert!(!Preferences::load(&prefs_path).unwrap().guide_dismissed);
    Preferences {
        guide_dismissed: true,
        ..Default::default()
    }
    .save(&prefs_path)
    .unwrap();
    assert!(Preferences::load(&prefs_path).unwrap().guide_dismissed);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(Config::load(&path).unwrap(), config);
}

#[test]
fn malformed_preferences_are_bounded_and_failed_replacement_retains_previous_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("preferences.json");
    for bytes in [b"{broken".to_vec(), vec![b' '; 4097]] {
        std::fs::write(&path, &bytes).unwrap();
        assert!(Preferences::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    Preferences {
        guide_dismissed: true,
        ..Default::default()
    }
    .save(&path)
    .unwrap();
    let temp = directory
        .path()
        .join(format!("preferences.json.{}.tmp", std::process::id()));
    std::fs::write(temp, b"occupied").unwrap();
    assert!(
        Preferences {
            guide_dismissed: false,
            ..Default::default()
        }
        .save(&path)
        .is_err()
    );
    assert!(Preferences::load(&path).unwrap().guide_dismissed);
}
