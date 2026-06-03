use crate::config::Hotkey;
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::gui::widgets::filled_button;
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

        let theme = AppTheme::from_ui(ui);

        if *recording_hotkey_for_preset == Some(preset_idx) {
            ui.colored_label(theme.warning(), text.press_keys);

            if filled_button(
                ui,
                text.cancel_label,
                theme.hotkey_cancel_fill(),
                egui::Color32::WHITE,
                10,
            )
            .clicked()
            {
                *recording_hotkey_for_preset = None;
            }
        } else if filled_button(
            ui,
            text.add_hotkey_button,
            theme.hotkey_add_fill(),
            egui::Color32::WHITE,
            10,
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
        {
            *recording_hotkey_for_preset = Some(preset_idx);
        }

        let hotkey_item_fill = theme.hotkey_item_fill();

        let mut hotkey_to_remove = None;
        for (h_idx, hotkey) in hotkeys.iter().enumerate() {
            if filled_button(
                ui,
                &format!("{} ×", hotkey.name),
                hotkey_item_fill,
                egui::Color32::WHITE,
                10,
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
        let theme = AppTheme::from_ui(ui);
        ui.colored_label(theme.danger_text(), msg);
    }

    changed
}
