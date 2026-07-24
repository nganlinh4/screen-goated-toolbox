use super::SettingsApp;
use crate::gui::icons::{Icon, draw_icon_static};
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::node_graph::request_node_graph_view_reset;
use crate::gui::theme::AppTheme;
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PresetModelUpdateAction {
    None,
    Skip,
    Apply,
}

impl SettingsApp {
    #[cfg(debug_assertions)]
    pub(crate) fn update_preset_model_update_preview_shortcut(&mut self, ctx: &egui::Context) {
        if self.recording_hotkey_for_preset.is_some()
            || self.recording_sr_hotkey
            || self.recording_computer_control_hotkey
        {
            return;
        }

        let shortcut = egui::KeyboardShortcut::new(
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Key::U,
        );
        if ctx.input_mut(|input| input.consume_shortcut(&shortcut)) {
            if self.config.pending_preset_model_update.is_none() {
                self.config.stage_preset_model_update_preview();
            }
            self.show_preset_model_update_modal = true;
            crate::log_info!("[Debug] showing preset model update dialog preview (Ctrl+Shift+U)");
        }
    }

    pub(crate) fn render_preset_model_update_modal(&mut self, ui: &mut egui::Ui) {
        if !self.show_preset_model_update_modal {
            return;
        }

        let text = LocaleText::get(&self.config.ui_language);
        match show_dialog(ui, &text) {
            PresetModelUpdateAction::None => {}
            PresetModelUpdateAction::Skip => {
                self.config.finish_preset_model_update(false);
                self.show_preset_model_update_modal = false;
                self.save_and_sync();
            }
            PresetModelUpdateAction::Apply => {
                self.config.finish_preset_model_update(true);
                self.show_preset_model_update_modal = false;
                self.snarl = None;
                self.last_edited_preset_key = None;
                request_node_graph_view_reset(ui.ctx());
                self.save_and_sync();
            }
        }
    }
}

fn show_dialog(ui: &mut egui::Ui, text: &LocaleText) -> PresetModelUpdateAction {
    let theme = AppTheme::from_ui(ui);
    let mut action = PresetModelUpdateAction::None;
    let _modal = egui::Modal::new(egui::Id::new("preset_model_update_modal"))
        .backdrop_color(theme.scrim_color())
        .frame(theme.dialog_frame())
        .show(ui.ctx(), |ui| {
            ui.set_width(520.0);

            ui.horizontal(|ui| {
                draw_icon_static(ui, Icon::Upgrade, Some(crate::gui::icons::ICON_MD));
                ui.heading(text.desktop_settings.preset_model_update_title);
            });
            ui.add_space(8.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(text.desktop_settings.preset_model_update_body)
                        .size(13.0)
                        .color(theme.on_surface_variant()),
                )
                .wrap(),
            );
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if crate::gui::widgets::filled_button(
                        ui,
                        text.desktop_settings.preset_model_update_apply,
                        theme.accent_fill(),
                        theme.on_accent(),
                        16,
                    )
                    .clicked()
                    {
                        action = PresetModelUpdateAction::Apply;
                    }
                    if crate::gui::widgets::filled_button(
                        ui,
                        text.desktop_settings.preset_model_update_skip,
                        theme.neutral_fill(),
                        theme.on_surface(),
                        16,
                    )
                    .clicked()
                    {
                        action = PresetModelUpdateAction::Skip;
                    }
                });
            });
        });

    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            egui::Id::new("preset_model_update_modal_test_rect"),
            _modal.response.rect,
        );
    });

    action
}

#[cfg(test)]
mod tests {
    use super::show_dialog;
    use crate::gui::locale::LocaleText;
    use eframe::egui;

    #[test]
    fn vietnamese_actions_match_the_product_copy() {
        let text = LocaleText::get("vi");
        assert_eq!(
            text.desktop_settings.preset_model_update_title,
            "Áp dụng các cài đặt mô hình preset mới?"
        );
        assert_eq!(
            text.desktop_settings.preset_model_update_body,
            "Bản cập nhật này có các mô hình đề xuất mới hơn cho một số preset. Khi áp dụng, chỉ trường mô hình của các preset đó được thay đổi; prompt, sơ đồ, chế độ, mục yêu thích, phím tắt và mọi cài đặt khác được giữ nguyên. Danh sách ưu tiên mô hình cũng được cập nhật theo mặc định mới. Nếu bỏ qua, bạn vẫn có thể nhận mặc định mới sau này bằng cách khôi phục tất cả preset hoặc từng preset."
        );
        assert_eq!(
            text.desktop_settings.preset_model_update_skip,
            "Không áp dụng"
        );
        assert_eq!(text.desktop_settings.preset_model_update_apply, "Áp dụng");
    }

    #[test]
    fn dialog_fits_the_minimum_window_in_every_supported_locale() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1245.0, 660.0));

        for language in ["en", "vi", "ko"] {
            let context = egui::Context::default();
            crate::gui::configure_fonts(&context);
            crate::gui::theme::AppTheme::apply_global_style(&context, false);
            let text = LocaleText::get(language);
            let mut observed_rects = Vec::new();

            for frame in 0..2 {
                let _ = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        time: Some(frame as f64 / 60.0),
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show_inside(ui, |ui| {
                            let _ = show_dialog(ui, &text);
                        });
                    },
                );
                observed_rects.push(
                    context
                        .data(|data| {
                            data.get_temp::<egui::Rect>(egui::Id::new(
                                "preset_model_update_modal_test_rect",
                            ))
                        })
                        .expect("modal rect should be captured"),
                );
            }

            let modal_rect = *observed_rects.last().unwrap();
            assert!(
                screen.contains_rect(modal_rect),
                "{language} dialog overflowed minimum viewport: {observed_rects:?}"
            );
        }
    }
}
