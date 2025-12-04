use eframe::egui;
use crate::gui::locale::LocaleText;

pub fn render_footer(ui: &mut egui::Ui, text: &LocaleText) {
    ui.horizontal(|ui| {
        // --- NEW LOGIC: Admin Status ---
        let is_admin = cfg!(target_os = "windows") && crate::gui::utils::is_running_as_admin();
        let footer_text = if is_admin {
            egui::RichText::new(text.footer_admin_running)
                 .size(11.0)
                 .color(egui::Color32::from_rgb(34, 139, 34)) // Same green as UpdateStatus::UpToDate
        } else {
            egui::RichText::new(text.footer_admin_text)
                 .size(11.0)
                 .color(ui.visuals().weak_text_color())
        };
        ui.label(footer_text);
        // -------------------------------
        
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let version_text = format!("{} v{}", text.footer_version, env!("CARGO_PKG_VERSION"));
            ui.label(egui::RichText::new(version_text).size(11.0).color(ui.visuals().weak_text_color()));
        });
    });
}
