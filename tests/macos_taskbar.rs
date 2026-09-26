use ledalert::taskbar::macos::{local_path, parse_dock_pins, parse_sidebar_pins};
use serde_json::{Value, json};

fn sidebar_bytes(records: Value) -> Vec<u8> {
    serde_json::to_vec(&records).unwrap()
}

fn sidebar_app(id: &str, url: &str) -> Value {
    // `name` is deliberately present: record display names must never become
    // discovery inputs; labels come from the installed bundle instead.
    json!({
        "id": id,
        "type": 0,
        "keepInDock": true,
        "bundleURL": url,
        "name": "Record Label That Must Be Ignored",
    })
}

fn file_tile(url: &str, identifier: Option<&str>) -> Value {
    let mut tile = json!({
        "tile-type": "file-tile",
        "tile-data": { "file-data": { "_CFURLString": url } },
    });
    if let Some(identifier) = identifier {
        tile["tile-data"]["bundle-identifier"] = json!(identifier);
    }
    tile
}

fn dock_bytes(tiles: Value) -> Vec<u8> {
    serde_json::to_vec(&tiles).unwrap()
}

fn paths(candidates: &[ledalert::taskbar::macos::PinCandidate]) -> Vec<String> {
    candidates
        .iter()
        .map(|candidate| candidate.path.clone())
        .collect()
}

#[test]
fn sidebar_pins_preserve_configured_order_and_dedup_identities() {
    let candidates = parse_sidebar_pins(&sidebar_bytes(json!([
        sidebar_app("com.example.alpha", "file:///Applications/Alpha.app"),
        // Explicit main-screen and per-screen pin flags count like keepInDock.
        json!({
            "id": "com.example.beta",
            "type": 0,
            "keepInDock": false,
            "pinnedToMainScreen": true,
            "bundleURL": "file:///Applications/Beta.app",
        }),
        json!({
            "id": "com.example.gamma",
            "type": 0,
            "pinnedScreens": [0, 1],
            "bundleURL": "file:///Applications/Gamma.app",
        }),
        // Non-application records and duplicates are not pins.
        json!({
            "id": "com.example.widget",
            "type": 1,
            "keepInDock": true,
            "bundleURL": "file:///Applications/Widget.app",
        }),
        sidebar_app("com.example.alpha", "file:///Applications/Alpha.app"),
    ])))
    .unwrap();
    let ids: Vec<_> = candidates
        .iter()
        .map(|candidate| candidate.id.clone().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["com.example.alpha", "com.example.beta", "com.example.gamma"]
    );
    assert_eq!(
        paths(&candidates),
        [
            "/Applications/Alpha.app",
            "/Applications/Beta.app",
            "/Applications/Gamma.app",
        ]
    );
}

#[test]
fn missing_or_false_pin_state_is_not_a_pin() {
    let candidates = parse_sidebar_pins(&sidebar_bytes(json!([
        json!({ "id": "com.example.off", "type": 0, "keepInDock": false, "bundleURL": "file:///A.app" }),
        json!({ "id": "com.example.absent", "type": 0, "bundleURL": "file:///B.app" }),
        json!({ "id": "com.example.empty", "type": 0, "pinnedScreens": [], "bundleURL": "file:///C.app" }),
        json!({
            "id": "com.example.both", "type": 0,
            "keepInDock": false, "pinnedToMainScreen": false, "pinnedScreens": [],
            "bundleURL": "file:///D.app",
        }),
    ])))
    .unwrap();
    assert!(candidates.is_empty());
}

#[test]
fn malformed_sidebar_sources_fail_instead_of_returning_partial_pins() {
    let cases = [
        json!([{ "id": "com.example.a", "keepInDock": true, "bundleURL": "file:///A.app" }]),
        json!([{ "id": "com.example.a", "type": "0", "keepInDock": true, "bundleURL": "file:///A.app" }]),
        json!([{ "id": "com.example.a", "type": 0, "keepInDock": "yes", "bundleURL": "file:///A.app" }]),
        json!([{ "id": "com.example.a", "type": 0, "pinnedToMainScreen": 1, "bundleURL": "file:///A.app" }]),
        json!([{ "id": "com.example.a", "type": 0, "pinnedScreens": {}, "bundleURL": "file:///A.app" }]),
        json!([{ "id": "com.example.a", "type": 0, "pinnedScreens": [["x"]], "bundleURL": "file:///A.app" }]),
        json!([{ "type": 0, "keepInDock": true, "bundleURL": "file:///A.app" }]),
        json!([{ "id": "com.example.a", "type": 0, "keepInDock": true }]),
        json!([{ "id": " ", "type": 0, "keepInDock": true, "bundleURL": "file:///A.app" }]),
        json!([{ "id": "com.exam\u{7}ple", "type": 0, "keepInDock": true, "bundleURL": "file:///A.app" }]),
        json!(["not an object"]),
        json!({ "not": "an array" }),
    ];
    for case in &cases {
        assert!(
            parse_sidebar_pins(&sidebar_bytes(case.clone())).is_err(),
            "expected malformed source to fail: {case}"
        );
    }
}

