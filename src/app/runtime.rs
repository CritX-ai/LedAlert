use super::*;

pub(super) struct NotificationGate {
    paused: bool,
    after: Instant,
}

impl NotificationGate {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            paused: true,
            after: now,
        }
    }

    pub(super) fn discard_before(&mut self, now: Instant) {
        self.after = now;
    }

    fn update(&mut self, paused: bool, now: Instant) {
        // Include the gap after the last inhibited tick, not only ticks spent paused.
        if paused || self.paused {
            self.discard_before(now);
        }
        self.paused = paused;
    }

    fn accepts(&self, received: Instant) -> bool {
        !self.paused && received >= self.after
    }
}

impl LedAlertApp {
    pub fn new(ctx: &egui::Context, path: PathBuf) -> anyhow::Result<Self> {
        let state = Arc::new(Mutex::new(AppState::new(ctx, path)?));
        let worker_state = Arc::clone(&state);
        let ctx = ctx.clone();
        let (stop, stopped) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("ledalert-runtime".into())
            .spawn(move || {
                loop {
                    if !matches!(stopped.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                        break;
                    }
                    let delay = {
                        let mut state = worker_state
                            .lock()
                            .expect("Application state lock poisoned");
                        state.tick()
                    };
                    // Never acquire egui locks while holding the state mutex:
                    // eframe can already hold egui locks when it enters App::ui.
                    ctx.request_repaint();
                    // Wayland can withhold redraws indefinitely while minimized.
                    // Only this bounded clock drives the existing runtime producer.
                    if !matches!(
                        stopped.recv_timeout(delay),
                        Err(mpsc::RecvTimeoutError::Timeout)
                    ) {
                        break;
                    }
                }
            })?;
        Ok(Self {
            state,
            stop,
            worker: Some(worker),
            #[cfg(windows)]
            notification_permission: Default::default(),
        })
    }
}

impl Drop for LedAlertApp {
    fn drop(&mut self) {
        let _ = self.stop.try_send(());
        // Join without the state mutex, before its existing output owner shuts down.
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            eprintln!("Application runtime worker failed before shutdown");
        }
    }
}

