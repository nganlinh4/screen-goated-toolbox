use super::super::types::SettingsApp;
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::gui::widgets::dialog_header;
use eframe::egui;

impl SettingsApp {
    pub(super) fn render_image_creator_coming_soon_dialog(
        &mut self,
        ctx: &egui::Context,
        text: &LocaleText,
    ) {
        if !self.show_image_creator_coming_soon_dialog {
            return;
        }

        let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);
        let mut close_requested = false;
        let modal = crate::gui::widgets::material_modal(
            ctx,
            &theme,
            egui::Id::new("image_creator_coming_soon_dialog"),
            |ui| {
                ui.set_width(360.0);
                if dialog_header(ui, &theme, text.shell.image_creator_title, None, |_| {}) {
                    close_requested = true;
                }
                ui.label(
                    egui::RichText::new(text.shell.coming_soon_label)
                        .size(15.0)
                        .color(theme.on_surface_variant()),
                );
                ui.add_space(8.0);
            },
        );

        if modal.should_close() {
            close_requested = true;
        }
        if close_requested {
            self.show_image_creator_coming_soon_dialog = false;
        }
    }

    pub(super) fn render_image_to_svg_coming_soon_dialog(
        &mut self,
        ctx: &egui::Context,
        text: &LocaleText,
    ) {
        if !self.show_image_to_svg_coming_soon_dialog {
            return;
        }

        let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);
        let mut close_requested = false;
        let modal = crate::gui::widgets::material_modal(
            ctx,
            &theme,
            egui::Id::new("image_to_svg_coming_soon_dialog"),
            |ui| {
                ui.set_width(360.0);
                if dialog_header(ui, &theme, text.shell.image_to_svg_title, None, |_| {}) {
                    close_requested = true;
                }
                ui.label(
                    egui::RichText::new(text.shell.coming_soon_label)
                        .size(15.0)
                        .color(theme.on_surface_variant()),
                );
                ui.add_space(8.0);
            },
        );

        if modal.should_close() {
            close_requested = true;
        }
        if close_requested {
            self.show_image_to_svg_coming_soon_dialog = false;
        }
    }
}
