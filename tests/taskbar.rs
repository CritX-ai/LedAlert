use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use ledalert::{
    config::{Config, Point},
    engine::Engine,
    taskbar::{AppKind, PinnedApp, application_at, discover_at, parse_launchers, suggested_rule},
};

struct Fixture {
    root: tempfile::TempDir,
    applets: PathBuf,
    directories: Vec<PathBuf>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let applets = root.path().join("appletsrc");
        let directories = vec![
            root.path().join("user/applications"),
            root.path().join("system/applications"),
        ];
        for directory in &directories {
            fs::create_dir_all(directory).unwrap();
        }
        Self {
            root,
            applets,
            directories,
        }
    }

    fn pins(&self, launchers: &str) {
        fs::write(&self.applets, taskmanager(launchers)).unwrap();
    }

    fn entry(&self, directory: usize, id: &str, body: &str) {
        let path = self.directories[directory].join(id);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn discover(&self) -> anyhow::Result<Vec<PinnedApp>> {
        discover_at(&self.applets, &self.directories)
    }
}

fn taskmanager(launchers: &str) -> String {
    format!(
        "[Containments][1][Applets][2]\nplugin=org.kde.plasma.icontasks\n\
         [Containments][1][Applets][2][Configuration][General]\nlaunchers={launchers}\n"
    )
}

fn application(name: &str, categories: &str) -> String {
    format!("[Desktop Entry]\nType=Application\nName={name}\nCategories={categories}\n")
}

#[test]
fn task_manager_scoping_preserves_pin_order_and_kconfig_escaping() {
    let fixture = Fixture::new();
    let input = r"[Containments]
launchers=applications:NotAnApplet.desktop
[Containments][1][Applets]
launchers=applications:StillNotAnApplet.desktop
[Containments][1][Applets][9]
plugin=org.kde.plasma.quicklaunch
[Containments][1][Applets][9][Configuration][General]
launchers=applications:../private.desktop,invalid\q
[Containments][1][Applets][2][Configuration][General]
launchers=applications:Mail.desktop,applications:Pixel\sEditor.desktop,applications:Comma\\, App.desktop,applications:Odd.desktop.desktop,applications:Mail.desktop,preferred://browser,applications:Space%20App.desktop
[Containments][1][Applets][2]
plugin=org.kde.plasma.icontasks
[Containments][1][Applets][2][Configuration][Other]
launchers=applications:Wrong.desktop
[Containments][3][Applets][2]
plugin=org.kde.plasma.taskmanager
[Containments][3][Applets][2][Configuration][General]
launchers=applications:Comma\, App.desktop,Second.desktop,applications:Pixel Editor.desktop
[Containments][4][Applets][2][Configuration][General]
launchers=applications:Orphan.desktop
";
    fs::write(&fixture.applets, input).unwrap();
    let expected = [
        "Mail.desktop",
        "Pixel Editor.desktop",
        "Comma, App.desktop",
        "Odd.desktop.desktop",
        "Space App.desktop",
        "Second.desktop",
    ];
    assert_eq!(parse_launchers(input).unwrap(), expected);
    for id in expected {
        fixture.entry(0, id, &application(id, "Notes;"));
    }
    fixture.entry(
        0,
        "Pixel Editor.desktop",
        &application(r"Pixel\sEditor", "TextEditor;"),
    );
    fixture.entry(0, "Wrong.desktop", &application("Not pinned", "Email;"));
    let apps = fixture.discover().unwrap();
    assert_eq!(
        apps.iter().map(|app| app.id.as_str()).collect::<Vec<_>>(),
        [
            "Mail",
            "Pixel Editor",
            "Comma, App",
            "Odd.desktop",
            "Space App",
            "Second"
        ]
    );
    assert_eq!(apps[1].name, "Pixel Editor");
}

#[test]
fn last_assignment_and_kconfig_flags_do_not_resurrect_removed_pins() {
    let input = r"[Containments][1][Applets][1][$i]
plugin[$i]=org.kde.plasma.taskmanager
[Containments][1][Applets][1][Configuration][General][$i]
launchers=applications:Old.desktop
launchers[$i]=applications:Current.desktop
[Containments][1][Applets][2]
plugin=org.kde.plasma.icontasks
[Containments][1][Applets][2][Configuration][General]
launchers=applications:Deleted.desktop
launchers[$d]
[Containments][1][Applets][3]
plugin=org.kde.plasma.icontasks
[Containments][1][Applets][3][Configuration][General]
launchers=applications:Expanded.desktop
launchers[$e]=$(touch /not-to-be-executed)
[Containments][1][Applets][4]
plugin=org.kde.plasma.icontasks
plugin=org.kde.plasma.quicklaunch
[Containments][1][Applets][4][Configuration][General]
launchers=applications:Unrelated.desktop
";
    assert_eq!(parse_launchers(input).unwrap(), ["Current.desktop"]);
}

