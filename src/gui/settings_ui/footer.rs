use crate::gui::icons::{paint_icon, Icon};
use crate::gui::locale::LocaleText;
use eframe::egui;
use egui::text::{LayoutJob, TextFormat};

pub fn render_footer(
    ui: &mut egui::Ui,
    text: &LocaleText,
    current_tip: String,
    tip_alpha: f32,
    show_modal: &mut bool,
) {
    ui.horizontal(|ui| {
        // 1. Left Side: Pointer Gallery Button (icon inside, matches header button style)
        let btn_label = text.pointer_gallery_btn;
        let is_dark = ui.visuals().dark_mode;
        let btn_color = if is_dark {
            egui::Color32::from_rgb(150, 150, 150)
        } else {
            egui::Color32::from_rgb(120, 120, 120)
        };
        let btn_bg = if is_dark {
            egui::Color32::from_rgb(80, 80, 80)
        } else {
            egui::Color32::from_rgb(210, 210, 210)
        };
        let btn_galley = ui.painter().layout_no_wrap(
            btn_label.to_string(),
            egui::FontId::proportional(12.0),
            btn_color,
        );
        let icon_sz = 12.0;
        let icon_gap = 4.0;
        let h_pad = 6.0;
        let v_pad = 1.0; // match egui::Button default vertical padding
        let btn_w = h_pad + icon_sz + icon_gap + btn_galley.rect.width() + h_pad;
        let btn_h = btn_galley.rect.height() + v_pad * 2.0;

        let (btn_rect, _) = ui.allocate_exact_size(egui::vec2(btn_w, btn_h), egui::Sense::hover());
        let p = ui.painter();
        p.rect_filled(btn_rect, 6.0, btn_bg);
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(btn_rect.left() + h_pad, btn_rect.center().y - icon_sz / 2.0),
            egui::vec2(icon_sz, icon_sz),
        );
        paint_icon(p, icon_rect, Icon::Pointer, btn_color);
        p.galley(
            egui::pos2(
                icon_rect.right() + icon_gap,
                btn_rect.center().y - btn_galley.rect.height() / 2.0,
            ),
            btn_galley,
            btn_color,
        );

        ui.add_space(8.0);

        // 2. Center: Tips (Takes remaining space)
        let tip_color = ui.visuals().text_color().linear_multiply(tip_alpha);
        let icon_color =
            egui::Color32::from_rgba_unmultiplied(255, 200, 50, (tip_alpha * 255.0) as u8);

        let icon_size = 14.0;
        let icon_spacing = 4.0;

        let is_dark_mode = ui.visuals().dark_mode;
        let layout_job = format_footer_tip(&current_tip, tip_color, is_dark_mode, tip_alpha);
        let text_galley = ui.painter().layout_job(layout_job);
        let total_width = icon_size + icon_spacing + text_galley.rect.width();

        // Reserve ~180px for the right-side admin text
        let available = ui.available_width() - 180.0;
        let tip_width = total_width.min(available.max(100.0));

        let (response, painter) = ui.allocate_painter(
            egui::vec2(tip_width + 8.0, ui.available_height().max(18.0)),
            egui::Sense::click(),
        );
        let rect = response.rect;

        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.center().y - icon_size / 2.0),
            egui::vec2(icon_size, icon_size),
        );
        paint_icon(&painter, icon_rect, Icon::Lightbulb, icon_color);

        let text_pos = egui::pos2(
            icon_rect.right() + icon_spacing,
            rect.center().y - text_galley.rect.height() / 2.0,
        );
        painter.galley(text_pos, text_galley, egui::Color32::WHITE);

        if response.on_hover_text(text.tips_click_hint).clicked() {
            *show_modal = true;
        }

        // 3. Right Side: Admin Text (moved from left)
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_admin = cfg!(target_os = "windows") && crate::gui::utils::is_running_as_admin();
            let footer_text = if is_admin {
                egui::RichText::new(text.footer_admin_running)
                    .size(11.0)
                    .color(egui::Color32::from_rgb(34, 139, 34))
            } else {
                egui::RichText::new(text.footer_admin_text)
                    .size(11.0)
                    .color(ui.visuals().weak_text_color())
            };
            ui.label(footer_text);
        });
    });
}

// Helper function to format footer tip with bold text
fn format_footer_tip(
    text: &str,
    base_color: egui::Color32,
    is_dark_mode: bool,
    alpha_factor: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();

    // Color scheme for bold text
    let bold_color = if is_dark_mode {
        egui::Color32::from_rgb(150, 200, 255) // Soft cyan for dark mode
    } else {
        egui::Color32::from_rgb(40, 100, 180) // Dark blue for light mode
    };

    // Apply alpha to colors
    let regular_color = egui::Color32::from_rgba_unmultiplied(
        base_color.r(),
        base_color.g(),
        base_color.b(),
        (base_color.a() as f32 * alpha_factor) as u8,
    );

    let bold_color_with_alpha = egui::Color32::from_rgba_unmultiplied(
        bold_color.r(),
        bold_color.g(),
        bold_color.b(),
        (255.0 * alpha_factor) as u8,
    );

    // Create text format
    let mut text_format = TextFormat::default();
    text_format.font_id = egui::FontId::proportional(11.0);
    text_format.color = regular_color;

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
                    fmt.color = bold_color_with_alpha;
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
            fmt.color = bold_color_with_alpha;
        }
        job.append(&current_text, 0.0, fmt);
    }

    job
}
