//! Deterministic, bounded spatial rendering; this module cannot control hardware.
use crate::config::{
    Config, EffectKind, GradientStop, MAX_RULES, NotificationMode, Point, RangeUnit, Rule,
};
use std::time::{Duration, Instant};

const MAX_PULSES: usize = 32;
const MAX_EVENT_AGE: Duration = Duration::from_secs(1);
const RIPPLE_HALF_WIDTH: f32 = 0.2;

struct Pulse {
    application: String,
    identity: Option<(u64, u32)>,
    urgency: u8,
    persistent: bool,
    rule_index: usize,
    born: Instant,
    until: Instant,
    sweep_started: Instant,
    sweep_from: f32,
    strength: f32,
    emphasize: bool,
}

impl Pulse {
    fn progress(&self, now: Instant) -> f32 {
        let elapsed = now
            .saturating_duration_since(self.sweep_started)
            .as_secs_f32();
        let duration = self.until.duration_since(self.sweep_started).as_secs_f32();
        self.sweep_from + (1.0 - self.sweep_from) * (elapsed / duration).min(1.0)
    }
}

struct SpatialField {
    anchor: usize,
    room_distances: Vec<f32>,
    led_distances: Vec<f32>,
}

pub struct Engine {
    config: Config,
    positions: Vec<Point>,
    fields: Vec<SpatialField>,
    rule_fields: Vec<usize>,
    pulses: Vec<Pulse>,
    demos: Vec<Pulse>,
    playing: Vec<String>,
    frame: Vec<[u8; 3]>,
    mixed: Vec<[f32; 3]>,
    active: bool,
}