#[test]
fn existing_entry_priority_masks_hidden_and_non_application_copies() {
    let fixture = Fixture::new();
    fixture.pins("applications:Edited.desktop,applications:Hidden.desktop,applications:Link.desktop,applications:System.desktop,applications:Missing.desktop");
    for id in [
        "Edited.desktop",
        "Hidden.desktop",
        "Link.desktop",
        "System.desktop",
    ] {
        fixture.entry(1, id, &application("System copy", "Email;"));
    }
    fixture.entry(0, "Edited.desktop", "[Desktop Entry]\nType=Application\nName=User override\nCategories=Notes;\nNoDisplay=true\n[Desktop Action New]\nName=Wrong action name\nCategories=WebBrowser;\n");
    fixture.entry(0, "Hidden.desktop", "[Desktop Entry]\nHidden=true\n");
    fixture.entry(
        0,
        "Link.desktop",
        "[Desktop Entry]\nType=Link\nName=Not an application\nURL=https://example.invalid\n",
    );
    let apps = fixture.discover().unwrap();
    assert_eq!(
        apps.iter()
            .map(|app| (app.id.as_str(), app.name.as_str(), app.kind))
            .collect::<Vec<_>>(),
        [
            ("Edited", "User override", AppKind::Productivity),
            ("System", "System copy", AppKind::Communication),
        ]
    );
    fixture.entry(
        0,
        "System.desktop",
        "[Desktop Entry]\nType=Application\nName=\n",
    );
    assert!(
        fixture.discover().is_err(),
        "invalid overrides must not fall back to a different application"
    );
}

#[test]
fn nested_desktop_ids_obey_directory_priority_and_exact_filename_precedence() {
    let fixture = Fixture::new();
    fixture.pins("applications:vendor-Editor.desktop");
    fixture.entry(
        0,
        "vendor/Editor.desktop",
        &application("User nested entry", "Development;"),
    );
    fixture.entry(
        1,
        "vendor-Editor.desktop",
        &application("Lower exact entry", "Email;"),
    );
    let apps = fixture.discover().unwrap();
    assert_eq!(
        (apps[0].id.as_str(), apps[0].name.as_str()),
        ("vendor-Editor", "User nested entry")
    );
    fixture.entry(
        0,
        "vendor-Editor.desktop",
        &application("User exact entry", "Notes;"),
    );
    assert_eq!(fixture.discover().unwrap()[0].name, "User exact entry");
}

#[test]
fn category_evidence_produces_opt_in_examples_not_capability_claims() {
    let fixture = Fixture::new();
    let cases = [
        (
            "mail",
            "Office;Network;Email;",
            AppKind::Communication,
            true,
        ),
        (
            "conversation",
            "Network;InstantMessaging;",
            AppKind::Communication,
            true,
        ),
        ("web", "Network;WebBrowser;", AppKind::Browser, false),
        (
            "development",
            "Development;IDE;",
            AppKind::Productivity,
            true,
        ),
        ("notes", "Office;Notes;", AppKind::Productivity, true),
        ("editor", "Utility;TextEditor;", AppKind::Productivity, true),
        ("settings", "Settings;", AppKind::Utility, false),
        ("password", "Utility;Security;", AppKind::Utility, false),
        ("authenticator", "Office;", AppKind::Utility, false),
        ("other", "Utility;", AppKind::Utility, false),
        ("codium", "WebBrowser;", AppKind::Browser, false),
        ("signal", "", AppKind::Communication, true),
        ("unrecognized", "", AppKind::Utility, false),
    ];
    fixture.pins(
        &cases
            .iter()
            .map(|(id, _, _, _)| format!("applications:{id}.desktop"))
            .collect::<Vec<_>>()
            .join(","),
    );
    for (id, categories, _, _) in cases {
        fixture.entry(0, &format!("{id}.desktop"), &application(id, categories));
    }
    let apps = fixture.discover().unwrap();
    assert_eq!(
        apps.iter()
            .map(|app| (app.id.as_str(), app.kind, app.suggested))
            .collect::<Vec<_>>(),
        cases.map(|(id, _, kind, suggested)| (id, kind, suggested))
    );
}

