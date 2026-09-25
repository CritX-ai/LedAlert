//! Read-only inventory/observation and opt-in synthetic Windows listener acceptance.
//! See docs/windows-verification.md. This example never constructs a WLED transport.

#[cfg(not(windows))]
fn main() {
    eprintln!("windows_probe requires Windows; portable fixture tests remain available");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [] => native::gui(),
        [command] if command == "inventory" => native::inventory(),
        [command, seconds] if command == "observe" => seconds
            .parse::<u64>()
            .ok()
            .filter(|seconds| (1..=300).contains(seconds))
            .ok_or_else(|| anyhow::anyhow!("observe requires 1..300 seconds"))
            .and_then(native::observe),
        _ => Err(anyhow::anyhow!(
            "usage: windows_probe [inventory | observe SECONDS]; no arguments opens packaged consent/lifecycle GUI"
        )),
    };
    if let Err(error) = result {
        // All errors crossing this boundary are fixed diagnostic labels, not OS payloads.
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod native {
    #![allow(unsafe_code)]

    use anyhow::{Result, anyhow, ensure};
    use eframe::egui;
    use ledalert::desktop::{DesktopMonitor, NotificationEvent, NotificationPermission};
    use serde_json::{Value, json};
    use std::{
        path::PathBuf,
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
    use windows::{
        ApplicationModel::Package,
        Data::Xml::Dom::XmlDocument,
        UI::Notifications::{ToastNotification, ToastNotificationManager},
        Win32::System::WinRT::{
            RO_INIT_MULTITHREADED, RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
        },
        core::HSTRING,
    };

    const READY: &str = "Monitoring Windows notifications (metadata only)";
    const WAIT: Duration = Duration::from_secs(15);

    pub fn inventory() -> Result<()> {
        let (send, receive) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let pins = ledalert::taskbar::discover();
            let displays = ledalert::displays::discover();
            let result = match (pins, displays) {
                (Ok(pins), Ok(displays)) => Ok(json!({
                    "result": "PASS", "scope": "native inventory only",
                    "pins": pins.iter().enumerate().map(|(index, pin)| json!({
                        "index": index, "kind": format!("{:?}", pin.kind),
                        "icon": pin.icon.as_ref().map(|icon| json!({
                            "width": icon.width, "height": icon.height
                        }))
                    })).collect::<Vec<_>>(),
                    "displays": displays.iter().enumerate().map(|(index, display)| json!({
                        "index": index, "x": display.x, "y": display.y,
                        "width": display.width, "height": display.height,
                        "primary": display.primary
                    })).collect::<Vec<_>>()
                })),
                (Err(_), _) => Err(anyhow!("taskbar discovery failed (OS details withheld)")),
                (_, Err(_)) => Err(anyhow!("display discovery failed (OS details withheld)")),
            };
            let _ = send.send(result);
        });
        let report = receive
            .recv_timeout(Duration::from_secs(30))
            .map_err(|_| anyhow!("inventory exceeded 30 seconds or worker failed"))??;
        println!("{report}");
        Ok(())
    }

    pub fn observe(seconds: u64) -> Result<()> {
        let monitor = start()?;
        let began = Instant::now();
        let mut raised = 0;
        let mut closed = 0;
        while began.elapsed() < Duration::from_secs(seconds) {
            while let Some(event) = monitor.try_notification() {
                match event {
                    NotificationEvent::Raised(_) => raised += 1,
                    NotificationEvent::Closed { .. } => closed += 1,
                }
            }
            let state = monitor.snapshot();
            println!(
                "{}",
                json!({
                    "elapsed_ms": began.elapsed().as_millis(), "locked": state.locked,
                    "playing_count": state.playing.len(), "raised_count": raised,
                    "closed_count": closed, "generation": state.notification_generation,
                    "notification_status": state.notification_status,
                    "media_status": state.media_status, "lock_status": state.lock_status
                })
            );
            thread::sleep(Duration::from_millis(250));
        }
        println!("{}", json!({"result": "OBSERVED", "seconds": seconds}));
        Ok(())
    }

    fn start() -> Result<DesktopMonitor> {
        DesktopMonitor::start().map_err(|_| anyhow!("desktop worker failed to start"))
    }

    fn until(mut condition: impl FnMut() -> Result<bool>) -> Result<()> {
        let began = Instant::now();
        loop {
            if condition()? {
                return Ok(());
            }
            ensure!(began.elapsed() < WAIT, "stage timed out after 15 seconds");
            thread::sleep(Duration::from_millis(100));
        }
    }

    struct Toasts {
        group: HSTRING,
    }

    impl Toasts {
        fn show(&self, tag: &str) -> Result<()> {
            let tag = HSTRING::from(tag);
            let xml = XmlDocument::new()?;
            xml.LoadXml(&HSTRING::from(
                "<toast><visual><binding template=\"ToastGeneric\"><text>LedAlert verification</text><text>Synthetic fixture; no personal content.</text></binding></visual></toast>",
            ))?;
            let toast = ToastNotification::CreateToastNotification(&xml)?;
            toast.SetTag(&tag)?;
            toast.SetGroup(&self.group)?;
            ToastNotificationManager::CreateToastNotifier()?.Show(&toast)?;
            // Showing a toast is not proof it reached notification history.
            until(|| {
                let history = ToastNotificationManager::History()?.GetHistory()?;
                for index in 0..history.Size()? {
                    let item = history.GetAt(index)?;
                    if item.Group()? == self.group && item.Tag()? == tag {
                        return Ok(true);
                    }
                }
                Ok(false)
            })
        }

        fn remove(&self, tag: &str) -> Result<()> {
            ToastNotificationManager::History()?
                .RemoveGroupedTag(&HSTRING::from(tag), &self.group)?;
            Ok(())
        }
    }

    impl Drop for Toasts {
        fn drop(&mut self) {
            if let Ok(history) = ToastNotificationManager::History() {
                let _ = history.RemoveGroup(&self.group);
            }
        }
    }

    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            // SAFETY: created only after successful initialization on this worker;
            // never transferred to another thread.
            unsafe { RoUninitialize() };
        }
    }

    fn baseline(monitor: &DesktopMonitor, app_id: &str) -> Result<()> {
        until(|| Ok(monitor.snapshot().notification_status == READY))?;
        let began = Instant::now();
        while began.elapsed() < Duration::from_secs(2) {
            while let Some(event) = monitor.try_notification() {
                if let NotificationEvent::Raised(event) = event {
                    ensure!(
                        event.application != app_id,
                        "baseline replayed an existing toast"
                    );
                }
            }
            ensure!(
                monitor.snapshot().notification_status == READY,
                "listener lost access"
            );
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    fn raised(monitor: &DesktopMonitor, app_id: &str) -> Result<(u64, u32)> {
        let mut found = None;
        until(|| {
            ensure!(
                monitor.snapshot().notification_status == READY,
                "listener lost access"
            );
            while let Some(event) = monitor.try_notification() {
                if let NotificationEvent::Raised(event) = event
                    && event.application == app_id
                {
                    ensure!(found.is_none(), "duplicate synthetic add");
                    found = Some((event.generation, event.id));
                }
            }
            Ok(found.is_some())
        })?;
        found.ok_or_else(|| anyhow!("synthetic add missing"))
    }

    fn closed(monitor: &DesktopMonitor, expected: (u64, u32)) -> Result<()> {
        until(|| {
            ensure!(
                monitor.snapshot().notification_status == READY,
                "listener lost access"
            );
            while let Some(event) = monitor.try_notification() {
                if let NotificationEvent::Closed { generation, id, .. } = event
                    && (generation, id) == expected
                {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }

    fn lifecycle(stages: &mpsc::Sender<Value>) -> Result<()> {
        // SAFETY: balanced by the thread-local guard before returning, including errors.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }?;
        let _apartment = Apartment;
        let family = Package::Current()?.Id()?.FamilyName()?;
        let app_id = format!("{family}!Probe");
        let toasts = Toasts {
            group: HSTRING::from(format!("la{}", std::process::id())),
        };
        let stage = |name: &str| {
            let _ = stages.send(json!({"stage": name, "result": "PASS"}));
        };
        toasts.show("baseline")?;
        let monitor = start()?;
        baseline(&monitor, &app_id)?;
        stage("initial baseline without replay");
        toasts.show("added")?;
        let event = raised(&monitor, &app_id)?;
        stage("new toast raised");
        toasts.remove("added")?;
        closed(&monitor, event)?;
        stage("removed toast closed");
        toasts.show("restart")?;
        let _ = raised(&monitor, &app_id)?;
        drop(monitor);
        let monitor = start()?;
        baseline(&monitor, &app_id)?;
        stage("worker restart without replay");
        toasts.show("after-restart")?;
        let event = raised(&monitor, &app_id)?;
        toasts.remove("after-restart")?;
        closed(&monitor, event)?;
        stage("new add and remove after restart");
        ToastNotificationManager::History()?.RemoveGroup(&toasts.group)?;
        stage("synthetic toast cleanup");
        Ok(())
    }

    pub fn gui() -> Result<()> {
        // SAFETY: this guard lives on the foreground GUI thread until run_native returns.
        unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }
            .map_err(|_| anyhow!("GUI COM initialization failed"))?;
        let _apartment = Apartment;
        let package = Package::Current().map_err(|_| {
            anyhow!("GUI lifecycle requires explicit verification package registration; see docs")
        })?;
        ensure!(
            package
                .Id()
                .and_then(|id| id.Name())
                .ok()
                .is_some_and(|name| name == "LedAlert.Verification"),
            "unexpected package identity"
        );
        let directory = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| anyhow!("LOCALAPPDATA unavailable"))?
            .join("LedAlertVerification");
        ensure!(
            directory.join("owned-by-windows-verify").is_file(),
            "verification ownership marker missing"
        );
        let monitor = start()?;
        let result = eframe::run_native(
            "LedAlert isolated Windows verification (no WLED)",
            eframe::NativeOptions::default(),
            Box::new(move |_| {
                Ok(Box::new(Probe {
                    monitor: Some(monitor),
                    permission: NotificationPermission::default(),
                    receive: None,
                    reports: Vec::new(),
                    began: Instant::now(),
                    complete: false,
                    directory,
                }))
            }),
        );
        result.map_err(|_| anyhow!("verification GUI failed"))
    }

    struct Probe {
        monitor: Option<DesktopMonitor>,
        permission: NotificationPermission,
        receive: Option<mpsc::Receiver<Value>>,
        reports: Vec<Value>,
        began: Instant,
        complete: bool,
        directory: PathBuf,
    }

    impl Probe {
        fn finish(&mut self, result: &str) {
            self.complete = true;
            self.reports
                .push(json!({"result": result, "scope": "synthetic listener lifecycle only"}));
            let report = json!({"schema": 1, "result": result, "stages": self.reports});
            if std::fs::write(self.directory.join("report.json"), report.to_string()).is_err() {
                self.reports
                    .push(json!({"result": "FAIL", "stage": "report write failed"}));
            }
        }
    }

    impl eframe::App for Probe {
        fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
            let context = ui.ctx().clone();
            while !self.complete {
                let Some(report) = self
                    .receive
                    .as_ref()
                    .and_then(|receive| receive.try_recv().ok())
                else {
                    break;
                };
                if let Some(result) = report.get("final").and_then(Value::as_str) {
                    self.finish(result);
                } else {
                    self.reports.push(report);
                }
            }
            if !self.complete && self.began.elapsed() > Duration::from_secs(180) {
                self.finish("FAIL: overall 180-second deadline");
            }
            egui::CentralPanel::default().show(ui, |ui| {
                ui.heading("Isolated Windows verification");
                ui.label("No WLED output. Only synthetic fixture toasts are created/removed.");
                if let Some(monitor) = &self.monitor {
                    let state = monitor.snapshot();
                    ui.label(&state.notification_status);
                    if ui.button("1. Request Windows notification access").clicked() {
                        self.permission.request_access(monitor);
                    }
                    if ui.add_enabled(state.notification_status == READY && !self.complete, egui::Button::new("2. Run synthetic lifecycle")).clicked() {
                        drop(self.monitor.take());
                        let (send, receive) = mpsc::channel();
                        self.receive = Some(receive);
                        thread::spawn(move || {
                            let result = if lifecycle(&send).is_ok() { "PASS" } else { "FAIL: lifecycle stage failed or timed out (OS details withheld)" };
                            let _ = send.send(json!({"final": result}));
                        });
                    }
                }
                for report in &self.reports {
                    ui.monospace(report.to_string());
                }
                ui.label("Close after the final result, then run the helper's Report and Cleanup actions.");
            });
            context.request_repaint_after(Duration::from_millis(100));
        }
    }
}
