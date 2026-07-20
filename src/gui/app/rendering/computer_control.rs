use super::super::types::SettingsApp;
use crate::gui::icons::{self, Icon};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::gui::widgets::{dialog_header, filled_button, removable_chip};
use eframe::egui;

impl SettingsApp {
    pub(super) fn render_computer_control_dialog(
        &mut self,
        ctx: &egui::Context,
        text: &LocaleText,
    ) {
        if !self.show_computer_control_dialog {
            return;
        }

        let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);
        let mut close_requested = false;
        let modal = egui::Modal::new(egui::Id::new("computer_control_dialog"))
            .backdrop_color(theme.scrim_color())
            .frame(theme.dialog_frame())
            .show(ctx, |ui| {
                ui.set_width(430.0);

                if dialog_header(ui, &theme, text.shell.computer_control_title, None, |_| {}) {
                    close_requested = true;
                }

                render_intro(ui, &theme, text);
                ui.add_space(12.0);
                self.render_computer_control_hotkey(ui, &theme, text);
                ui.add_space(16.0);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let active = crate::overlay::computer_control::is_active();
                    let label = if active {
                        text.shell.computer_control_stop
                    } else {
                        text.shell.computer_control_start
                    };
                    let fill = if active {
                        theme.danger_fill()
                    } else {
                        theme.accent_fill()
                    };
                    if filled_button(ui, label, fill, theme.on_accent(), 16).clicked() {
                        if active {
                            crate::overlay::computer_control::stop_overlay();
                        } else {
                            crate::overlay::computer_control::show_overlay();
                        }
                        close_requested = true;
                    }
                });
            });

        if modal.should_close() {
            close_requested = true;
        }
        if close_requested {
            self.show_computer_control_dialog = false;
            self.recording_computer_control_hotkey = false;
            self.computer_control_hotkey_conflict_msg = None;
        }
    }

    fn render_computer_control_hotkey(
        &mut self,
        ui: &mut egui::Ui,
        theme: &AppTheme,
        text: &LocaleText,
    ) {
        let mut hotkey_to_remove = None;
        egui::Frame::new()
            .fill(theme.card_bg())
            .stroke(theme.card_stroke())
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(text.shell.computer_control_hotkey_label)
                            .strong()
                            .color(theme.on_surface()),
                    );

                    if self.recording_computer_control_hotkey {
                        ui.colored_label(theme.warning(), text.preset_basics.press_keys);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if filled_button(
                                ui,
                                text.preset_basics.cancel_label,
                                theme.hotkey_cancel_fill(),
                                egui::Color32::WHITE,
                                12,
                            )
                            .clicked()
                            {
                                self.recording_computer_control_hotkey = false;
                                self.computer_control_hotkey_conflict_msg = None;
                            }
                        });
                    } else if filled_button(
                        ui,
                        text.preset_basics.add_hotkey_button,
                        theme.hotkey_add_fill(),
                        egui::Color32::WHITE,
                        10,
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                    {
                        self.recording_computer_control_hotkey = true;
                        self.computer_control_hotkey_conflict_msg = None;
                    }
                });

                ui.add_space(8.0);
                if self.config.computer_control_hotkeys.is_empty() {
                    ui.label(
                        egui::RichText::new(text.shell.computer_control_hotkey_unset)
                            .color(theme.on_surface_variant()),
                    );
                } else {
                    ui.horizontal_wrapped(|ui| {
                        for hotkey in &self.config.computer_control_hotkeys {
                            if removable_chip(
                                ui,
                                &hotkey.name,
                                theme.hotkey_item_fill(),
                                egui::Color32::WHITE,
                                10,
                            )
                            .clicked()
                            {
                                hotkey_to_remove = Some((hotkey.code, hotkey.modifiers));
                            }
                        }
                    });
                }

                if let Some(conflict) = &self.computer_control_hotkey_conflict_msg {
                    ui.add_space(6.0);
                    ui.colored_label(theme.danger_text(), text.hotkey_conflict_message(conflict));
                }
            });

        if let Some((code, modifiers)) = hotkey_to_remove {
            self.sync_global_hotkeys();
            if let Some(index) = self
                .config
                .computer_control_hotkeys
                .iter()
                .position(|hotkey| hotkey.code == code && hotkey.modifiers == modifiers)
            {
                self.config.computer_control_hotkeys.remove(index);
                self.computer_control_hotkey_conflict_msg = None;
                self.save_and_sync();
            }
        }
    }
}

fn render_intro(ui: &mut egui::Ui, theme: &AppTheme, text: &LocaleText) {
    egui::Frame::new()
        .fill(theme.neutral_fill())
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let icon_size = 34.0;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(icon_size, icon_size), egui::Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::same(10),
                    theme.launch_computer_control(),
                );
                let glyph_rect = egui::Rect::from_center_size(
                    rect.center(),
                    egui::vec2(icons::ICON_XL, icons::ICON_XL),
                );
                let glyph_color = if ui.visuals().dark_mode {
                    egui::Color32::from_rgb(22, 22, 26)
                } else {
                    egui::Color32::WHITE
                };
                icons::paint_icon(ui.painter(), glyph_rect, Icon::SmartToy, glyph_color);

                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(text.shell.computer_control_intro)
                            .size(13.0)
                            .color(theme.on_surface()),
                    );
                    ui.add_space(3.0);
                    ui.label(
                        egui::RichText::new(text.shell.computer_control_note)
                            .size(11.5)
                            .color(theme.on_surface_variant()),
                    );
                });
            });
        });
}
