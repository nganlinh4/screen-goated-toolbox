use eframe::egui;
use crate::gui::locale::LocaleText;
use crate::gui::icons::{Icon, paint_icon};

pub fn render_footer(
    ui: &mut egui::Ui, 
    text: &LocaleText,
    current_tip: String,
    tip_alpha: f32,
    show_modal: &mut bool
) {
    ui.horizontal(|ui| {
        // 1. Left Side: Admin Status
        // Use a fixed width container for left side to ensure stability
        ui.allocate_ui(egui::vec2(180.0, ui.available_height()), |ui| {
            ui.horizontal_centered(|ui| {
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

        // 2. Right Side: Version
        // We use with_layout to pack from right, but we need to reserve space first 
        // or egui might push the center content over it.
        // A better approach in horizontal layout: Left -> Expanded Center -> Right.
        
        // 3. Center: Tips (Takes available space)
        let version_text = format!("{} v{}", text.footer_version, env!("CARGO_PKG_VERSION"));
        let version_galley = ui.painter().layout_no_wrap(
            version_text.clone(), 
            egui::FontId::proportional(11.0), 
            ui.visuals().weak_text_color()
        );
        let version_width = version_galley.rect.width() + 10.0;

        // Allocate center space: Total - Left - Right
        let available_w = ui.available_width() - version_width;
        
        ui.allocate_ui(egui::vec2(available_w, ui.available_height()), |ui| {
            ui.vertical_centered(|ui| {
                let tip_color = ui.visuals().text_color().linear_multiply(tip_alpha);
                let icon_color = egui::Color32::from_rgba_unmultiplied(255, 200, 50, (tip_alpha * 255.0) as u8); // Yellow/gold color for lightbulb
                
                // First, calculate text width to properly center everything
                let icon_size = 14.0;
                let icon_spacing = 4.0;
                let text_galley = ui.painter().layout_no_wrap(
                    current_tip.clone(),
                    egui::FontId::proportional(11.0),
                    tip_color
                );
                let total_width = icon_size + icon_spacing + text_galley.rect.width();
                
                // Allocate space for icon + text centered
                let (response, painter) = ui.allocate_painter(
                    egui::vec2(total_width + 8.0, ui.available_height().max(18.0)), 
                    egui::Sense::click()
                );
                let rect = response.rect;
                
                // Draw lightbulb icon on the left
                let icon_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), rect.center().y - icon_size / 2.0),
                    egui::vec2(icon_size, icon_size)
                );
                paint_icon(&painter, icon_rect, Icon::Lightbulb, icon_color);
                
                // Draw text to the right of icon
                let text_pos = egui::pos2(
                    icon_rect.right() + icon_spacing,
                    rect.center().y - text_galley.rect.height() / 2.0
                );
                painter.galley(text_pos, text_galley, tip_color);

                if response.on_hover_text(text.tips_click_hint).clicked() {
                    *show_modal = true;
                }
            });
        });

        // 4. Draw Version on the far right
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(version_text).size(11.0).color(ui.visuals().weak_text_color()));
        });
    });
}