#[test]
fn suggestions_render_finite_fading_notifications_without_media_or_critical_emphasis() {
    let now = Instant::now();
    let mut colors = Vec::new();
    for kind in [
        AppKind::Communication,
        AppKind::Browser,
        AppKind::Productivity,
        AppKind::Utility,
    ] {
        let app = PinnedApp {
            id: "Exact.desktop".into(),
            name: "Human label".into(),
            kind,
            suggested: true,
            icon: None,
        };
        let mut config = Config::default();
        config.room.screens[0].id = 42;
        let center = config.room.screens[0].position;
        config.room.strip = vec![
            center,
            Point {
                x: center.x + 0.5,
                ..center
            },
        ];
        config.rules = vec![suggested_rule(&app, 42, config.room.width)];
        config.validate().unwrap();
        colors.push(config.rules[0].color);
        let mut normal = Engine::new(&config).unwrap();
        let mut critical = Engine::new(&config).unwrap();
        assert!(!normal.notify(&app.name, 1, now, now, None));
        assert!(!normal.notify("Exact", 1, now, now, None));
        assert!(normal.notify(&app.id, 1, now, now, None));
        assert!(critical.notify(&app.id, 2, now, now, None));
        let early = normal
            .render(now + Duration::from_millis(50), false)
            .to_vec();
        let peak = normal
            .render(now + Duration::from_millis(300), false)
            .to_vec();
        assert!(early[0].iter().sum::<u8>() < peak[0].iter().sum::<u8>());
        assert!(peak[0].iter().all(|channel| *channel < 100));
        assert_eq!(
            critical.render(now + Duration::from_millis(300), false),
            peak
        );
        normal.set_playing(std::slice::from_ref(&app.id));
        critical.set_playing(std::slice::from_ref(&app.id));
        let expired =
            now + Duration::from_secs_f32(config.rules[0].duration) + Duration::from_millis(1);
        assert!(
            normal
                .render(expired, false)
                .iter()
                .all(|pixel| *pixel == [0; 3])
        );
        assert!(
            critical
                .render(expired, false)
                .iter()
                .all(|pixel| *pixel == [0; 3])
        );
        assert!(!normal.is_active());
        assert!(!critical.is_active());
        for width in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -1.0,
            0.0,
            f32::MAX,
        ] {
            config.rules[0] = suggested_rule(&app, 42, width);
            config.validate().unwrap();
        }
    }
    colors.sort_unstable();
    colors.dedup();
    assert_eq!(
        colors.len(),
        4,
        "categories must remain visually distinguishable"
    );
}

#[test]
fn malformed_launchers_cannot_escape_or_invent_routing_ids() {
    for launcher in [
        "applications:../secret.desktop",
        "applications:/tmp/secret.desktop",
        "applications:..%2fsecret.desktop",
        "applications:..%5csecret.desktop",
        "applications:%00secret.desktop",
        "applications:%0asecret.desktop",
        "applications:%C2%85secret.desktop",
        "applications:%ff.desktop",
        "applications:bad%2.desktop",
        "applications:bad%xx.desktop",
        "applications:query.desktop?exec=1",
        "applications:fragment.desktop#action",
        "applications:*.desktop",
        "applications:.desktop",
        "applications:...desktop",
        "applications:unsuffixed",
        r"applications:bad\q.desktop",
        r"applications:bad\x0z.desktop",
        r"applications:bad\n.desktop",
        r"applications:bad\\",
    ] {
        assert!(
            parse_launchers(&taskmanager(launcher)).is_err(),
            "accepted hostile/malformed launcher {launcher:?}"
        );
    }
    assert!(
        parse_launchers(&taskmanager(&format!(
            "applications:{}.desktop",
            "x".repeat(129)
        )))
        .is_err()
    );
    assert!(
        parse_launchers(&taskmanager(&format!(
            "applications:{}.desktop",
            "é".repeat(64)
        )))
        .is_err()
    );
    let malformed_group = format!(
        "{}[unterminated\nlaunchers=applications:Spoof.desktop\n",
        taskmanager("applications:Valid.desktop")
    );
    assert!(parse_launchers(&malformed_group).is_err());
    assert!(parse_launchers(&taskmanager("applications:control\0.desktop")).is_err());
}