impl Engine {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        config.validate()?;
        let mut engine = Self {
            config: config.clone(),
            positions: Vec::new(),
            fields: Vec::new(),
            rule_fields: Vec::new(),
            pulses: Vec::with_capacity(MAX_PULSES),
            demos: Vec::with_capacity(MAX_RULES),
            playing: Vec::new(),
            frame: Vec::new(),
            mixed: Vec::new(),
            active: false,
        };
        engine.rebuild();
        Ok(engine)
    }

    /// Valid edits reposition active persistent lights; finite bursts are not replayed.
    pub fn configure(&mut self, config: &Config) -> anyhow::Result<()> {
        if self.config != *config {
            config.validate()?;
            self.pulses.retain_mut(|pulse| {
                let Some(index) = config.rule_index_for(&pulse.application) else {
                    return false;
                };
                let rule = &config.rules[index];
                if !pulse.persistent
                    || !config.notifications_enabled
                    || !rule.options.notifications
                    || rule.options.mode != NotificationMode::Persistent
                    || pulse.urgency < rule.options.minimum_urgency
                    || rule.options.intensity == 0.0
                {
                    return false;
                }
                pulse.rule_index = index;
                pulse.strength =
                    (if pulse.urgency == 0 { 0.55 } else { 1.0 }) * rule.options.intensity;
                pulse.emphasize = pulse.urgency >= 2 && rule.options.critical_accent;
                true
            });
            // Demo indices belong to the exact configuration that started them, not app routing.
            self.demos.clear();
            self.config = config.clone();
            self.playing.clear();
            self.frame.fill([0; 3]);
            self.active = false;
            self.rebuild();
        }
        Ok(())
    }

    fn rebuild(&mut self) {
        self.positions = sample_allocated_strip(
            &self.config.room.strip,
            self.config.device.led_count,
            self.config.room.reverse,
            &self.config.room.led_anchors,
        );
        self.frame.resize(self.positions.len(), [0; 3]);
        self.mixed.resize(self.positions.len(), [0.0; 3]);
        let mut used = 0;
        self.rule_fields.clear();
        for rule in &self.config.rules {
            let anchor = rule_anchor(&self.config, &self.positions, rule).unwrap_or(0);
            let index = self.fields[..used]
                .iter()
                .position(|field| field.anchor == anchor)
                .unwrap_or_else(|| {
                    let index = used;
                    if index == self.fields.len() {
                        self.fields.push(SpatialField {
                            anchor,
                            room_distances: Vec::new(),
                            led_distances: Vec::new(),
                        });
                    }
                    let field = &mut self.fields[index];
                    field.anchor = anchor;
                    field.room_distances.resize(self.positions.len(), 0.0);
                    field.led_distances.resize(self.positions.len(), 0.0);
                    for (index, point) in self.positions.iter().enumerate() {
                        field.room_distances[index] = point.distance(self.positions[anchor]);
                        field.led_distances[index] = index.abs_diff(anchor) as f32;
                    }
                    used += 1;
                    index
                });
            self.rule_fields.push(index);
        }
        self.fields.truncate(used);
    }

    pub fn notify(
        &mut self,
        application: &str,
        urgency: u8,
        received: Instant,
        now: Instant,
        identity: Option<(u64, u32)>,
    ) -> bool {
        if !self.config.notifications_enabled
            || now.saturating_duration_since(received) > MAX_EVENT_AGE
            || received > now
        {
            return false;
        }
        let Some(rule_index) = self.config.rule_index_for(application) else {
            return false;
        };
        let rule = &self.config.rules[rule_index];
        if !rule.options.notifications
            || urgency < rule.options.minimum_urgency
            || rule.options.intensity == 0.0
        {
            return false;
        }
        let emphasize = urgency >= 2 && rule.options.critical_accent;
        let duration =
            Duration::from_secs_f32((rule.duration * if emphasize { 1.25 } else { 1.0 }).min(15.0));
        let persistent = rule.options.mode == NotificationMode::Persistent;
        if let Some(pulse) = self.pulses.iter_mut().find(|pulse| {
            (identity.is_some() && pulse.identity == identity)
                || (pulse.application.eq_ignore_ascii_case(application)
                    && pulse.persistent == persistent
                    && ((!persistent && pulse.until > now)
                        || (persistent && identity.is_none() && pulse.identity.is_none())))
        }) {
            // A replacement updates its own notification; finite bursts coalesce without backtracking.
            let settled = pulse.persistent && persistent && now >= pulse.until;
            if !settled
                && rule.options.effect == EffectKind::Ripple
                && pulse.persistent == persistent
            {
                pulse.sweep_from = pulse.progress(now);
                pulse.sweep_started = now;
            } else if pulse.persistent != persistent {
                pulse.born = now;
                pulse.sweep_started = now;
                pulse.sweep_from = 0.0;
            }
            if pulse.application != application {
                pulse.application = application.to_owned();
            }
            pulse.identity = identity;
            pulse.rule_index = rule_index;
            pulse.urgency = urgency;
            pulse.persistent = persistent;
            if !settled {
                pulse.until = now + duration;
            }
            pulse.emphasize = emphasize;
            pulse.strength = (if urgency == 0 { 0.55 } else { 1.0 }) * rule.options.intensity;
            return true;
        }
        if self.pulses.len() == MAX_PULSES {
            let Some(oldest) = self.pulses.iter().position(|pulse| !pulse.persistent) else {
                return false;
            };
            self.pulses.remove(oldest);
        }
        self.pulses.push(Pulse {
            application: application.to_owned(),
            identity,
            urgency,
            persistent,
            rule_index,
            born: now,
            until: now + duration,
            sweep_started: now,
            sweep_from: 0.0,
            strength: (if urgency == 0 { 0.55 } else { 1.0 }) * rule.options.intensity,
            emphasize,
        });
        true
    }

    /// Starts or restarts only this rule's synthetic notification, independently of real notices.
    pub fn demo_rule(&mut self, rule_index: usize, now: Instant) -> bool {
        if !self.demo_eligible(rule_index) {
            return false;
        }
        let rule = &self.config.rules[rule_index];
        let urgency = rule.options.minimum_urgency.max(1);
        let emphasize = urgency >= 2 && rule.options.critical_accent;
        let duration =
            Duration::from_secs_f32((rule.duration * if emphasize { 1.25 } else { 1.0 }).min(15.0));
        let demo = Pulse {
            application: String::new(),
            identity: None,
            urgency,
            persistent: rule.options.mode == NotificationMode::Persistent,
            rule_index,
            born: now,
            until: now + duration,
            sweep_started: now,
            sweep_from: 0.0,
            strength: rule.options.intensity,
            emphasize,
        };
        if let Some(previous) = self
            .demos
            .iter_mut()
            .find(|pulse| pulse.rule_index == rule_index)
        {
            *previous = demo;
        } else {
            // Config validation and replacement above bound this separately from real notifications.
            self.demos.push(demo);
        }
        true
    }

    pub fn clear_demos(&mut self) {
        self.demos.clear();
    }

    /// Reports retained demo records; rendering expires completed one-off envelopes.
    pub fn demo_active(&self, rule_index: Option<usize>) -> bool {
        self.demos
            .iter()
            .any(|pulse| rule_index.is_none_or(|index| pulse.rule_index == index))
    }

    pub fn demo_eligible(&self, rule_index: usize) -> bool {
        self.config.notifications_enabled
            && self.config.rules.get(rule_index).is_some_and(|rule| {
                rule.enabled && rule.options.notifications && rule.options.intensity > 0.0
            })
    }

    pub fn set_playing(&mut self, applications: &[String]) {
        if self.playing != applications {
            self.playing = applications.iter().take(32).cloned().collect();
        }
    }

    pub fn dismiss(&mut self, identity: (u64, u32)) {
        self.pulses.retain(|pulse| pulse.identity != Some(identity));
    }

    pub fn dismiss_persistent(&mut self, rule_index: Option<usize>) {
        self.pulses.retain(|pulse| {
            !pulse.persistent || rule_index.is_some_and(|index| pulse.rule_index != index)
        });
    }

    pub fn persistent_count(&self, rule_index: Option<usize>) -> usize {
        self.pulses
            .iter()
            .filter(|pulse| {
                pulse.persistent && rule_index.is_none_or(|index| pulse.rule_index == index)
            })
            .count()
    }

    /// A temporary inhibit suppresses active light without forgetting acknowledged persistent alerts.
    pub fn clear_transients(&mut self) {
        self.pulses.retain(|pulse| pulse.persistent);
        self.demos.clear();
        self.playing.clear();
        self.frame.fill([0; 3]);
        self.active = false;
    }

    pub fn clear(&mut self) {
        self.pulses.clear();
        self.demos.clear();
        self.playing.clear();
        self.frame.fill([0; 3]);
        self.active = false;
    }

    pub fn render(&mut self, now: Instant, quiet: bool) -> &[[u8; 3]] {
        self.frame.fill([0; 3]);
        self.mixed.fill([0.0; 3]);
        self.pulses.retain(|p| p.persistent || p.until > now);
        self.demos
            .retain(|pulse| pulse.persistent || pulse.until > now);
        self.active = false;
        if quiet {
            self.clear_transients();
            return &self.frame;
        }
        for pulse in self.pulses.iter().chain(&self.demos) {
            let rule = &self.config.rules[pulse.rule_index];
            let field = &self.fields[self.rule_fields[pulse.rule_index]];
            let age = now.saturating_duration_since(pulse.born).as_secs_f32();
            let left = pulse.until.saturating_duration_since(now).as_secs_f32();
            let envelope = if self.config.reduced_motion || !rule.options.fade {
                1.0
            } else {
                smooth((age / 0.25).min(1.0))
                    * if pulse.persistent {
                        1.0
                    } else {
                        smooth((left / 0.8).min(1.0))
                    }
            };
            let ripple = (rule.options.effect == EffectKind::Ripple
                && !self.config.reduced_motion
                && (!pulse.persistent || pulse.until > now))
                .then(|| pulse.progress(now));
            if pulse.persistent && ripple.is_some() {
                paint(
                    &mut self.mixed,
                    field,
                    rule,
                    rule.spread,
                    self.config.brightness * pulse.strength * envelope,
                    pulse.emphasize,
                    None,
                );
            }
            paint(
                &mut self.mixed,
                field,
                rule,
                rule.spread,
                self.config.brightness * pulse.strength * envelope,
                pulse.emphasize,
                ripple,
            );
            self.active = true;
        }
        if self.config.media_enabled {
            for application in &self.playing {
                let Some(index) = self.config.rule_index_for(application) else {
                    continue;
                };
                let rule = &self.config.rules[index];
                if !rule.options.media || rule.options.intensity == 0.0 {
                    continue;
                }
                let field = &self.fields[self.rule_fields[index]];
                // A quiet, steady marker; no song metadata, audio sampling or rhythmic flashes.
                paint(
                    &mut self.mixed,
                    field,
                    rule,
                    rule.spread * 0.6,
                    self.config.brightness * 0.2 * rule.options.intensity,
                    false,
                    None,
                );
                self.active = true;
            }
        }
        let cap = 255.0 * self.config.brightness;
        for (pixel, mixed) in self.frame.iter_mut().zip(&self.mixed) {
            let peak = mixed.iter().copied().fold(0.0_f32, f32::max);
            let scale = if peak > cap { cap / peak } else { 1.0 };
            *pixel = mixed.map(|channel| (channel * scale).floor().clamp(0.0, cap.floor()) as u8);
        }
        &self.frame
    }

    pub fn is_active(&self) -> bool {
        self.active && self.config.brightness > 0.0
    }
    pub fn frame(&self) -> &[[u8; 3]] {
        &self.frame
    }
    pub fn positions(&self) -> &[Point] {
        &self.positions
    }
    pub fn pulse_count(&self) -> usize {
        self.pulses.len()
    }
}

