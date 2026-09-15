use super::*;

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Undo,
    Redo,
    Play,
    Stop,
    Save,
    Back,
    Next,
    Check,
    Plus,
    Trash,
    Reset,
    Close,
    EyeOff,
    Settings,
    Help,
    Monitor,
    Bell,
    Media,
    Spark,
    Ripple,
    Palette,
    Ruler,
    Leds,
    ChevronDown,
    ChevronRight,
    Wall,
    Reverse,
    Grid,
    Power,
}

/// A normal egui button with a consistently drawn 18-point icon. The tooltip
/// remains its accessible name even when hover help is hidden.
pub(super) struct Action<'a> {
    icon: Icon,
    tooltip: &'a str,
    label: Option<&'a str>,
    selected: Option<bool>,
}

impl<'a> Action<'a> {
    pub(super) fn new(icon: Icon, tooltip: &'a str) -> Self {
        Self {
            icon,
            tooltip,
            label: None,
            selected: None,
        }
    }
    pub(super) fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }
    pub(super) fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }
}

impl egui::Widget for Action<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let icon_id = egui::Id::new("action-icon");
        let atom = egui::Atom::custom(icon_id, Vec2::splat(18.0));
        let mut button = if let Some(label) = self.label {
            egui::Button::new((atom, label)).truncate()
        } else {
            egui::Button::new(atom)
        };
        if let Some(selected) = self.selected {
            button = button.selected(selected);
        }
        let painted = button.atom_ui(ui);
        if let Some(rect) = painted.rect(icon_id) {
            let color = if self.selected == Some(true) {
                ACCENT
            } else {
                ui.style().interact(&painted.response).text_color()
            };
            self.icon.paint(ui.painter(), rect, color);
        }
        painted.response.widget_info(|| {
            if let Some(selected) = self.selected {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    selected,
                    self.tooltip,
                )
            } else {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), self.tooltip)
            }
        });
        painted
            .response
            .on_hover_text(self.tooltip)
            .on_disabled_hover_text(self.tooltip)
    }
}