#[test]
fn metadata_and_unique_pin_limits_fail_instead_of_returning_partial_snapshots() {
    let fixture = Fixture::new();
    let pins = (0..64)
        .map(|index| format!("applications:app-{index}.desktop"))
        .collect::<Vec<_>>()
        .join(",");
    let mut input = taskmanager(&pins);
    assert_eq!(parse_launchers(&input).unwrap().len(), 64);
    assert_eq!(
        parse_launchers(&taskmanager(&format!("{pins},{pins}")))
            .unwrap()
            .len(),
        64
    );
    assert!(
        parse_launchers(&taskmanager(&format!(
            "{pins},applications:overflow.desktop"
        )))
        .is_err()
    );
    input.push('#');
    input.extend(std::iter::repeat_n('x', 1024 * 1024 - input.len()));
    fs::write(&fixture.applets, &input).unwrap();
    assert!(fixture.discover().unwrap().is_empty());
    input.push('x');
    assert!(parse_launchers(&input).is_err());
    fs::write(&fixture.applets, &input).unwrap();
    assert!(fixture.discover().is_err());
    fixture.pins("applications:Bounded.desktop");
    let mut entry = application("Bounded", "Email;");
    entry.push('#');
    entry.extend(std::iter::repeat_n('x', 64 * 1024 - entry.len()));
    fixture.entry(0, "Bounded.desktop", &entry);
    assert_eq!(fixture.discover().unwrap()[0].name, "Bounded");
    entry.push('x');
    fixture.entry(0, "Bounded.desktop", &entry);
    assert!(fixture.discover().is_err());
}

#[test]
fn malformed_desktop_metadata_is_not_replaced_by_names_from_commands_or_actions() {
    let fixture = Fixture::new();
    fixture.pins("applications:Broken.desktop");
    for entry in [
        "[Desktop Entry]\nType=Application\nExec=plausible-name\n[Desktop Action New]\nName=Action only\n".to_owned(),
        application(r"Control\nName", "Email;"),
        application(&"x".repeat(129), "Notes;"),
        "[Desktop Entry]\nType=Application\nName=First\nName=Second\n".into(),
        "[Desktop Entry]\nType=Application\nName=App\nHidden=maybe\n".into(),
        "[Desktop Entry]\nType=Application\nName=First\n[Desktop Entry]\nType=Application\nName=Second\n".into(),
    ] {
        fixture.entry(0, "Broken.desktop", &entry);
        assert!(fixture.discover().is_err());
    }
    fs::write(fixture.directories[0].join("Broken.desktop"), [0xff]).unwrap();
    assert!(fixture.discover().is_err());
    fs::write(&fixture.applets, [0xff]).unwrap();
    assert!(fixture.discover().is_err());
}