/// Device-order anchor shared by rendering and both positioning surfaces.
pub fn rule_anchor(config: &Config, positions: &[Point], rule: &Rule) -> Option<usize> {
    if positions.is_empty() {
        return None;
    }
    if let Some(position) = rule.options.position {
        return Some((position.clamp(0.0, 1.0) * (positions.len() - 1) as f32).round() as usize);
    }
    let screen = config
        .room
        .screens
        .iter()
        .find(|screen| screen.id == rule.screen_id)?;
    positions
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            a.distance(screen.position)
                .total_cmp(&b.distance(screen.position))
        })
        .map(|(index, _)| index)
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn paint(
    frame: &mut [[f32; 3]],
    field: &SpatialField,
    rule: &Rule,
    spread: f32,
    gain: f32,
    critical: bool,
    ripple: Option<f32>,
) {
    let distances = match rule.options.range_unit {
        RangeUnit::Room => &field.room_distances,
        RangeUnit::Leds => &field.led_distances,
    };
    let color = accent(rule.color.map(f32::from), critical);
    let center =
        ripple.map(|progress| progress * (1.0 + 2.0 * RIPPLE_HALF_WIDTH) - RIPPLE_HALF_WIDTH);
    for (pixel, &distance) in frame.iter_mut().zip(distances) {
        let radial = distance / spread;
        if radial > 1.0 {
            continue;
        }
        let weight = if let Some(center) = center {
            // The band enters at the anchor and fully leaves the selected radius exactly once.
            smooth((1.0 - (radial - center).abs() / RIPPLE_HALF_WIDTH).max(0.0))
        } else {
            (1.0 - radial).max(0.0)
        };
        let scale = gain * weight * weight;
        if scale <= 0.0 {
            continue;
        }
        let color = if rule.options.gradient.is_empty() {
            color
        } else {
            accent(gradient_color(&rule.options.gradient, radial), critical)
        };
        for channel in 0..3 {
            pixel[channel] += color[channel] * scale;
        }
    }
}

