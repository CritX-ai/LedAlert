use super::*;

pub(super) fn controls(ui: &mut egui::Ui, rule: &mut Rule, reduced_motion: bool) {
    ui.add_space(6.0);
    let mut gradient = !rule.options.gradient.is_empty();
    egui::Grid::new("effect-controls")
        .num_columns(3)
        .spacing([6.0, 6.0])
        .show(ui, |ui| {
            ui.label("Effect");
            ui.horizontal(|ui| {
                ui.set_width(176.0);
                for (effect, icon, label, tooltip) in [
                    (EffectKind::Glow, Icon::Spark, "Glow", "A glow centered at the strip point nearest the display"),
                    (EffectKind::Ripple, Icon::Ripple, "Ripple", "One smooth wave expanding from the strip point nearest the display"),
                ] {
                    if ui.add(Action::new(icon, tooltip).label(label).selected(rule.options.effect == effect)).clicked() {
                        rule.options.effect = effect;
                    }
                }
            });
            ui.end_row();
            ui.label("Range");
            ui.horizontal(|ui| {
                ui.set_width(176.0);
                for (unit, icon, label, tooltip) in [
                    (RangeUnit::Room, Icon::Ruler, "Room", "Radius in room meters from the nearest LED. Changing units resets the range."),
                    (RangeUnit::Leds, Icon::Leds, "LEDs", "LED-index steps in either direction from the nearest LED. Changing units resets the range."),
                ] {
                    if ui.add(Action::new(icon, tooltip).label(label).selected(rule.options.range_unit == unit)).clicked()
                        && rule.options.range_unit != unit
                    {
                        rule.options.range_unit = unit;
                        rule.spread = if unit == RangeUnit::Room { 1.2 } else { 32.0 };
                    }
                }
            });
            match rule.options.range_unit {
                RangeUnit::Room => {
                    ui.add(egui::DragValue::new(&mut rule.spread)
                        .range(0.1..=20.0).speed(0.05).suffix(" m")
                        .max_decimals(2).custom_parser(decimal_parser).update_while_editing(false));
                }
                RangeUnit::Leds => {
                    ui.add(egui::DragValue::new(&mut rule.spread)
                        .range(1.0..=MAX_LEDS as f32).speed(1.0)
                        .max_decimals(0).custom_parser(decimal_parser))
                        .on_hover_text("Number of LED steps on either side");
                    rule.spread = rule.spread.round();
                }
            }
            ui.end_row();
            ui.label("Color");
            ui.horizontal(|ui| {
                ui.set_width(176.0);
                if ui.add(Action::new(Icon::Palette, "Use one color across the effect").label("Solid").selected(!gradient)).clicked() {
                    gradient = false;
                }
                if ui.add(Action::new(Icon::Palette, "Blend two to eight colors from the display to the outer range").label("Gradient").selected(gradient)).clicked() {
                    gradient = true;
                }
            });
            if !gradient {
                ui.color_edit_button_srgb(&mut rule.color);
            }
            ui.end_row();
        });
    if reduced_motion && rule.options.effect == EffectKind::Ripple {
        ui.label(
            RichText::new("Reduced motion: steady glow")
                .small()
                .color(MUTED),
        );
    }
    let selection_id = ui.make_persistent_id("selected-gradient-stop");
    if gradient && rule.options.gradient.is_empty() {
        rule.options.gradient = vec![
            GradientStop {
                position: 0.0,
                color: rule.color,
            },
            GradientStop {
                position: 1.0,
                color: [255, 101, 125],
            },
        ];
        ui.data_mut(|data| data.insert_temp(selection_id, 0_usize));
    } else if !gradient && !rule.options.gradient.is_empty() {
        rule.color = rule.options.gradient[0].color;
        rule.options.gradient.clear();
    }
    if !gradient {
        return;
    }
    ui.add_space(6.0);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 12.0), Sense::hover());
    response.on_hover_text("Left: nearest the display. Right: outer edge of the effect range. Select a color stop below to edit it.");
    for i in 0..64 {
        let t = (i as f32 + 0.5) / 64.0;
        let pair = rule
            .options
            .gradient
            .windows(2)
            .find(|pair| t <= pair[1].position)
            .unwrap();
        let amount =
            ((t - pair[0].position) / (pair[1].position - pair[0].position)).clamp(0.0, 1.0);
        let color = std::array::from_fn::<_, 3, _>(|channel| {
            (f32::from(pair[0].color[channel]) * (1.0 - amount)
                + f32::from(pair[1].color[channel]) * amount)
                .round() as u8
        });
        ui.painter().rect_filled(
            Rect::from_min_max(
                Pos2::new(rect.left() + rect.width() * i as f32 / 64.0, rect.top()),
                Pos2::new(
                    rect.left() + rect.width() * (i + 1) as f32 / 64.0,
                    rect.bottom(),
                ),
            ),
            0.0,
            Color32::from_rgb(color[0], color[1], color[2]),
        );
    }
    let length = rule.options.gradient.len();
    let mut selected = ui
        .data_mut(|data| data.get_temp::<usize>(selection_id))
        .unwrap_or(0)
        .min(length - 1);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let width = ((ui.available_width() - 4.0 * (length - 1) as f32) / length as f32).min(40.0);
        for (index, stop) in rule.options.gradient.iter().enumerate() {
            let color = Color32::from_rgb(stop.color[0], stop.color[1], stop.color[2]);
            let linear = egui::Rgba::from(color);
            let luminance = 0.2126 * linear.r() + 0.7152 * linear.g() + 0.0722 * linear.b();
            let text = if luminance > 0.179 {
                Color32::BLACK
            } else {
                Color32::WHITE
            };
            let response = ui
                .add_sized(
                    [width, 28.0],
                    egui::Button::new(RichText::new((index + 1).to_string()).color(text))
                        .fill(color)
                        .stroke(Stroke::new(
                            if selected == index { 2.0 } else { 1.0 },
                            if selected == index { text } else { LINE },
                        )),
                )
                .on_hover_text(format!(
                    "Color stop {} · {:.2}%{}",
                    index + 1,
                    stop.position * 100.0,
                    if index == 0 {
                        " · nearest display (fixed)"
                    } else if index + 1 == length {
                        " · outer edge (fixed)"
                    } else {
                        ""
                    }
                ));
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    ui.is_enabled(),
                    selected == index,
                    format!("Gradient stop {}", index + 1),
                )
            });
            if response.clicked() {
                selected = index;
            }
        }
    });
    let lo = if selected == 0 {
        0.0
    } else {
        rule.options.gradient[selected - 1].position.next_up()
    };
    let hi = if selected + 1 == length {
        1.0
    } else {
        rule.options.gradient[selected + 1].position.next_down()
    };
    let interior = selected > 0 && selected + 1 < length;
    let mut add = false;
    let mut remove = false;
    ui.horizontal(|ui| {
        ui.label(format!("Stop {}", selected + 1));
        let stop = &mut rule.options.gradient[selected];
        ui.color_edit_button_srgb(&mut stop.color);
        let mut percent = stop.position * 100.0;
        if ui
            .add_enabled(
                interior && lo <= hi,
                egui::DragValue::new(&mut percent)
                    .range(lo * 100.0..=hi.max(lo) * 100.0)
                    .speed(0.5)
                    .suffix("%")
                    .max_decimals(2)
                    .custom_parser(decimal_parser),
            )
            .on_hover_text(if interior {
                "Position within the effect range. Stops stay strictly ordered."
            } else {
                "The first and last stops stay fixed at 0% and 100%."
            })
            .changed()
        {
            stop.position = (percent / 100.0).clamp(lo, hi);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            remove = ui
                .add_enabled(
                    interior,
                    Action::new(Icon::Trash, "Remove this color stop; endpoints are fixed"),
                )
                .clicked();
            add = ui
                .add_enabled(
                    length < 8,
                    Action::new(
                        Icon::Plus,
                        "Add a color stop in the widest gap (up to eight stops)",
                    ),
                )
                .clicked();
        });
    });
    if remove {
        rule.options.gradient.remove(selected);
        selected = selected.min(rule.options.gradient.len() - 1);
    } else if add {
        let (index, pair) = rule
            .options
            .gradient
            .windows(2)
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                (a[1].position - a[0].position).total_cmp(&(b[1].position - b[0].position))
            })
            .unwrap();
        let stop = GradientStop {
            position: (pair[0].position + pair[1].position) * 0.5,
            color: std::array::from_fn(|channel| {
                ((u16::from(pair[0].color[channel]) + u16::from(pair[1].color[channel])) / 2) as u8
            }),
        };
        selected = index + 1;
        rule.options.gradient.insert(selected, stop);
    }
    ui.data_mut(|data| data.insert_temp(selection_id, selected));
}