impl AppState {
    pub(super) fn tick(&mut self) -> Duration {
        let now = Instant::now();
        let previous_generation = self.desktop_state.notification_generation;
        if now.duration_since(self.desktop_poll) >= Duration::from_millis(100) {
            self.desktop_state = self.desktop.snapshot();
            self.desktop_poll = now;
        }
        let generation = self.desktop.notification_generation();
        if generation != previous_generation {
            self.engine.clear();
            self.stop_demos();
            self.desktop_state.notification_generation = generation;
        }
        if let Some(receiver) = &self.display_scan {
            match receiver.try_recv() {
                Ok(result) => {
                    self.display_scan = None;
                    match result {
                        Ok(displays) => {
                            self.detected = displays;
                            if self.detected.is_empty() {
                                self.auto_import = false;
                                self.display_error = Some(
                                    "The desktop reported no active displays. Add one manually or Refresh."
                                        .into(),
                                );
                            } else if !self.auto_import {
                                let known: Vec<_> = self
                                    .detected
                                    .iter()
                                    .filter(|display| {
                                        self.config.room.screens.iter().any(|screen| {
                                            screen.connector.as_deref()
                                                == Some(display.connector.as_str())
                                        })
                                    })
                                    .cloned()
                                    .collect();
                                match displays::import_displays(&mut self.config, &known, false) {
                                    Ok(_) => self.record_discrete(),
                                    Err(error) => self.display_error = Some(error.to_string()),
                                }
                            }
                        }
                        Err(error) => self.display_error = Some(error),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.display_scan = None;
                    self.display_error = Some("Display scan stopped. Refresh to retry.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.auto_import
            && self.display_scan.is_none()
            && !self.detected.is_empty()
            && self.resize_origin.is_none()
            && self.object_drag.is_none()
        {
            self.use_detected(true);
        }
        if let Some(receiver) = &self.probe {
            match receiver.try_recv() {
                Ok(result) => {
                    self.probe = None;
                    if self.config.device.address == self.probe_address
                        && self.address.parse().ok() == Some(self.probe_address)
                    {
                        match result {
                            Ok(info) => {
                                if self.config.device.led_count != info.led_count {
                                    self.config.room.led_anchors.clear();
                                }
                                self.config.device.led_count = info.led_count;
                                self.record_discrete();
                                self.notify(if info.live {
                                    "Connected; another realtime source is active."
                                } else {
                                    "Connected. Map the strip; settings remain untouched."
                                });
                                self.connected = Some(info);
                                self.remember_connection();
                            }
                            Err(error) => {
                                self.probe_error = Some(format!("Connection failed: {error}"));
                                self.select_step(Inspector::Strip);
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.probe = None;
                    self.probe_error = Some("Connection stopped. Retry Connect.".into());
                    self.select_step(Inspector::Strip);
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let output = self.output.snapshot();
        let guiding = self.guidance.active();
        let invalid = self.valid().is_err();
        let inhibited = self.quiet || self.desktop_state.locked != Some(false) || invalid;
        self.notification_gate
            .update(inhibited || guiding || output.error.is_some(), now);
        if guiding && invalid {
            self.stop_guidance(StopReason::InvalidSelection);
        }
        if guiding || output.error.is_some() {
            self.engine.clear();
        } else if inhibited {
            self.engine.clear_transients();
        }
        if let Err(error) = self.engine.configure(&self.config) {
            self.validation_error = Some(error.to_string());
        }
        for _ in 0..64 {
            let Some(event) = self.desktop.try_notification() else {
                break;
            };
            match event {
                NotificationEvent::Closed {
                    generation: event_generation,
                    id,
                    reason,
                } => {
                    if event_generation == generation && reason != 1 {
                        self.engine.dismiss((event_generation, id));
                    }
                }
                NotificationEvent::Raised(notification) => {
                    if notification.generation != generation {
                        continue;
                    }
                    if !self.recent_applications.contains(&notification.application) {
                        if self.recent_applications.len() == 8 {
                            self.recent_applications.remove(0);
                        }
                        self.recent_applications
                            .push(notification.application.clone());
                    }
                    // New events received while inhibited are not replayed; closes always apply.
                    if self.notification_gate.accepts(notification.received) {
                        self.engine.notify(
                            &notification.application,
                            notification.urgency,
                            notification.received,
                            Instant::now(),
                            Some((notification.generation, notification.id)),
                        );
                    }
                }
            }
        }
        if !inhibited && !guiding {
            self.engine.set_playing(&self.desktop_state.playing);
        }
        self.tick_demos(now);
        let preview_config = self.example_config.as_ref().unwrap_or(&self.config);
        if self.preview.configure(preview_config).is_err() {
            self.preview.clear();
            self.example_config = None;
        }
        if let Some(config) = &self.example_config
            && now >= self.example_next
            && let Some(rule) = config.rules.get(self.example_index)
        {
            let application = rule.application.clone();
            let urgency = rule.options.minimum_urgency.max(1);
            self.preview.notify(&application, urgency, now, now, None);
            self.example_index += 1;
            self.example_next = now + Duration::from_secs(2);
            let name = self
                .pinned
                .iter()
                .find(|app| app.id == application)
                .map_or(application.as_str(), |app| app.name.as_str());
            self.notify(format!("Local demo: {name}"));
        }
        let now = Instant::now();
        self.engine.render(now, inhibited || guiding);
        self.preview.render(now, false);
        if self
            .example_config
            .as_ref()
            .is_some_and(|config| self.example_index >= config.rules.len())
            && !self.preview.is_active()
        {
            self.example_config = None;
        }
        let area = self.guidance_area();
        let guidance_sending = self.guidance.tick(
            self.config.device,
            area,
            self.desktop_state.locked == Some(false),
            self.quiet,
            &output,
            now,
        );
        if guidance_sending {
            self.output.submit(
                self.config.device,
                self.guidance.frame(),
                true,
                self.guidance.deadline(),
            );
        } else {
            self.output.submit(
                self.config.device,
                self.engine.frame(),
                self.enabled && !inhibited && self.engine.is_active(),
                None,
            );
        }
        self.advance_scene(now);
        if self.guidance.active()
            || self.engine.is_active()
            || self.preview.is_active()
            || self.example_config.is_some()
            || self.demo_mode == DemoMode::Random
            || (!self.sidebar_visible && self.scene_motion_ready() && !self.config.reduced_motion)
        {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(100)
        }
    }

    pub(super) fn poll_gui(&mut self, ctx: &egui::Context) {
        // Texture creation stays on the GUI path; minimized output needs no egui locks.
        self.poll_applications(ctx);
        if let Some(receiver) = &self.pinned_scan {
            match receiver.try_recv() {
                Ok(result) => {
                    self.pinned_scan = None;
                    match result {
                        Ok(apps) => {
                            self.pinned_selected = apps
                                .iter()
                                .map(|app| {
                                    self.pinned
                                        .iter()
                                        .position(|old| old.id == app.id)
                                        .and_then(|index| self.pinned_selected.get(index).copied())
                                        .unwrap_or(app.suggested)
                                })
                                .collect();
                            for app in &apps {
                                self.cache_application(ctx, app.id.clone(), Some(app.clone()));
                            }
                            self.pinned = apps;
                        }
                        Err(error) => self.pinned_error = Some(error),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pinned_scan = None;
                    self.pinned_error = Some("Pinned-app scan stopped. Refresh to retry.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_queued_during_inhibition_do_not_replay_after_resume() {
        let start = Instant::now();
        let mut gate = NotificationGate::new(start);
        gate.update(false, start);

        // Quiet can toggle on and off between worker ticks. Its UI transition
        // must invalidate queued raises even when the worker never sampled it.
        gate.discard_before(start + Duration::from_millis(10));
        let queued_during_quiet = start + Duration::from_millis(15);
        gate.discard_before(start + Duration::from_millis(20));
        assert!(!gate.accepts(queued_during_quiet));
        assert!(gate.accepts(start + Duration::from_millis(21)));

        // Lock/guidance/error recovery also discards arrivals in the interval
        // between the final paused tick and the first resumed tick.
        gate.update(true, start + Duration::from_millis(30));
        let queued_during_lock = start + Duration::from_millis(35);
        assert!(!gate.accepts(queued_during_lock));
        gate.update(false, start + Duration::from_millis(40));
        assert!(!gate.accepts(queued_during_lock));
        assert!(gate.accepts(start + Duration::from_millis(41)));
    }
}
