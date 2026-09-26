//! Child processes exercise the actual CLI without mutating the test runner's environment.
use ledalert::config::Config;
use std::process::Command;

fn isolated() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ledalert"));
    command
        .env_remove("APPDATA")
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME");
    command
}

#[test]
fn default_storage_uses_the_native_user_directory_with_unicode_paths() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().join("通知 settings");
    #[cfg(windows)]
    let (variable, path) = ("APPDATA", base.join("LedAlert/config.json"));
    #[cfg(target_os = "macos")]
    let (variable, path) = (
        "HOME",
        base.join("Library/Application Support/LedAlert/config.json"),
    );
    #[cfg(not(any(windows, target_os = "macos")))]
    let (variable, path) = ("XDG_CONFIG_HOME", base.join("ledalert/config.json"));
    Config::default().save(&path).unwrap();
    let mut command = isolated();
    command.env(variable, &base);
    #[cfg(target_os = "macos")]
    command.env("XDG_CONFIG_HOME", root.path().join("not-macos-storage"));
    let result = command.arg("check-config").output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    // Resolving a relative environment path must not silently select a file
    // relative to whichever directory the app happened to be launched from.
    let result = isolated()
        .current_dir(root.path())
        .env(variable, "通知 settings")
        .arg("check-config")
        .output()
        .unwrap();
    assert!(!result.status.success());
}

#[test]
fn explicit_configuration_works_without_any_desktop_environment_paths() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("chosen.json");
    Config::default().save(&path).unwrap();
    let result = isolated()
        .arg("--config")
        .arg(&path)
        .arg("check-config")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = isolated().arg("check-config").output().unwrap();
    assert!(!result.status.success());
}
