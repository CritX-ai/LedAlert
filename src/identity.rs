//! Native identity assets and the shared dark workbench palette.
use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, RichText, Stroke,
    TextureHandle, Vec2,
};
use std::sync::Arc;

pub const INK: Color32 = Color32::from_rgb(233, 238, 250);
pub const MUTED: Color32 = Color32::from_rgb(167, 180, 206);
pub const CANVAS: Color32 = Color32::from_rgb(12, 16, 29);
pub const PANEL: Color32 = Color32::from_rgb(19, 24, 42);
pub const FLOOR: Color32 = Color32::from_rgb(27, 37, 57);
pub const LINE: Color32 = Color32::from_rgb(69, 85, 116);
pub const ACCENT: Color32 = Color32::from_rgb(91, 227, 211);
pub const CORAL: Color32 = Color32::from_rgb(255, 101, 125);
pub const WARNING: Color32 = Color32::from_rgb(255, 202, 119);
pub const ERROR: Color32 = Color32::from_rgb(255, 161, 166);

pub struct Identity {
    mark: TextureHandle,
}

pub fn icon() -> anyhow::Result<egui::IconData> {
    Ok(eframe::icon_data::from_png_bytes(include_bytes!(
        "../assets/ledalert.png"
    ))?)
}

impl Identity {
    pub fn install(ctx: &egui::Context) -> anyhow::Result<Self> {
        ctx.set_theme(egui::Theme::Dark);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = PANEL;
        visuals.window_fill = PANEL;
        visuals.override_text_color = Some(INK);
        visuals.selection.bg_fill = Color32::from_rgb(28, 73, 76);
        visuals.selection.stroke = Stroke::new(1.5, ACCENT);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(35, 45, 66);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(48, 64, 86);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(36, 85, 87);
        ctx.set_visuals(visuals);
        ctx.global_style_mut(|style| {
            style.spacing.item_spacing = Vec2::new(8.0, 8.0);
            style.spacing.button_padding = Vec2::new(10.0, 7.0);
            style.spacing.interact_size.y = 30.0;
            style.animation_time = 0.18;
            for (kind, size) in [
                (egui::TextStyle::Body, 14.0),
                (egui::TextStyle::Button, 14.0),
                (egui::TextStyle::Heading, 20.0),
                (egui::TextStyle::Small, 12.0),
            ] {
                style.text_styles.insert(kind, FontId::proportional(size));
            }
        });
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert(
            "silkscreen".into(),
            Arc::new(FontData::from_static(include_bytes!(
                "../assets/fonts/Silkscreen-Bold.ttf"
            ))),
        );
        fonts.families.insert(
            FontFamily::Name("ledalert".into()),
            vec!["silkscreen".into()],
        );
        ctx.set_fonts(fonts);
        let image = icon()?;
        let mark = ctx.load_texture(
            "ledalert-mark",
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width as usize, image.height as usize],
                &image.rgba,
            ),
            egui::TextureOptions::NEAREST,
        );
        Ok(Self { mark })
    }

    pub fn wordmark(&self, ui: &mut egui::Ui, reduced_motion: bool) {
        ui.horizontal(|ui| {
            let elapsed = ui.input(|i| i.time) % 6.0;
            let animate = !reduced_motion
                && ui.is_rect_visible(ui.max_rect())
                && ui.input(|i| i.viewport().focused.unwrap_or(true));
            let wake = if animate && elapsed < 0.9 {
                (elapsed / 0.9) as f32
            } else {
                0.0
            };
            self.mark(ui, 32.0, wake);
            if animate && elapsed < 0.9 {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(33));
            }
            ui.spacing_mut().item_spacing.x = 0.0;
            let font = FontId::new(22.0, FontFamily::Name("ledalert".into()));
            ui.label(RichText::new("LED").font(font.clone()).color(CORAL));
            ui.label(RichText::new("ALERT").font(font).color(INK));
        });
    }

    /// A finite UI-only acknowledgment; never used to animate physical output.
    pub fn mark(&self, ui: &mut egui::Ui, size: f32, wake: f32) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
        let phase = wake.clamp(0.0, 1.0);
        let offset = (phase * std::f32::consts::TAU).sin() * (1.0 - phase) * 3.0;
        let image = rect.translate(Vec2::new(offset, 0.0));
        ui.painter().image(
            self.mark.id(),
            image,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        response.on_hover_text("LedAlert. Panic optional.")
    }
}
