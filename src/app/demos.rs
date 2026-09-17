use super::*;

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum DemoMode {
    #[default]
    Idle,
    Rule(usize),
    All,
    Random,
}

impl AppState {
    pub(super) fn remember_connection(&mut self) {
        let address = self.config.device.address;
        if self.connected.is_some()
            && self.probe_address == address
            && self
                .saved
                .as_ref()
                .is_some_and(|saved| saved.device.address == address)
            && self.preferences.verified_address != Some(address)
        {
            self.preferences.verified_address = Some(address);
            self.persist_preferences();
        }
    }

    pub(super) fn stop_demos(&mut self) {
        self.demo_mode = DemoMode::Idle;
        self.engine.clear_demos();
        self.demo_order.clear();
    }

    pub(super) fn demo_ready(&self) -> bool {
        self.valid().is_ok()
            && !self.quiet
            && self.desktop_state.locked == Some(false)
            && !self.guidance.active()
            && self.output.snapshot().error.is_none()
    }

    pub(super) fn scene_motion_ready(&self) -> bool {
        self.enabled && self.connected.is_some() && self.demo_ready()
    }

    pub(super) fn setup_ready(&self) -> bool {
        self.preferences.setup_complete && self.saved.is_some() && self.valid().is_ok()
    }

    fn prepare_demo(&mut self) -> bool {
        if !self.demo_ready() {
            return false;
        }
        self.preview.clear();
        self.example_config = None;
        self.engine.configure(&self.config).is_ok()
    }

    pub(super) fn play_rule_demo(&mut self) {
        if !self.prepare_demo() {
            return;
        }
        let index = self.selected_rule;
        let stop = self.demo_mode == DemoMode::Rule(index)
            && self.engine.demo_active(Some(index))
            && self
                .config
                .rules
                .get(index)
                .is_some_and(|rule| rule.options.mode == NotificationMode::Persistent);
        self.stop_demos();
        if !stop && self.engine.demo_rule(index, Instant::now()) {
            self.demo_mode = DemoMode::Rule(index);
        }
    }

    pub(super) fn play_all_demos(&mut self) {
        if !self.prepare_demo() {
            return;
        }
        let stop = self.demo_mode == DemoMode::All && self.engine.demo_active(None);
        self.stop_demos();
        if !stop {
            let now = Instant::now();
            for index in 0..self.config.rules.len() {
                self.engine.demo_rule(index, now);
            }
            if self.engine.demo_active(None) {
                self.demo_mode = DemoMode::All;
            }
        }
    }

    pub(super) fn choose_rule(&mut self, index: usize) {
        if self.selected_rule != index || matches!(self.demo_mode, DemoMode::All | DemoMode::Random)
        {
            self.stop_demos();
        }
        self.selected_rule = index;
        self.sync_rule_display();
    }

    pub(super) fn start_preview(&mut self) {
        if self.demo_mode == DemoMode::Random {
            self.stop_demos();
        } else if self.setup_ready() && self.prepare_demo() {
            self.stop_demos();
            self.demo_mode = DemoMode::Random;
            self.demo_next = Instant::now();
        }
    }

    pub(super) fn tick_demos(&mut self, now: Instant) {
        if !self.demo_ready() {
            self.stop_demos();
            return;
        }
        if self.demo_mode != DemoMode::Random {
            if !self.engine.demo_active(None) {
                self.demo_mode = DemoMode::Idle;
            }
            return;
        }
        if !self.setup_ready() {
            self.stop_demos();
            return;
        }
        if now < self.demo_next {
            return;
        }
        if self.demo_order.is_empty() {
            for index in 0..self.config.rules.len() {
                if self.engine.demo_eligible(index) {
                    self.demo_order.push(index);
                }
            }
            for index in (1..self.demo_order.len()).rev() {
                let other = next_random(&mut self.demo_rng) as usize % (index + 1);
                self.demo_order.swap(index, other);
            }
        }
        if let Some(index) = self.demo_order.pop() {
            self.engine.demo_rule(index, now);
            self.demo_next = now + random_delay(&mut self.demo_rng);
        } else {
            self.stop_demos();
        }
    }

    pub(super) fn finish_setup(&mut self) {
        self.save_config();
        if self.saved.as_ref() != Some(&self.config) {
            return;
        }
        if matches!(self.demo_mode, DemoMode::Rule(_) | DemoMode::All) {
            self.stop_demos();
        }
        self.preview.clear();
        self.example_config = None;
        self.guide = false;
        self.preferences.guide_dismissed = true;
        self.preferences.setup_complete = true;
        self.persist_preferences();
        self.sidebar_visible = false;
        self.selection = None;
        self.object_drag = None;
        self.display_drag = None;
        self.resize_origin = None;
        self.orbit_origin = None;
        self.placing_walls = false;
        self.scene_clock = Instant::now();
        self.notify("Setup saved. Select a tab to edit; lighting remains under your control.");
    }

    pub(super) fn demo_controls(&mut self, ui: &mut egui::Ui) {
        let ready = self.demo_ready();
        let one = self.demo_mode == DemoMode::Rule(self.selected_rule)
            && self.engine.demo_active(Some(self.selected_rule))
            && self
                .config
                .rules
                .get(self.selected_rule)
                .is_some_and(|rule| rule.options.mode == NotificationMode::Persistent);
        let all = self.demo_mode == DemoMode::All && self.engine.demo_active(None);
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(ready && self.engine.demo_eligible(self.selected_rule),
                Action::new(if one { Icon::Stop } else { Icon::Play },
                    "Play one notification with this rule's settings. Persistent demos toggle off; choosing another rule stops them. Uses real LEDs only while lighting is explicitly enabled.")
                    .label(if one { "Stop demo" } else { "Demo rule" }).selected(one)).clicked() {
                self.demo_request = Some(DemoMode::Rule(self.selected_rule));
            }
            if ui.add_enabled(ready && (0..self.config.rules.len()).any(|i| self.engine.demo_eligible(i)),
                Action::new(if all { Icon::Stop } else { Icon::Play },
                    "Play every enabled notification rule together. Click again, select a rule or leave Rules to stop. Uses real LEDs only while lighting is enabled.")
                    .label(if all { "Stop all" } else { "Demo all" }).selected(all)).clicked() {
                self.demo_request = Some(DemoMode::All);
            }
        });
    }
}

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn random_delay(state: &mut u64) -> Duration {
    Duration::from_millis(1000 + next_random(state) % 4001)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_delays_are_varied_and_never_less_than_one_second() {
        let mut seed = 123456789;
        let first = random_delay(&mut seed);
        let delays: Vec<_> = (0..64).map(|_| random_delay(&mut seed)).collect();
        assert!(
            delays
                .iter()
                .all(|delay| (Duration::from_secs(1)..=Duration::from_secs(5)).contains(delay))
        );
        assert!(delays.iter().any(|delay| *delay != first));
    }
}
