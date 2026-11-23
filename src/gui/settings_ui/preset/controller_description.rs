use crate::gui::theme::AppTheme;
use eframe::egui;

pub(super) fn render_controller_mode_description(ui: &mut egui::Ui, ui_language: &str) {
    ui.add_space(20.0);

    let is_dark = ui.visuals().dark_mode;
    let theme = AppTheme::from_dark(is_dark);
    let bg_color = theme.controller_mode_bg();
    let text_color = if is_dark {
        egui::Color32::from_gray(200)
    } else {
        egui::Color32::from_gray(60)
    };
    let accent_color = theme.controller_mode_accent();

    egui::Frame::new()
        .fill(bg_color)
        .stroke(theme.card_stroke())
        .inner_margin(24.0)
        .corner_radius(12.0)
        .show(ui, |ui| {
            ui.set_min_height(260.0);

            let text = &crate::gui::locale::LocaleText::get(ui_language).preset_editor;
            ui.label(
                egui::RichText::new(text.controller_mode_title)
                    .heading()
                    .color(accent_color),
            );

            ui.add_space(16.0);

            ui.label(egui::RichText::new(text.controller_mode_description).color(text_color));
        });
}