#[test]
fn hostile_file_uris_cannot_read_arbitrary_existing_files_and_exec_is_never_run() {
    let fixture = Fixture::new();
    let private = fixture.root.path().join("private.desktop");
    // Reading this as desktop metadata would fail; it is not an application input.
    fs::write(&private, [0xff, 0x00]).unwrap();
    let marker = fixture.root.path().join("command-ran");
    fixture.pins(&format!(
        "{},https://example.invalid/app.desktop,preferred://browser,applications:Safe.desktop",
        reqwest::Url::from_file_path(&private).unwrap()
    ));
    fixture.entry(0, "Safe.desktop", &format!(
        "[Desktop Entry]\nType=Application\nName=Safe application\nCategories=Notes;\nExec=sh -c 'touch {}'\nTryExec=/missing/command\nDBusActivatable=true\n",
        marker.display()
    ));
    // An unpinned, malformed desktop entry must not be read either.
    fs::write(fixture.directories[0].join("Unpinned.desktop"), [0xff]).unwrap();
    let apps = fixture.discover().unwrap();
    assert_eq!(
        apps.iter().map(|app| app.name.as_str()).collect::<Vec<_>>(),
        ["Safe application"]
    );
    assert!(!marker.exists());
    fixture.pins("applications:../private.desktop");
    assert!(fixture.discover().is_err());
    fixture.pins(&format!("applications:{}", private.display()));
    assert!(fixture.discover().is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_supply_metadata_from_outside_the_application_directory() {
    use std::{os::unix::fs::symlink, path::Path};

    let fixture = Fixture::new();
    let private = fixture.root.path().join("private.desktop");
    fs::write(
        &private,
        application("Private file, not an installed app", "Email;"),
    )
    .unwrap();
    symlink(&private, fixture.directories[0].join("Escape.desktop")).unwrap();
    fixture.entry(1, "Escape.desktop", &application("Lower copy", "Email;"));
    fixture.pins("applications:Escape.desktop");
    assert!(
        fixture.discover().is_err(),
        "escaping entry must neither read private metadata nor reveal a lower copy"
    );
    symlink(fixture.root.path(), fixture.directories[0].join("outside")).unwrap();
    fixture.pins("applications:outside-private.desktop");
    assert!(fixture.discover().is_err());
    fixture.entry(
        0,
        "Real.desktop",
        &application("Safe local target", "Notes;"),
    );
    symlink(
        Path::new("Real.desktop"),
        fixture.directories[0].join("Alias.desktop"),
    )
    .unwrap();
    fixture.pins("applications:Alias.desktop");
    let apps = fixture.discover().unwrap();
    assert_eq!(
        (apps[0].id.as_str(), apps[0].name.as_str()),
        ("Alias", "Safe local target")
    );
}

#[test]
fn declared_shortcut_icons_follow_desktop_entry_priority_and_seed_source_colors() {
    let fixture = Fixture::new();
    let user_icon = fixture.root.path().join("user icon.svg");
    let system_icon = fixture.root.path().join("system.svg");
    // Desktop-entry strings escape backslashes, including Windows fixture paths.
    let user_icon_field = user_icon.to_string_lossy().replace('\\', "\\\\");
    let system_icon_field = system_icon.to_string_lossy().replace('\\', "\\\\");
    for (path, color) in [(&user_icon, "#20b050"), (&system_icon, "#ff0000")] {
        fs::write(path, format!("<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16'><rect width='16' height='16' fill='{color}'/></svg>")).unwrap();
    }
    fixture.entry(
        1,
        "Mail.desktop",
        &format!(
            "{}Icon={}\n",
            application("System mail", "Email;"),
            system_icon_field
        ),
    );
    fixture.entry(
        0,
        "Mail.desktop",
        &format!(
            "{}Icon={}\n[Desktop Action Wrong]\nIcon={}\n",
            application("My mail", "Email;"),
            user_icon_field,
            system_icon_field
        ),
    );
    fixture.pins("applications:Mail.desktop");
    let apps = fixture.discover().unwrap();
    let icon = apps[0].icon.as_ref().unwrap();
    assert_eq!(icon.dominant, [32, 176, 80]);
    assert_eq!(suggested_rule(&apps[0], 1, 3.0).color, [32, 176, 80]);
    let routed = application_at("Mail", &fixture.directories)
        .unwrap()
        .unwrap();
    assert_eq!(routed.icon.as_ref().unwrap().dominant, icon.dominant);
    // The selected shortcut remains authoritative even when its icon is gone.
    fs::remove_file(user_icon).unwrap();
    let apps = fixture.discover().unwrap();
    assert_eq!(apps[0].name, "My mail");
    assert!(apps[0].icon.is_none());
}

#[test]
fn arbitrary_routing_lookup_preserves_hidden_masks_nested_priority_and_exact_suffixes() {
    let fixture = Fixture::new();
    fixture.entry(1, "Mail.desktop", &application("System mail", "Email;"));
    fixture.entry(0, "Mail.desktop", "[Desktop Entry]\nHidden=true\n");
    assert!(
        application_at("Mail", &fixture.directories)
            .unwrap()
            .is_none()
    );
    fixture.entry(
        0,
        "vendor/Editor.desktop",
        &application("Nested editor", "Notes;"),
    );
    fixture.entry(
        1,
        "vendor-Editor.desktop",
        &application("Lower editor", "Notes;"),
    );
    assert_eq!(
        application_at("vendor-Editor", &fixture.directories)
            .unwrap()
            .unwrap()
            .name,
        "Nested editor"
    );
    fixture.entry(
        0,
        "Exact.desktop.desktop",
        &application("Suffix retained", "Email;"),
    );
    fixture.entry(
        0,
        "Exact.desktop",
        &application("Different routing ID", "Email;"),
    );
    let app = application_at("Exact.desktop", &fixture.directories)
        .unwrap()
        .unwrap();
    assert_eq!(
        (app.id.as_str(), app.name.as_str()),
        ("Exact.desktop", "Suffix retained")
    );
    for id in ["applications:Exact", "../Exact", "file:///tmp/private", "*"] {
        assert!(application_at(id, &fixture.directories).is_err());
    }
    assert!(
        application_at("Missing", &fixture.directories)
            .unwrap()
            .is_none()
    );
}

#[test]
fn missing_or_invalid_declared_icons_do_not_turn_into_category_artwork() {
    let fixture = Fixture::new();
    fixture.pins("applications:Mail.desktop");
    for icon in [
        "https://example.invalid/mail.png",
        "file:///tmp/mail.png",
        "../mail.png",
        "missing-ledalert-fixture-icon-no-generic-fallback",
        r"bad\qescape",
    ] {
        fixture.entry(
            0,
            "Mail.desktop",
            &format!("{}Icon={icon}\n", application("Mail", "Email;")),
        );
        let apps = fixture.discover().unwrap();
        assert_eq!(apps[0].name, "Mail");
        assert!(apps[0].icon.is_none(), "{icon}");
    }
}
