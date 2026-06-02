use crate::config::Hotkey;
use crate::gui::locale::LocaleText;
use eframe::egui;

pub(super) fn render_hotkeys(
    ui: &mut egui::Ui,
    preset_idx: usize,
    hotkeys: &mut Vec<Hotkey>,
    recording_hotkey_for_preset: &mut Option<usize>,
    hotkey_conflict_msg: &Option<String>,
    text: &LocaleText,
) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.hotkeys_section).strong());

        let is_dark = ui.visuals().dark_mode;

        if *recording_hotkey_for_preset == Some(preset_idx) {
            let text_color = if is_dark {
                egui::Color32::from_rgb(255, 200, 60)
            } else {
                egui::Color32::from_rgb(200, 130, 0)
            };
            ui.colored_label(text_color, text.press_keys);

            let cancel_bg = if is_dark {
                egui::Color32::from_rgb(120, 60, 60)
            } else {
                egui::Color32::from_rgb(220, 150, 150)
            };
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(text.cancel_label).color(egui::Color32::WHITE),
                    )
                    .fill(cancel_bg)
                    .corner_radius(10.0),
                )
                .clicked()
            {
                *recording_hotkey_for_preset = None;
            }
        } else {
            let add_bg = if is_dark {
                egui::Color32::from_rgb(50, 110, 120)
            } else {
                egui::Color32::from_rgb(100, 170, 180)
            };
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(text.add_hotkey_button).color(egui::Color32::WHITE),
                    )
                    .fill(add_bg)
                    .corner_radius(10.0),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                *recording_hotkey_for_preset = Some(preset_idx);
            }
        }

        let hotkey_bg = if is_dark {
            egui::Color32::from_rgb(90, 70, 130)
        } else {
            egui::Color32::from_rgb(170, 150, 200)
        };

        let mut hotkey_to_remove = None;
        for (h_idx, hotkey) in hotkeys.iter().enumerate() {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(format!("{} ×", hotkey.name))
                            .color(egui::Color32::WHITE)
                            .small(),
                    )
                    .fill(hotkey_bg)
                    .corner_radius(10.0),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                hotkey_to_remove = Some(h_idx);
            }
        }
        if let Some(hotkey_idx) = hotkey_to_remove {
            hotkeys.remove(hotkey_idx);
            changed = true;
        }
    });

    if let Some(msg) = hotkey_conflict_msg
        && *recording_hotkey_for_preset == Some(preset_idx)
    {
        ui.colored_label(egui::Color32::RED, msg);
    }

    changed
}
