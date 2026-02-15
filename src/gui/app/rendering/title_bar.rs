// --- TITLE BAR RENDERING ---
// Window title bar with theme/language switchers, action buttons, and window controls.

use super::super::types::SettingsApp;
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::ViewMode;
use eframe::egui;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{LPARAM, WPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetForegroundWindow, SendMessageW, HTCAPTION, WM_NCLBUTTONDOWN,
};
#[cfg(target_os = "windows")]
use windows::core::w;

impl SettingsApp {
    pub(crate) fn render_title_bar(&mut self, ctx: &egui::Context) {
        let text = LocaleText::get(&self.config.ui_language);
        let is_dark = ctx.style().visuals.dark_mode;
        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

        // Match Footer Color
        let bar_bg = if is_dark {
            egui::Color32::from_gray(20)
        } else {
            egui::Color32::from_gray(240)
        };

        egui::TopBottomPanel::top("title_bar")
            .exact_height(40.0)
            .frame(
                egui::Frame::default()
                    .inner_margin(if is_maximized {
                        egui::Margin {
                            left: 8,
                            right: 0,
                            top: 0,
                            bottom: 0,
                        }
                    } else {
                        egui::Margin {
                            left: 8,
                            right: 8,
                            top: 6,
                            bottom: 6,
                        }
                    })
                    .fill(bar_bg)
                    .corner_radius(egui::CornerRadius {
                        nw: if is_maximized { 0 } else { 12 },
                        ne: if is_maximized { 0 } else { 12 },
                        sw: 0,
                        se: 0,
                    })
                    .stroke(egui::Stroke::NONE),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                // --- DRAG HANDLE (Whole Bar) ---
                // We use interact instead of allocate_response to avoid pushing content
                let drag_resp =
                    ui.interact(ui.max_rect(), ui.id().with("drag_bar"), egui::Sense::drag());
                if drag_resp.drag_started() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    start_native_window_drag_fallback();
                }

                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;

                    // --- LEFT SIDE: Sidebar Controls ---
                    self.render_title_bar_left_side(ui, &text);

                    // --- RIGHT SIDE: Window Controls & Branding ---
                    self.render_title_bar_right_side(ui, ctx, is_dark, is_maximized);
                });
            });
    }

    fn render_title_bar_left_side(&mut self, ui: &mut egui::Ui, text: &LocaleText) {
        let is_dark = ui.ctx().style().visuals.dark_mode;

        // Theme Switcher
        let (theme_icon, tooltip) = match self.config.theme_mode {
            crate::config::ThemeMode::Dark => (crate::gui::icons::Icon::Moon, "Theme: Dark"),
            crate::config::ThemeMode::Light => (crate::gui::icons::Icon::Sun, "Theme: Light"),
            crate::config::ThemeMode::System => {
                (crate::gui::icons::Icon::Device, "Theme: System (Auto)")
            }
        };

        if crate::gui::icons::icon_button_sized(ui, theme_icon, 18.0)
            .on_hover_text(tooltip)
            .clicked()
        {
            self.config.theme_mode = match self.config.theme_mode {
                crate::config::ThemeMode::System => crate::config::ThemeMode::Dark,
                crate::config::ThemeMode::Dark => crate::config::ThemeMode::Light,
                crate::config::ThemeMode::Light => crate::config::ThemeMode::System,
            };
            self.save_and_sync();
        }

        // Language Switcher
        let original_lang = self.config.ui_language.clone();
        let lang_flag = match self.config.ui_language.as_str() {
            "vi" => "🇻🇳",
            "ko" => "🇰🇷",
            _ => "🇺🇸",
        };
        egui::ComboBox::from_id_salt("title_lang_switch")
            .width(30.0)
            .selected_text(lang_flag)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.config.ui_language, "en".to_string(), "🇺🇸 English");
                ui.selectable_value(
                    &mut self.config.ui_language,
                    "vi".to_string(),
                    "🇻🇳 Tiếng Việt",
                );
                ui.selectable_value(&mut self.config.ui_language, "ko".to_string(), "🇰🇷 한국어");
            });
        if original_lang != self.config.ui_language {
            self.save_and_sync();
        }

        // History Button
        ui.spacing_mut().item_spacing.x = 2.0;
        crate::gui::icons::draw_icon_static(ui, crate::gui::icons::Icon::History, Some(14.0));
        let is_history = matches!(self.view_mode, ViewMode::History);
        if ui
            .selectable_label(is_history, egui::RichText::new(text.history_btn).size(13.0))
            .clicked()
        {
            self.view_mode = ViewMode::History;
        }

        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_space(2.0);

        // Chill Corner (PromptDJ)
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(format!("🎵 {}", text.prompt_dj_btn))
                        .color(egui::Color32::WHITE)
                        .size(12.0),
                )
                .fill(egui::Color32::from_rgb(100, 100, 200))
                .corner_radius(6.0),
            )
            .clicked()
        {
            crate::overlay::prompt_dj::show_prompt_dj();
        }

        // Download Manager
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(format!("⬇ {}", text.download_feature_btn))
                        .color(egui::Color32::WHITE)
                        .size(12.0),
                )
                .fill(egui::Color32::from_rgb(200, 100, 100))
                .corner_radius(6.0),
            )
            .clicked()
        {
            self.download_manager.show_window = true;
        }

        // Screen Record
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(format!("🎥 {}", text.screen_record_btn))
                        .color(egui::Color32::WHITE)
                        .size(12.0),
                )
                .fill(egui::Color32::from_rgb(60, 140, 100))
                .corner_radius(6.0),
            )
            .clicked()
        {
            crate::overlay::screen_record::show_screen_record();
        }

        // Help Assistant
        let help_bg = if is_dark {
            egui::Color32::from_rgb(80, 60, 120)
        } else {
            egui::Color32::from_rgb(180, 160, 220)
        };
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(format!("❓ {}", text.help_assistant_btn))
                        .color(egui::Color32::WHITE)
                        .size(12.0),
                )
                .fill(help_bg)
                .corner_radius(6.0),
            )
            .on_hover_text(text.help_assistant_title)
            .clicked()
        {
            std::thread::spawn(|| {
                crate::gui::settings_ui::help_assistant::show_help_input();
            });
        }

        // Global Settings
        ui.spacing_mut().item_spacing.x = 2.0;
        crate::gui::icons::draw_icon_static(ui, crate::gui::icons::Icon::Settings, Some(14.0));
        let is_global = matches!(self.view_mode, ViewMode::Global);
        if ui
            .selectable_label(is_global, egui::RichText::new(text.global_settings).size(13.0))
            .clicked()
        {
            self.view_mode = ViewMode::Global;
        }
    }

    fn render_title_bar_right_side(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        is_dark: bool,
        is_maximized: bool,
    ) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            let grid_h = if is_maximized { 40.0 } else { 28.0 };
            let btn_size = egui::vec2(40.0, grid_h);

            // Close Button
            let close_resp = ui.allocate_response(btn_size, egui::Sense::click());
            if close_resp.clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if close_resp.hovered() {
                ui.painter().rect_filled(
                    close_resp.rect,
                    0.0,
                    egui::Color32::from_rgb(232, 17, 35),
                );
            }
            crate::gui::icons::paint_icon(
                ui.painter(),
                close_resp
                    .rect
                    .shrink2(egui::vec2(12.0, if is_maximized { 12.0 } else { 6.0 })),
                crate::gui::icons::Icon::Close,
                if close_resp.hovered() {
                    egui::Color32::WHITE
                } else if is_dark {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::BLACK
                },
            );

            // Maximize / Restore
            let max_resp = ui.allocate_response(btn_size, egui::Sense::click());
            if max_resp.clicked() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
            }
            if max_resp.hovered() {
                ui.painter().rect_filled(
                    max_resp.rect,
                    0.0,
                    if is_dark {
                        egui::Color32::from_gray(60)
                    } else {
                        egui::Color32::from_gray(220)
                    },
                );
            }
            let max_icon = if is_maximized {
                crate::gui::icons::Icon::Restore
            } else {
                crate::gui::icons::Icon::Maximize
            };
            crate::gui::icons::paint_icon(
                ui.painter(),
                max_resp
                    .rect
                    .shrink2(egui::vec2(13.0, if is_maximized { 13.0 } else { 7.0 })),
                max_icon,
                if is_dark {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::BLACK
                },
            );

            // Minimize
            let min_resp = ui.allocate_response(btn_size, egui::Sense::click());
            if min_resp.clicked() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            if min_resp.hovered() {
                ui.painter().rect_filled(
                    min_resp.rect,
                    0.0,
                    if is_dark {
                        egui::Color32::from_gray(60)
                    } else {
                        egui::Color32::from_gray(220)
                    },
                );
            }
            crate::gui::icons::paint_icon(
                ui.painter(),
                min_resp
                    .rect
                    .shrink2(egui::vec2(13.0, if is_maximized { 13.0 } else { 7.0 })),
                crate::gui::icons::Icon::Minimize,
                if is_dark {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::BLACK
                },
            );

            ui.add_space(8.0);

            // Title Text
            let title_text = egui::RichText::new("Screen Goated Toolbox (by nganlinh4)")
                .strong()
                .size(13.0)
                .color(if is_dark {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::BLACK
                });

            if ui
                .add(egui::Label::new(title_text).sense(egui::Sense::click()))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(
                    "https://github.com/nganlinh4/screen-goated-toolbox",
                ));
            }

            ui.add_space(6.0);

            // App Icon
            self.render_app_icon(ui, ctx, is_dark);
        });
    }

    fn render_app_icon(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, is_dark: bool) {
        let icon_handle = if is_dark {
            if self.icon_dark.is_none() {
                let bytes = include_bytes!("../../../../assets/app-icon-small.png");
                if let Ok(image) = image::load_from_memory(bytes) {
                    let resized = image.resize(128, 20, image::imageops::FilterType::Lanczos3);
                    let image_buffer = resized.to_rgba8();
                    let size = [image_buffer.width() as _, image_buffer.height() as _];
                    let pixels = image_buffer.as_raw();
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels);
                    let handle =
                        ctx.load_texture("app-icon-dark", color_image, Default::default());
                    self.icon_dark = Some(handle);
                }
            }
            self.icon_dark.as_ref()
        } else {
            if self.icon_light.is_none() {
                let bytes = include_bytes!("../../../../assets/app-icon-small-light.png");
                if let Ok(image) = image::load_from_memory(bytes) {
                    let resized = image.resize(128, 20, image::imageops::FilterType::Lanczos3);
                    let image_buffer = resized.to_rgba8();
                    let size = [image_buffer.width() as _, image_buffer.height() as _];
                    let pixels = image_buffer.as_raw();
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels);
                    let handle =
                        ctx.load_texture("app-icon-light", color_image, Default::default());
                    self.icon_light = Some(handle);
                }
            }
            self.icon_light.as_ref()
        };

        if let Some(texture) = icon_handle {
            ui.add(egui::Image::new(texture).max_height(20.0));
        }
    }
}

#[cfg(target_os = "windows")]
fn start_native_window_drag_fallback() {
    unsafe {
        // eframe StartDrag can be ignored after hidden-start restore on some setups.
        // Fall back to native caption dragging to guarantee draggability.
        let hwnd = {
            let fg = GetForegroundWindow();
            if !fg.is_invalid() {
                fg
            } else {
                let class_name = w!("eframe");
                let h = FindWindowW(class_name, None).unwrap_or_default();
                if !h.is_invalid() {
                    h
                } else {
                    let title = w!("Screen Goated Toolbox (SGT by nganlinh4)");
                    FindWindowW(None, title).unwrap_or_default()
                }
            }
        };

        if !hwnd.is_invalid() {
            let _ = ReleaseCapture();
            let _ = SendMessageW(
                hwnd,
                WM_NCLBUTTONDOWN,
                Some(WPARAM(HTCAPTION as usize)),
                Some(LPARAM(0)),
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn start_native_window_drag_fallback() {}
