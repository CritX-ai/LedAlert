//! Opt-in native metadata smoke. No permissions, notifications or lighting are activated.
#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use ledalert::{
        desktop::{DesktopMonitor, NotificationEvent},
        displays, taskbar,
    };
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_none_or(|value| value == "inventory") {
        anyhow::ensure!(args.len() <= 1, "usage: macos_probe inventory");
        let apps = taskbar::discover()?;
        let screens = displays::discover()?;
        let calculator = taskbar::application("com.apple.calculator")?;
        println!(
            "{}",
            serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"), "pin_count": apps.len(),
                "decoded_icons": apps.iter().filter(|app| app.icon.is_some()).count(),
                "exact_bundle_lookup": calculator.is_some(),
                "displays": screens.iter().map(|screen| serde_json::json!({
                    "x": screen.x, "y": screen.y, "width": screen.width, "height": screen.height,
                    "primary": screen.primary,
                })).collect::<Vec<_>>()
            })
        );
        return Ok(());
    }
    let (seconds, expected) = match args.as_slice() {
        [command, seconds] if command == "observe" => (seconds, None),
        [command, application, seconds] if command == "audio-check" => (seconds, Some(application)),
        _ => anyhow::bail!(
            "usage: macos_probe inventory | observe SECONDS | audio-check BUNDLE_ID SECONDS"
        ),
    };
    let seconds: u64 = seconds.parse()?;
    anyhow::ensure!(
        (1..=30).contains(&seconds),
        "observation must be 1–30 seconds"
    );
    let monitor = DesktopMonitor::start()?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let (mut raised, mut closed, mut matched) = (0, 0, false);
    while Instant::now() < deadline {
        while let Some(event) = monitor.try_notification() {
            match event {
                NotificationEvent::Raised(_) => raised += 1,
                NotificationEvent::Closed { .. } => closed += 1,
            }
        }
        if let Some(expected) = expected {
            matched |= monitor.snapshot().playing.contains(expected);
        }
        thread::sleep(Duration::from_millis(100));
    }
    let state = monitor.snapshot();
    println!(
        "{}",
        serde_json::json!({
            "notification_status": state.notification_status, "notification_generation": state.notification_generation,
            "media_status": state.media_status, "lock_status": state.lock_status,
            "locked": state.locked, "playing_app_count": state.playing.len(),
            "raises": raised, "closes": closed,
            "expected_audio_app_active": expected.map(|_| matched),
        })
    );
    anyhow::ensure!(
        expected.is_none() || matched,
        "expected application's audio activity was not observed"
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("macos_probe requires an interactive macOS session");
    std::process::exit(2);
}
