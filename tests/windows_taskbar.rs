use ledalert::taskbar::{
    AppKind, PinnedApp,
    windows::{canonical_app_id, classify_app, parse_taskband, resolve_taskband},
};

const EXPLORER: &str = "Microsoft.Windows.Explorer";
const TERMINAL: &str = "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App";
const SIGNAL: &str = "org.whispersystems.signal-desktop";

fn captured() -> Vec<u8> {
    let hex = include_str!("fixtures/windows-taskband-v3.hex")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<String>();
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn snapshot(pidls: &[&[u8]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for pidl in pidls {
        bytes.push(0);
        bytes.extend_from_slice(&(pidl.len() as u32).to_le_bytes());
        bytes.extend_from_slice(pidl);
    }
    bytes.push(0xff);
    bytes
}

fn metadata(id: &str, name: &str) -> PinnedApp {
    let kind = classify_app(id, name);
    PinnedApp {
        id: id.to_owned(),
        name: name.to_owned(),
        kind,
        suggested: matches!(kind, AppKind::Communication | AppKind::Productivity),
        icon: None,
    }
}

#[test]
fn captured_taskband_includes_packaged_pin_without_a_shortcut() {
    let bytes = captured();
    let pins = parse_taskband(&bytes).unwrap();
    assert_eq!(
        pins.iter()
            .map(|pin| pin.app_id.as_deref())
            .collect::<Vec<_>>(),
        [Some(EXPLORER), Some(TERMINAL), Some(SIGNAL)]
    );
    // Terminal is a namespace/packaged PIDL, not a file-system .lnk item.
    let shortcut = ".lnk"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    assert!(
        !pins[1]
            .pidl
            .windows(shortcut.len())
            .any(|bytes| bytes == shortcut)
    );
}

#[test]
fn stale_pins_do_not_hide_a_later_live_duplicate_and_order_is_preserved() {
    let captured = captured();
    let pins = parse_taskband(&captured).unwrap();
    let bytes = snapshot(&[
        pins[0].pidl,
        pins[1].pidl,
        pins[2].pidl,
        pins[0].pidl,
        pins[1].pidl,
    ]);
    let mut explorer_attempts = 0;
    let apps = resolve_taskband(&bytes, |pin| match pin.app_id.as_deref()? {
        EXPLORER => {
            explorer_attempts += 1;
            // First Shell record is stale; the later duplicate still resolves.
            (explorer_attempts > 1).then(|| metadata(EXPLORER, "File Explorer"))
        }
        TERMINAL => Some(metadata(TERMINAL, "Terminal")),
        // Uninstalled app: cached identity alone must not make a suggestion.
        SIGNAL => None,
        _ => panic!("Only real pinned records may be resolved"),
    })
    .unwrap();
    assert_eq!(
        apps.iter().map(|app| app.id.as_str()).collect::<Vec<_>>(),
        [TERMINAL, EXPLORER]
    );
}

#[test]
fn malformed_snapshot_never_resolves_a_partial_pin_inventory() {
    let bytes = captured();
    let mut truncated = bytes.clone();
    truncated.pop();
    let mut bad_marker = bytes.clone();
    bad_marker[0] = 1;
    let mut bad_size = bytes.clone();
    bad_size[1..5].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut invalid_item = bytes.clone();
    invalid_item[5..7].copy_from_slice(&u16::MAX.to_le_bytes());
    let mut trailing = bytes.clone();
    trailing.push(0);
    for malformed in [truncated, bad_marker, bad_size, invalid_item, trailing] {
        assert!(
            resolve_taskband(&malformed, |_| {
                panic!("Malformed complete snapshots must fail before resolution")
            })
            .is_err()
        );
    }
    assert!(parse_taskband(&[0xff]).unwrap().is_empty());
    assert!(parse_taskband(&[]).is_err());
}

#[test]
fn corrupt_identity_extensions_are_not_scraped_as_application_names() {
    let bytes = captured();
    let pins = parse_taskband(&bytes).unwrap();
    let original = pins[0].pidl;
    let signature = original
        .windows(4)
        .position(|word| word == [0x1d, 0, 0xef, 0xbe])
        .unwrap();
    let mut invalid_utf16 = original.to_vec();
    invalid_utf16[signature + 6..signature + 8].copy_from_slice(&0xd800u16.to_le_bytes());
    let mut unknown_version = original.to_vec();
    unknown_version[signature - 2] = 1;
    let mut truncated_extension = original.to_vec();
    truncated_extension[signature - 4..signature - 2].copy_from_slice(&u16::MAX.to_le_bytes());
    for pidl in [invalid_utf16, unknown_version, truncated_extension] {
        assert!(parse_taskband(&snapshot(&[&pidl])).is_err());
    }
    // Package records contain two identity extensions; disagreement is corrupt,
    // not an invitation to select whichever ID happened to be found first.
    let mut conflicting = pins[1].pidl.to_vec();
    let signatures = conflicting
        .windows(4)
        .enumerate()
        .filter_map(|(index, word)| (word == [0x1d, 0, 0xef, 0xbe]).then_some(index))
        .collect::<Vec<_>>();
    conflicting[signatures[1] + 6] = b'X';
    assert!(parse_taskband(&snapshot(&[&conflicting])).is_err());
}

#[test]
fn metadata_work_is_bounded_before_shell_resolution() {
    let bytes = captured();
    let pins = parse_taskband(&bytes).unwrap();
    let sixty_four = vec![pins[0].pidl; 64];
    assert_eq!(parse_taskband(&snapshot(&sixty_four)).unwrap().len(), 64);
    let sixty_five = vec![pins[0].pidl; 65];
    assert!(
        resolve_taskband(&snapshot(&sixty_five), |_| {
            panic!("Over-limit inventory must never reach native Shell")
        })
        .is_err()
    );
    assert!(parse_taskband(&vec![0; 1024 * 1024 + 1]).is_err());
    let mut oversized = vec![0];
    oversized.extend_from_slice(&(65_537u32).to_le_bytes());
    assert!(parse_taskband(&oversized).is_err());
}

#[test]
fn routing_identity_preserves_case_paths_and_full_unicode_without_truncation() {
    for id in [
        TERMINAL,
        SIGNAL,
        "Org.Vendor.Application.desktop",
        r"{6D809377-6AF0-444B-8957-A3773F02200E}\KeePassXC\KeePassXC.exe",
        r"C:\Users\example\Apps\Utility.exe",
    ] {
        assert_eq!(canonical_app_id(id), Some(id));
    }
    let wide = "界".repeat(128);
    assert_eq!(canonical_app_id(&wide), Some(wide.as_str()));
    assert!(canonical_app_id(&"界".repeat(129)).is_none());
    assert!(canonical_app_id(&"\u{10000}".repeat(65)).is_none());
    for id in ["", "*", " App", "App ", "App\0Extra", "App\nExtra"] {
        assert!(canonical_app_id(id).is_none());
    }
}

#[test]
fn observed_generated_win32_ids_can_still_offer_useful_examples() {
    let signal = metadata(SIGNAL, "Signal");
    assert_eq!(signal.kind, AppKind::Communication);
    assert!(signal.suggested);
    let mail = metadata("5C3291FF93E4E04E", "Betterbird");
    assert_eq!(mail.kind, AppKind::Communication);
    assert!(mail.suggested);
    let browser = metadata("22EB8429C9C8096C", "Floorp");
    assert_eq!(browser.kind, AppKind::Browser);
    assert!(!browser.suggested);
    assert_eq!(
        metadata("Unknown.Vendor", "Unknown Application").kind,
        AppKind::Utility
    );
}
