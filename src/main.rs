use anyhow::{Context, Result, bail};
use ledalert::{
    app::LedAlertApp,
    config::{Config, default_path},
    wled,
};
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("LedAlert: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut path = None;
    let mut command = "gui";
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                println!(
                    "LedAlert — spatial desktop notifications for WLED\n\nUsage: ledalert [--config PATH] [gui|check-config|probe]\n\n  gui           Open the native room workbench (default); lighting starts disabled\n  check-config  Validate an existing configuration without desktop or device access\n  probe         Read WLED capabilities; does not send pixels or change settings\n  --config PATH Use a specific configuration file\n  --version     Print the application version\n\nConfiguration: Windows: %APPDATA%\\LedAlert\\config.json\n               Linux: $XDG_CONFIG_HOME/ledalert/config.json, or ~/.config/ledalert/config.json\nNo daemon, autostart or background installation is performed."
                );
                return Ok(());
            }
            "--version" | "-V" => {
                println!("LedAlert {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--config" => {
                path = Some(PathBuf::from(
                    arguments.next().context("--config requires a file path")?,
                ));
            }
            "gui" | "probe" | "check-config" => {
                if command != "gui" {
                    bail!("Choose one command");
                }
                command = match argument.as_str() {
                    "probe" => "probe",
                    "check-config" => "check-config",
                    _ => "gui",
                };
            }
            _ => bail!("Unknown argument {argument:?}; use --help"),
        }
    }
    let path = path.map_or_else(default_path, Ok)?;
    match command {
        "check-config" => {
            Config::load(&path).with_context(|| format!("Cannot validate {}", path.display()))?;
            println!("Configuration valid");
        }
        "probe" => {
            let config = if path.exists() {
                Config::load(&path)?
            } else {
                Config::default()
            };
            let info = wled::probe(config.device)?;
            println!(
                "WLED {} · {} RGB LEDs · realtime {}",
                info.version,
                info.led_count,
                if info.live { "active" } else { "idle" }
            );
        }
        _ => {
            let options = eframe::NativeOptions {
                viewport: eframe::egui::ViewportBuilder::default()
                    .with_inner_size([1240.0, 820.0])
                    .with_min_inner_size([960.0, 640.0])
                    .with_icon(ledalert::identity::icon()?)
                    .with_app_id("io.github.critx.LedAlert"),
                renderer: eframe::Renderer::Glow,
                ..Default::default()
            };
            eframe::run_native(
                "LedAlert",
                options,
                Box::new(move |cc| Ok(Box::new(LedAlertApp::new(&cc.egui_ctx, path)?))),
            )
            .map_err(|error| anyhow::anyhow!("Cannot open native window: {error}"))?;
        }
    }
    Ok(())
}