#[test]
fn non_local_and_escaped_bundle_urls_are_handled_explicitly() {
    for url in [
        "https://example.com/App.app",
        "ftp://example.com/App.app",
        "file://remote.host/App.app",
        "file://",
        "Applications/App.app",
        "file:///A%2",
        "file:///A%ZZ.app",
    ] {
        assert!(
            parse_sidebar_pins(&sidebar_bytes(json!([
                { "id": "com.example.a", "type": 0, "keepInDock": true, "bundleURL": url }
            ])))
            .is_err(),
            "expected non-local URL to fail: {url}"
        );
        assert!(
            local_path(url).is_err(),
            "expected non-local URL to fail: {url}"
        );
    }
    // Local URLs decode percent escapes; plain absolute paths are accepted as
    // the Dock's conventional plain-path form and never percent-decoded.
    assert_eq!(
        local_path("file:///Users/x/App%20One.app/").unwrap(),
        "/Users/x/App One.app/"
    );
    assert_eq!(
        local_path("/Applications/App One.app").unwrap(),
        "/Applications/App One.app"
    );
}

#[test]
fn dock_tiles_keep_only_file_tiles_and_declared_identities() {
    let candidates = parse_dock_pins(&dock_bytes(json!([
        { "tile-type": "spacer-tile" },
        { "tile-type": "flex-space-tile" },
        { "tile-type": "directory-tile", "tile-data": { "file-data": { "_CFURLString": "file:///Users/x/Folder/" } } },
        { "tile-type": "url-tile", "tile-data": { "URL": "https://example.com" } },
        { "tile-type": "future-tile" },
        file_tile("file:///Applications/Safari.app/", Some("com.apple.Safari")),
        file_tile("/Applications/Terminal.app", None),
        file_tile("file:///Applications/Safari.app/", Some("com.apple.Safari")),
    ])))
    .unwrap();
    assert_eq!(
        candidates,
        [
            ledalert::taskbar::macos::PinCandidate {
                id: Some("com.apple.Safari".into()),
                path: "/Applications/Safari.app/".into(),
            },
            ledalert::taskbar::macos::PinCandidate {
                id: None,
                path: "/Applications/Terminal.app".into()
            },
        ]
    );
}

#[test]
fn malformed_dock_tiles_fail_instead_of_inventing_pins() {
    let cases = [
        json!([{ "tile-data": {} }]),
        json!(["not an object"]),
        json!([{ "tile-type": "file-tile" }]),
        json!([{ "tile-type": "file-tile", "tile-data": "not an object" }]),
        json!([{ "tile-type": "file-tile", "tile-data": { "bundle-identifier": 7 } }]),
        json!([{ "tile-type": "file-tile", "tile-data": { "bundle-identifier": "bad\nid" } }]),
        json!([{ "tile-type": "file-tile", "tile-data": { "file-data": {} } }]),
        file_tile("https://example.com/App.app", None),
        file_tile("file://remote.host/App.app", None),
    ];
    for case in &cases {
        assert!(
            parse_dock_pins(&dock_bytes(case.clone())).is_err(),
            "expected malformed tile to fail: {case}"
        );
    }
}

#[test]
fn configuration_blobs_and_record_counts_are_bounded() {
    let oversized = vec![b' '; 1024 * 1024 + 1];
    assert!(parse_sidebar_pins(&oversized).is_err());
    assert!(parse_dock_pins(&oversized).is_err());
    // At most 64 pins from one snapshot.
    let records: Vec<Value> = (0..65)
        .map(|index| {
            sidebar_app(
                &format!("com.example.app{index}"),
                &format!("file:///Applications/App{index}.app"),
            )
        })
        .collect();
    assert!(parse_sidebar_pins(&sidebar_bytes(json!(records))).is_err());
}

#[test]
fn sidebar_manual_order_excludes_hidden_pins_and_keeps_ties_stable() {
    let mut alpha = sidebar_app("com.example.alpha", "file:///Applications/Alpha.app");
    let mut beta = sidebar_app("com.example.beta", "file:///Applications/Beta.app");
    let mut gamma = sidebar_app("com.example.gamma", "file:///Applications/Gamma.app");
    let mut hidden = sidebar_app("com.example.hidden", "file:///Applications/Hidden.app");
    alpha["manualSortOrder"] = json!(20);
    beta["manualSortOrder"] = json!(10);
    gamma["manualSortOrder"] = json!(10);
    hidden["manualSortOrder"] = json!(0);
    hidden["hiddenInDock"] = json!(true);
    let pins = parse_sidebar_pins(&sidebar_bytes(json!([alpha, beta, gamma, hidden]))).unwrap();
    assert_eq!(
        pins.iter()
            .map(|pin| pin.id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["com.example.beta", "com.example.gamma", "com.example.alpha"]
    );
}

#[test]
fn incomplete_sidebar_order_retains_source_positions() {
    let mut alpha = sidebar_app("com.example.alpha", "file:///Applications/Alpha.app");
    let beta = sidebar_app("com.example.beta", "file:///Applications/Beta.app");
    let mut gamma = sidebar_app("com.example.gamma", "file:///Applications/Gamma.app");
    alpha["manualSortOrder"] = json!(20);
    gamma["manualSortOrder"] = json!(0);
    let pins = parse_sidebar_pins(&sidebar_bytes(json!([alpha, beta, gamma]))).unwrap();
    assert_eq!(
        pins.iter()
            .map(|pin| pin.id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["com.example.alpha", "com.example.beta", "com.example.gamma"]
    );
}

#[test]
fn malformed_sidebar_visibility_and_order_fail_closed() {
    for field in ["hiddenInDock", "manualSortOrder"] {
        let mut app = sidebar_app("com.example.alpha", "file:///Applications/Alpha.app");
        app[field] = json!("invalid");
        assert!(parse_sidebar_pins(&sidebar_bytes(json!([app]))).is_err());
    }
}
