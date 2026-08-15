// --- OVERLAY RENDERING ---
// Window resize handles and drop overlay for drag-and-drop.

use super::super::types::SettingsApp;
use crate::gui::locale::LocaleText;
use eframe::egui;

impl SettingsApp {
    pub(crate) fn render_fade_overlay(&mut self, ctx: &egui::Context) {
        if let Some(start_time) = self.fade_in_start {
            let elapsed = ctx.input(|i| i.time) - start_time;
            if elapsed < 0.6 {
                let opacity = 1.0 - (elapsed / 0.6) as f32;
                let rect = ctx.input(|i| {
                    i.viewport().inner_rect.unwrap_or(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::Vec2::ZERO,
                    ))
                });
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("fade_overlay"),
                ));
                painter.rect_filled(
                    rect,
                    0.0,
                    eframe::egui::Color32::from_black_alpha((opacity * 255.0) as u8),
                );
                ctx.request_repaint();
            } else {
                self.fade_in_start = None;
            }
        }
    }

    /// Render a drop overlay when files are being dragged over the window
    pub(crate) fn render_drop_overlay(&mut self, ctx: &egui::Context) {
        use super::super::input_handler::is_files_hovered;

        // --- ANIMATION LOGIC ---
        let delta = ctx.input(|i| i.stable_dt).min(0.1);
        let is_hovered = is_files_hovered(ctx);
        let fade_speed = 8.0_f32;

        if is_hovered {
            self.drop_overlay_fade += fade_speed * delta;
        } else {
            self.drop_overlay_fade -= fade_speed * delta;
        }
        self.drop_overlay_fade = self.drop_overlay_fade.clamp(0.0, 1.0);

        // If completely invisible and not hovered, do nothing
        if self.drop_overlay_fade <= 0.0 {
            return;
        }

        // Keep repainting while animating
        if self.drop_overlay_fade > 0.0 && self.drop_overlay_fade < 1.0 {
            ctx.request_repaint();
        } else if is_hovered {
            ctx.request_repaint(); // Animate bobbing
        }

        // --- RENDER ---
        let text = LocaleText::get(&self.config.ui_language);
        let screen_rect = ctx.content_rect();

        // Overlay layer (Debug order to stay on top)
        let overlay_layer = egui::LayerId::new(egui::Order::Debug, egui::Id::new("drop_overlay"));
        let painter = ctx.layer_painter(overlay_layer);

        // Backdrop with fade
        let max_alpha = 180;
        let alpha = (max_alpha as f32 * self.drop_overlay_fade) as u8;
        let backdrop_color = egui::Color32::from_rgba_unmultiplied(0, 120, 215, alpha);
        painter.rect_filled(screen_rect, 0.0, backdrop_color);

        // Content opacity
        let content_opacity = self.drop_overlay_fade;
        let element_color = egui::Color32::from_white_alpha((255.0_f32 * content_opacity) as u8);

        // Dashed border with pulse
        let inset = 24.0;
        let inner_rect = screen_rect.shrink(inset);
        let time = ctx.input(|i| i.time);
        let pulse = (time * 2.5).sin() as f32 * 0.2_f32 + 0.8_f32;
        let border_alpha = (255.0_f32 * content_opacity * pulse) as u8;
        let border_color = egui::Color32::from_white_alpha(border_alpha);
        let stroke = egui::Stroke::new(3.0, border_color);

        let dash_length = 12.0;
        let gap_length = 8.0;

        // Helper to draw dashed line
        let draw_dashed_line = |p1: egui::Pos2, p2: egui::Pos2| {
            let vec = p2 - p1;
            let len = vec.length();
            let dir = vec / len;
            let count = (len / (dash_length + gap_length)).ceil() as i32;

            for i in 0..count {
                let start = p1 + dir * (i as f32 * (dash_length + gap_length));
                let end = start + dir * dash_length;
                let end = if (end - p1).length() > len { p2 } else { end };
                painter.line_segment([start, end], stroke);
            }
        };

        draw_dashed_line(inner_rect.left_top(), inner_rect.right_top());
        draw_dashed_line(inner_rect.right_top(), inner_rect.right_bottom());
        draw_dashed_line(inner_rect.right_bottom(), inner_rect.left_bottom());
        draw_dashed_line(inner_rect.left_bottom(), inner_rect.left_top());

        // Center content
        let center = screen_rect.center();
        let icon_size = 64.0;

        // Bobbing animation
        let bob_offset = (time * 5.0).sin() as f32 * 4.0_f32;

        // Draw Rounded Document Icon
        let file_width = icon_size * 0.7;
        let file_height = icon_size * 0.9;
        let file_rect = egui::Rect::from_center_size(center, egui::vec2(file_width, file_height));

        painter.rect_stroke(
            file_rect,
            8.0_f32,
            egui::Stroke::new(3.0, element_color),
            egui::StrokeKind::Middle,
        );

        // Draw Arrow (Bobbing inside)
        let arrow_center = center + egui::vec2(0.0, bob_offset);
        let arrow_len = icon_size * 0.4;
        let arrow_start = arrow_center - egui::vec2(0.0, arrow_len * 0.5);
        let arrow_end = arrow_center + egui::vec2(0.0, arrow_len * 0.5);

        let arrow_stroke = egui::Stroke::new(4.0, element_color);
        painter.line_segment([arrow_start, arrow_end], arrow_stroke);

        let arrow_head_size = 10.0;
        painter.line_segment(
            [
                arrow_end,
                arrow_end + egui::vec2(-arrow_head_size, -arrow_head_size),
            ],
            arrow_stroke,
        );
        painter.line_segment(
            [
                arrow_end,
                arrow_end + egui::vec2(arrow_head_size, -arrow_head_size),
            ],
            arrow_stroke,
        );

        // Text below
        let text_offset_y = icon_size * 0.8;
        let text_pos = center + egui::vec2(0.0, text_offset_y);
        let galley = painter.layout_no_wrap(
            text.shell.drop_overlay_text.to_string(),
            egui::FontId::proportional(22.0),
            element_color,
        );
        let text_rect = galley.rect;
        painter.galley(
            text_pos - egui::vec2(text_rect.width() * 0.5, 0.0),
            galley,
            element_color,
        );

        // Request repaint for close animation
        if self.drop_overlay_fade > 0.0 {
            ctx.request_repaint();
        }
    }
}
