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
    FindWindowW, GetForegroundWindow, HTCAPTION, SendMessageW, WM_NCLBUTTONDOWN,
};
#[cfg(target_os = "windows")]
use windows::core::w;

impl SettingsApp {
    pub(crate) fn render_title_bar(&mut self, root_ui: &mut egui::Ui) {
        let text = LocaleText::get(&self.config.ui_language);
        let is_dark = root_ui.visuals().dark_mode;
        let is_maximized = root_ui.input(|i| i.viewport().maximized.unwrap_or(false));

        // Match Footer Color
        let bar_bg = crate::gui::theme::AppTheme::from_dark(is_dark).bar_bg();

        egui::Panel::top("title_bar")
            .exact_size(40.0)
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
            .show_inside(root_ui, |ui| {
                let ctx = ui.ctx().clone();
                // --- DRAG HANDLE (Middle Gap Only) ---
                // Registered FIRST so buttons rendered later always take priority.
                // Uses last frame's measured gap rect — the drag zone never overlaps buttons.
                if self.title_bar_drag_rect.width() > 4.0 {
                    let drag_resp = ui.interact(
                        self.title_bar_drag_rect,
                        ui.id().with("drag_bar"),
                        egui::Sense::drag(),
                    );
                    if drag_resp.drag_started() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        start_native_window_drag_fallback();
                    }
                }

                // Hard-allocate the row as a fixed-height cell at the content top,
                // then center items within it. A plain with_layout/horizontal here
                // inherits an over-tall available rect, which egui's row layout snaps
                // to the TOP — leaving short items (theme icon, combo) high.
                let title_row_w = ui.available_width();
                let title_row_h = if is_maximized { 40.0 } else { 28.0 };
                ui.allocate_ui_with_layout(egui::vec2(title_row_w, title_row_h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;

                    // --- LEFT SIDE: Sidebar Controls ---
                    self.render_title_bar_left_side(ui, &text);
                    let left_end_x = ui.cursor().left();

                    // --- RIGHT SIDE: Window Controls & Branding ---
                    let right_start_x =
                        self.render_title_bar_right_side(ui, &ctx, is_dark, is_maximized);

                    // Update drag zone for next frame: the empty gap between the two sides.
                    let bar_rect = ui.max_rect();
                    if right_start_x > left_end_x {
                        self.title_bar_drag_rect = egui::Rect::from_min_max(
                            egui::pos2(left_end_x, bar_rect.top()),
                            egui::pos2(right_start_x, bar_rect.bottom()),
                        );
                    }
                });
            });
    }

    fn render_title_bar_left_side(&mut self, ui: &mut egui::Ui, text: &LocaleText) {
        let is_dark = ui.visuals().dark_mode;
        let theme = crate::gui::theme::AppTheme::from_dark(is_dark);
        // Launcher buttons use bright accent fills; in dark mode those fills are
        // light enough that near-black label text reads better than white.
        let btn_text = if is_dark {
            egui::Color32::from_rgb(22, 22, 26)
        } else {
            egui::Color32::WHITE
        };

        // Nudge the controls in from the rounded left corner so they breathe.
        ui.add_space(6.0);

        // Theme Switcher
        let (theme_icon, tooltip) = match self.config.theme_mode {
            crate::config::ThemeMode::Dark => (crate::gui::icons::Icon::Moon, "Theme: Dark"),
            crate::config::ThemeMode::Light => (crate::gui::icons::Icon::Sun, "Theme: Light"),
            crate::config::ThemeMode::System => {
                (crate::gui::icons::Icon::Device, "Theme: System (Auto)")
            }
        };

        if crate::gui::icons::icon_button_sized(ui, theme_icon, crate::gui::icons::ICON_XL)
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
        // A plain menu button instead of egui's ComboBox (the ComboBox draws its
        // box ~2px below a same-height button, which a wrapper cell can't fix). We
        // reserve trailing space in the label and paint a Material chevron over it,
        // so it reads as a dropdown without egui's tofu ▾ glyph. Size it off the
        // same `spacing.icon_width` egui uses for every ComboBox arrow, so this
        // dropdown's chevron matches the others instead of being an arbitrary size.
        let chevron_px = ui.spacing().icon_width;
        let chevron_gap = 4.0_f32;
        let space_w = ui
            .painter()
            .layout_no_wrap(
                " ".to_string(),
                egui::TextStyle::Button.resolve(ui.style()),
                egui::Color32::WHITE,
            )
            .rect
            .width()
            .max(0.1);
        let lead = (((chevron_px + chevron_gap) / space_w).ceil() as usize).max(1);
        let lang_label = format!("{}{}", lang_flag, " ".repeat(lead));
        let menu_resp = ui.menu_button(lang_label, |ui| {
            if ui
                .selectable_value(&mut self.config.ui_language, "en".to_string(), "🇺🇸 English")
                .clicked()
            {
                ui.close();
            }
            if ui
                .selectable_value(
                    &mut self.config.ui_language,
                    "vi".to_string(),
                    "🇻🇳 Tiếng Việt",
                )
                .clicked()
            {
                ui.close();
            }
            if ui
                .selectable_value(&mut self.config.ui_language, "ko".to_string(), "🇰🇷 한국어")
                .clicked()
            {
                ui.close();
            }
        })
        .response;
        // Paint the Material chevron over the reserved trailing space.
        let chevron_color = ui.style().interact(&menu_resp).fg_stroke.color;
        let chevron_rect = egui::Rect::from_center_size(
            egui::pos2(
                menu_resp.rect.right() - ui.spacing().button_padding.x - chevron_px / 2.0,
                menu_resp.rect.center().y,
            ),
            egui::vec2(chevron_px, chevron_px),
        );
        crate::gui::icons::paint_icon(
            ui.painter(),
            chevron_rect,
            crate::gui::icons::Icon::ArrowDown,
            chevron_color,
        );
        if original_lang != self.config.ui_language {
            self.save_and_sync();
        }

        // History Button
        ui.spacing_mut().item_spacing.x = 2.0;
        crate::gui::icons::draw_icon_static(ui, crate::gui::icons::Icon::History, Some(crate::gui::icons::ICON_SM));
        let is_history = matches!(self.view_mode, ViewMode::History);
        if ui
            .selectable_label(is_history, egui::RichText::new(text.history_btn).size(13.0))
            .clicked()
        {
            self.view_mode = ViewMode::History;
        }

        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_space(2.0);

        // Chill Corner (PromptDJ) — violet accent (its on-brand #9900ff family).
        if crate::gui::widgets::filled_icon_button(
            ui,
            crate::gui::icons::Icon::Album,
            text.prompt_dj_btn,
            theme.accent_prompt_dj(),
            btn_text,
            6,
        )
        .clicked()
        {
            crate::overlay::prompt_dj::show_prompt_dj();
        }

        // Download Manager — red accent.
        if crate::gui::widgets::filled_icon_button(
            ui,
            crate::gui::icons::Icon::Movie,
            text.download_feature_btn,
            theme.accent_download(),
            btn_text,
            6,
        )
        .clicked()
        {
            self.download_manager.show_window = true;
        }

        // Screen Record — blue accent (its design-system primary).
        if crate::gui::widgets::filled_icon_button(
            ui,
            crate::gui::icons::Icon::Videocam,
            text.screen_record_btn,
            theme.accent_screen_record(),
            btn_text,
            6,
        )
        .clicked()
        {
            crate::overlay::screen_record::show_screen_record();
        }

        // Help Assistant — teal accent (distinct from the violet PromptDJ button).
        if crate::gui::widgets::filled_icon_button(
            ui,
            crate::gui::icons::Icon::AutoStories,
            text.help_assistant_btn,
            theme.accent_help(),
            btn_text,
            6,
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
        crate::gui::icons::draw_icon_static(ui, crate::gui::icons::Icon::Settings, Some(crate::gui::icons::ICON_SM));
        let is_global = matches!(self.view_mode, ViewMode::Global);
        if ui
            .selectable_label(
                is_global,
                egui::RichText::new(text.global_settings).size(13.0),
            )
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
    ) -> f32 {
        let theme = crate::gui::theme::AppTheme::from_dark(is_dark);
        let resp = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            let grid_h = if is_maximized { 40.0 } else { 28.0 };
            let btn_size = egui::vec2(40.0, grid_h);

            // Close Button
            let close_resp = ui.allocate_response(btn_size, egui::Sense::click());
            if close_resp.clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if close_resp.hovered() {
                ui.painter()
                    .rect_filled(close_resp.rect, 0.0, theme.window_control_close_hover());
            }
            crate::gui::icons::paint_icon(
                ui.painter(),
                close_resp
                    .rect
                    .shrink2(egui::vec2(11.0, if is_maximized { 11.0 } else { 5.0 })),
                crate::gui::icons::Icon::Close,
                if close_resp.hovered() || is_dark {
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
                ui.painter()
                    .rect_filled(max_resp.rect, 0.0, theme.window_control_hover());
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
                ui.painter()
                    .rect_filled(min_resp.rect, 0.0, theme.window_control_hover());
            }
            crate::gui::icons::paint_icon(
                ui.painter(),
                min_resp
                    .rect
                    .shrink2(egui::vec2(11.0, if is_maximized { 11.0 } else { 5.0 })),
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

            // Return the leftmost x of all right-side widgets (used to measure the drag gap).
            ui.min_rect().left()
        });
        resp.inner
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
                    let handle = ctx.load_texture("app-icon-dark", color_image, Default::default());
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
