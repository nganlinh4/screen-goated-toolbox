// --- FOOTER RENDERING ---
// Footer panel with tips and tips popup modal.

use super::super::types::SettingsApp;
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::render_footer;
use eframe::egui;
use egui::text::{LayoutJob, TextFormat};

impl SettingsApp {
    pub(crate) fn render_footer_and_tips_modal(&mut self, ctx: &egui::Context) {
        let text = LocaleText::get(&self.config.ui_language);
        let visuals = ctx.style().visuals.clone();
        let footer_bg = if visuals.dark_mode {
            egui::Color32::from_gray(20)
        } else {
            egui::Color32::from_gray(240)
        };

        // Determine current tip text for footer
        let current_tip = text
            .tips_list
            .get(self.current_tip_idx)
            .unwrap_or(&"")
            .to_string();

        egui::TopBottomPanel::bottom("footer_panel")
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
            .show(ctx, |ui| {
                render_footer(
                    ui,
                    &text,
                    current_tip.clone(),
                    self.tip_fade_state,
                    &mut self.show_tips_modal,
                    &mut self.pointer_gallery.show_window,
                );
            });

        // [TIPS POPUP]
        self.render_tips_popup(ctx, &text);

        // Pointer Gallery Window
        self.pointer_gallery.render(ctx, &text);

        // Render Download Manager Modal
        self.download_manager.render(ctx, &text);
    }

    fn render_tips_popup(&mut self, ctx: &egui::Context, text: &LocaleText) {
        let tips_popup_id = egui::Id::new("tips_popup_modal");

        if self.show_tips_modal {
            // Register this as an open popup so any_popup_open() returns true
            egui::Popup::open_id(ctx, tips_popup_id);

            let tips_list_copy = text.tips_list.clone();
            let tips_title = text.tips_title;

            // Popup area centered on screen
            egui::Area::new(tips_popup_id)
                .order(egui::Order::Tooltip) // High priority layer
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style())
                        .inner_margin(egui::Margin::same(16))
                        .show(ui, |ui| {
                            ui.set_max_width(1000.0);
                            ui.set_max_height(550.0);

                            // Header with title and close button
                            ui.horizontal(|ui| {
                                ui.heading(tips_title);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if crate::gui::icons::icon_button(
                                            ui,
                                            crate::gui::icons::Icon::Close,
                                        )
                                        .clicked()
                                        {
                                            self.show_tips_modal = false;
                                        }
                                    },
                                );
                            });
                            ui.separator();
                            ui.add_space(8.0);

                            // Scrollable tips list
                            egui::ScrollArea::vertical()
                                .max_height(400.0)
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    for (i, tip) in tips_list_copy.iter().enumerate() {
                                        let is_dark_mode = ctx.style().visuals.dark_mode;
                                        let layout_job =
                                            format_tip_with_bold(i + 1, tip, is_dark_mode);
                                        ui.label(layout_job);
                                        if i < tips_list_copy.len() - 1 {
                                            ui.add_space(8.0);
                                            ui.separator();
                                            ui.add_space(8.0);
                                        }
                                    }
                                });
                        });
                });

            // Close on click outside (check if clicked outside the popup area)
            if ctx.input(|i| i.pointer.any_click()) {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    // Check if click is on the backdrop (outside popup content)
                    if let Some(layer) = ctx.layer_id_at(pos) {
                        if layer.id == egui::Id::new("tips_backdrop") {
                            self.show_tips_modal = false;
                        }
                    }
                }
            }

            // Close on Escape
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.show_tips_modal = false;
            }
        }
    }
}

/// Helper function to format tips with bold text using LayoutJob
pub(super) fn format_tip_with_bold(tip_number: usize, text: &str, is_dark_mode: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    let number_text = format!("{}. ", tip_number);

    // Color scheme based on theme
    let regular_color = if is_dark_mode {
        egui::Color32::from_rgb(180, 180, 180) // Gray for dark mode
    } else {
        egui::Color32::from_rgb(100, 100, 100) // Darker gray for light mode
    };

    let bold_color = if is_dark_mode {
        egui::Color32::from_rgb(150, 200, 255) // Soft cyan for dark mode
    } else {
        egui::Color32::from_rgb(40, 100, 180) // Dark blue for light mode
    };

    // Create text format for regular text
    let mut text_format = TextFormat::default();
    text_format.font_id = egui::FontId::proportional(13.0);
    text_format.color = regular_color;

    // Append number in regular color
    job.append(&number_text, 0.0, text_format.clone());

    // Parse text for **bold** markers
    let mut current_text = String::new();
    let mut chars = text.chars().peekable();
    let mut is_bold = false;

    while let Some(ch) = chars.next() {
        if ch == '*' && chars.peek() == Some(&'*') {
            // Found ** marker
            chars.next(); // consume second *

            if !current_text.is_empty() {
                // Append accumulated text
                let mut fmt = text_format.clone();
                if is_bold {
                    fmt.color = bold_color;
                }
                job.append(&current_text, 0.0, fmt);
                current_text.clear();
            }

            is_bold = !is_bold;
        } else {
            current_text.push(ch);
        }
    }

    // Append remaining text
    if !current_text.is_empty() {
        let mut fmt = text_format.clone();
        if is_bold {
            fmt.color = bold_color;
        }
        job.append(&current_text, 0.0, fmt);
    }

    job
}