fn accent(color: [f32; 3], critical: bool) -> [f32; 3] {
    if critical {
        [color[0].max(220.0), color[1].min(100.0), color[2].min(75.0)]
    } else {
        color
    }
}

fn gradient_color(stops: &[GradientStop], position: f32) -> [f32; 3] {
    // Validated gradients have 2–8 stops, including both ends of the sampled radius.
    let upper = stops
        .partition_point(|stop| stop.position < position)
        .clamp(1, stops.len() - 1);
    let left = &stops[upper - 1];
    let right = &stops[upper];
    let t = (position - left.position) / (right.position - left.position);
    std::array::from_fn(|channel| {
        f32::from(left.color[channel])
            + (f32::from(right.color[channel]) - f32::from(left.color[channel])) * t
    })
}

/// Explicit LED anchors decouple allocation from the drawn path's proportions.
/// Invalid anchors produce no frame; an empty allocation keeps uniform spacing.
pub fn sample_allocated_strip(
    points: &[Point],
    count: usize,
    reverse: bool,
    anchors: &[usize],
) -> Vec<Point> {
    if anchors.is_empty() {
        return sample_strip(points, count, reverse);
    }
    if points.len() < 2
        || anchors.len() != points.len()
        || anchors.first() != Some(&0)
        || anchors.last().copied() != count.checked_sub(1)
        || !anchors.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(count);
    let mut segment = 0;
    for led in 0..count {
        while segment + 2 < anchors.len() && led > anchors[segment + 1] {
            segment += 1;
        }
        let t = (led - anchors[segment]) as f32 / (anchors[segment + 1] - anchors[segment]) as f32;
        result.push(points[segment].lerp(points[segment + 1], t));
    }
    if reverse {
        result.reverse();
    }
    result
}

/// Uniform physical spacing along a polyline, including both ends. Reversal changes LED order only.
pub fn sample_strip(points: &[Point], count: usize, reverse: bool) -> Vec<Point> {
    if points.len() < 2 || count == 0 {
        return Vec::new();
    }
    let lengths: Vec<f32> = points.windows(2).map(|p| p[0].distance(p[1])).collect();
    let total: f32 = lengths.iter().sum();
    let mut segment = 0;
    let mut start = 0.0;
    let mut result = Vec::with_capacity(count);
    for led in 0..count {
        let offset = if count == 1 {
            0.0
        } else {
            total * led as f32 / (count - 1) as f32
        };
        while segment + 1 < lengths.len() && offset > start + lengths[segment] {
            start += lengths[segment];
            segment += 1;
        }
        let t = if lengths[segment] > 0.0 {
            ((offset - start) / lengths[segment]).clamp(0.0, 1.0)
        } else {
            0.0
        };
        result.push(points[segment].lerp(points[segment + 1], t));
    }
    if reverse {
        result.reverse();
    }
    result
}