impl Icon {
    pub(super) fn paint(self, painter: &egui::Painter, rect: Rect, color: Color32) {
        let at = |x: f32, y: f32| {
            Pos2::new(
                rect.left() + x / 24.0 * rect.width(),
                rect.top() + y / 24.0 * rect.height(),
            )
        };
        let stroke = Stroke::new(1.6, color);
        let path = |points: &[(f32, f32)]| {
            for pair in points.windows(2) {
                painter.line_segment([at(pair[0].0, pair[0].1), at(pair[1].0, pair[1].1)], stroke);
            }
        };
        let circle = |x: f32, y: f32, r: f32| {
            painter.circle_stroke(at(x, y), r / 24.0 * rect.width(), stroke);
        };
        match self {
            Self::Back => {
                path(&[(10., 5.), (3., 12.), (10., 19.)]);
                path(&[(3., 12.), (21., 12.)]);
            }
            Self::Next => {
                path(&[(14., 5.), (21., 12.), (14., 19.)]);
                path(&[(3., 12.), (21., 12.)]);
            }
            Self::ChevronDown => path(&[(5., 9.), (12., 16.), (19., 9.)]),
            Self::ChevronRight => path(&[(9., 5.), (16., 12.), (9., 19.)]),
            Self::Undo => {
                path(&[(8., 3.), (3., 8.), (8., 13.)]);
                path(&[
                    (3., 8.),
                    (15., 8.),
                    (20., 12.),
                    (20., 17.),
                    (16., 21.),
                    (8., 21.),
                ]);
            }
            Self::Redo => {
                path(&[(16., 3.), (21., 8.), (16., 13.)]);
                path(&[
                    (21., 8.),
                    (9., 8.),
                    (4., 12.),
                    (4., 17.),
                    (8., 21.),
                    (16., 21.),
                ]);
            }
            Self::Play => path(&[(6., 3.), (21., 12.), (6., 21.), (6., 3.)]),
            Self::Stop => path(&[(5., 5.), (19., 5.), (19., 19.), (5., 19.), (5., 5.)]),
            Self::Save => {
                path(&[
                    (4., 3.),
                    (17., 3.),
                    (21., 7.),
                    (21., 21.),
                    (3., 21.),
                    (3., 3.),
                    (4., 3.),
                ]);
                path(&[(7., 3.), (7., 9.), (16., 9.), (16., 3.)]);
                path(&[(7., 21.), (7., 14.), (17., 14.), (17., 21.)]);
            }
            Self::Check => path(&[(3., 12.), (9., 18.), (21., 5.)]),
            Self::Plus => {
                path(&[(12., 4.), (12., 20.)]);
                path(&[(4., 12.), (20., 12.)]);
            }
            Self::Close => {
                path(&[(5., 5.), (19., 19.)]);
                path(&[(19., 5.), (5., 19.)]);
            }
            Self::Trash => {
                path(&[(4., 6.), (20., 6.)]);
                path(&[(8., 6.), (8., 3.), (16., 3.), (16., 6.)]);
                path(&[(6., 6.), (7., 21.), (17., 21.), (18., 6.)]);
                path(&[(10., 10.), (10., 17.)]);
                path(&[(14., 10.), (14., 17.)]);
            }
            Self::Reset => {
                path(&[(3., 3.), (3., 9.), (9., 9.)]);
                path(&[
                    (3., 9.),
                    (7., 4.),
                    (15., 3.),
                    (20., 8.),
                    (21., 15.),
                    (16., 21.),
                    (8., 21.),
                    (3., 17.),
                ]);
            }
            Self::EyeOff => {
                path(&[
                    (2., 12.),
                    (6., 6.),
                    (12., 4.),
                    (18., 6.),
                    (22., 12.),
                    (18., 18.),
                    (12., 20.),
                    (6., 18.),
                    (2., 12.),
                ]);
                circle(12., 12., 3.);
                path(&[(3., 3.), (21., 21.)]);
            }
            Self::Settings => {
                circle(12., 12., 4.);
                for (a, b) in [
                    ((12., 2.), (12., 5.)),
                    ((12., 19.), (12., 22.)),
                    ((2., 12.), (5., 12.)),
                    ((19., 12.), (22., 12.)),
                    ((5., 5.), (7., 7.)),
                    ((17., 17.), (19., 19.)),
                    ((5., 19.), (7., 17.)),
                    ((17., 7.), (19., 5.)),
                ] {
                    path(&[a, b]);
                }
                circle(12., 12., 7.);
            }
            Self::Help => {
                circle(12., 12., 10.);
                path(&[
                    (8., 8.),
                    (9., 6.),
                    (14., 6.),
                    (16., 8.),
                    (15., 11.),
                    (12., 13.),
                    (12., 15.),
                ]);
                painter.circle_filled(at(12., 18.), 1., color);
            }
            Self::Monitor => {
                path(&[(2., 4.), (22., 4.), (22., 17.), (2., 17.), (2., 4.)]);
                path(&[(12., 17.), (12., 21.)]);
                path(&[(7., 21.), (17., 21.)]);
            }
            Self::Grid => {
                for (x, y) in [(3., 3.), (14., 3.), (3., 14.), (14., 14.)] {
                    path(&[(x, y), (x + 7., y), (x + 7., y + 7.), (x, y + 7.), (x, y)]);
                }
            }
            Self::Bell => {
                path(&[
                    (4., 17.),
                    (6., 14.),
                    (6., 8.),
                    (8., 4.),
                    (16., 4.),
                    (18., 8.),
                    (18., 14.),
                    (20., 17.),
                    (4., 17.),
                ]);
                path(&[(9., 20.), (12., 22.), (15., 20.)]);
            }
            Self::Media => {
                path(&[(9., 18.), (9., 5.), (20., 3.), (20., 16.)]);
                circle(6., 18., 3.);
                circle(17., 16., 3.);
            }
            Self::Spark => path(&[
                (12., 2.),
                (15., 9.),
                (22., 12.),
                (15., 15.),
                (12., 22.),
                (9., 15.),
                (2., 12.),
                (9., 9.),
                (12., 2.),
            ]),
            Self::Ripple => {
                circle(12., 12., 3.);
                circle(12., 12., 7.);
                circle(12., 12., 11.);
            }
            Self::Palette => {
                circle(12., 12., 10.);
                for (x, y) in [(7., 8.), (13., 6.), (18., 10.), (8., 15.)] {
                    painter.circle_filled(at(x, y), 1.5, color);
                }
                path(&[(13., 21.), (13., 16.), (18., 16.)]);
            }
            Self::Ruler => {
                path(&[(2., 7.), (22., 7.), (22., 17.), (2., 17.), (2., 7.)]);
                for x in [6., 10., 14., 18.] {
                    path(&[(x, 7.), (x, 12.)]);
                }
            }
            Self::Leds => {
                path(&[(2., 12.), (22., 12.)]);
                for x in [5., 12., 19.] {
                    painter.rect_filled(
                        Rect::from_center_size(at(x, 12.), Vec2::splat(rect.width() * 0.2)),
                        1.,
                        color,
                    );
                }
            }
            Self::Wall => {
                path(&[(3., 21.), (3., 3.), (21., 3.), (21., 21.)]);
                path(&[(3., 10.), (21., 10.)]);
                path(&[(3., 17.), (21., 17.)]);
                path(&[(10., 3.), (10., 10.)]);
                path(&[(15., 10.), (15., 17.)]);
            }
            Self::Reverse => {
                path(&[(3., 7.), (21., 7.), (16., 2.)]);
                path(&[(21., 17.), (3., 17.), (8., 22.)]);
            }
            Self::Power => {
                path(&[(12., 2.), (12., 12.)]);
                path(&[
                    (7., 4.),
                    (3., 8.),
                    (3., 16.),
                    (8., 21.),
                    (16., 21.),
                    (21., 16.),
                    (21., 8.),
                    (17., 4.),
                ]);
            }
        }
    }
}

