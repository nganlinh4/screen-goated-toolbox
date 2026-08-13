// --- FOOTER RENDERING ---
// Footer mini-app launchers and the Tips modal host.

use super::super::types::SettingsApp;
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::{FooterToggles, render_footer, render_tips_modal};
use eframe::egui;

impl SettingsApp {
    pub(crate) fn render_footer_and_tips_modal(&mut self, root_ui: &mut egui::Ui) {
        let text = LocaleText::get(&self.config.ui_language);
        let ctx = root_ui.ctx().clone();
        let ctx = &ctx;
        let visuals = root_ui.visuals().clone();
        let footer_bg = crate::gui::theme::AppTheme::from_dark(visuals.dark_mode).bar_bg();

        egui::Panel::bottom("footer_panel")
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::default()
                    .inner_margin(egui::Margin::symmetric(10, 4))
                    .fill(footer_bg)
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
                            0
                        } else {
                            12
                        },
                        se: if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
                            0
                        } else {
                            12
                        },
                    })
                    .stroke(egui::Stroke::NONE),
            )
            .show_inside(root_ui, |ui| {
                let screen_translate_was_open = self.show_screen_translate_dialog;
                render_footer(
                    ui,
                    &text,
                    FooterToggles {
                        show_modal: &mut self.show_tips_modal,
                        show_computer_control: &mut self.show_computer_control_dialog,
                        show_screen_translate: &mut self.show_screen_translate_dialog,
                        show_pointer_gallery: &mut self.pointer_gallery.show_window,
                        show_translation_gummy: &mut self.show_translation_gummy,
                        show_tts_playground: &mut self.show_tts_playground,
                        show_download: &mut self.download_manager.show_window,
                    },
                );
                if !screen_translate_was_open && self.show_screen_translate_dialog {
                    crate::overlay::screen_translate::prepare_detector();
                }
            });

        render_tips_modal(
            ctx,
            &text.workspace,
            &mut self.show_tips_modal,
            &mut self.selected_tips_category,
        );
        self.render_computer_control_dialog(ctx, &text);
        self.render_screen_translate_dialog(ctx, &text);

        // Pointer Gallery Window
        self.pointer_gallery.render(ctx, &text);

        if self.show_translation_gummy {
            self.show_translation_gummy = false;
            crate::overlay::translation_gummy::show_translation_gummy();
        }

        // The TTS Playground now lives in a WRY mini-app window. The footer
        // sets `show_tts_playground=true` to request open; consume the flag
        // and route to the WebView window.
        if self.show_tts_playground {
            self.show_tts_playground = false;
            crate::overlay::tts_playground::show_tts_playground();
        }

        // Render Download Manager Modal
        self.download_manager.render(ctx, &text);
    }
}