pub(super) fn decimal_parser(text: &str) -> Option<f64> {
    let text = text.trim();
    let value = if text.contains([',', '−']) {
        text.chars()
            .map(|c| match c {
                ',' => '.',
                '−' => '-',
                other => other,
            })
            .collect::<String>()
            .parse::<f64>()
            .ok()?
    } else {
        text.parse::<f64>().ok()?
    };
    value.is_finite().then_some(value)
}

pub(super) fn distance_slider(
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> egui::Slider<'_> {
    egui::Slider::new(value, range)
        .clamping(egui::SliderClamping::Edits)
        .suffix(" m")
        .max_decimals(2)
        .custom_parser(decimal_parser)
        .update_while_editing(false)
}

#[cfg(test)]
mod tests {
    use super::{decimal_parser, distance_slider, egui};

    #[test]
    fn decimal_input_accepts_both_separators_and_negative_angles() {
        assert_eq!(decimal_parser("1,25"), Some(1.25));
        assert_eq!(decimal_parser("1.25"), Some(1.25));
        assert_eq!(decimal_parser("  −24,5 "), Some(-24.5));
        assert_eq!(decimal_parser("0,01"), Some(0.01));
    }

    #[test]
    fn ambiguous_or_nonfinite_measurements_do_not_replace_a_value() {
        for input in [
            "1,234.5", "1.234,5", "1,,5", "NaN", "inf", "-inf", "1e999", "",
        ] {
            assert_eq!(decimal_parser(input), None, "{input}");
        }
    }

    #[test]
    fn inspecting_a_distance_preserves_subcentimetre_geometry() {
        let ctx = egui::Context::default();
        let original = 1.632_000_1_f32;
        let mut distance = original;
        let mut output = ctx.run_ui(Default::default(), |ui| {
            ui.add(distance_slider(&mut distance, 0.0..=5.0));
        });
        output.textures_delta.clear();
        assert_eq!(distance, original);
    }
}
