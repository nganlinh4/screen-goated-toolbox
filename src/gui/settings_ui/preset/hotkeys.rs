use crate::config::{Hotkey, HotkeyConflict};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::gui::widgets::{filled_button, removable_chip};
use eframe::egui;

pub(super) fn render_hotkeys(
    ui: &mut egui::Ui,
    preset_idx: usize,
    hotkeys: &mut Vec<Hotkey>,
    recording_hotkey_for_preset: &mut Option<usize>,
    hotkey_conflict_msg: &Option<HotkeyConflict>,
    text: &LocaleText,
) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        // Vertically centre the section label with the taller buttons beside it.
        // egui's horizontal/Align::Center does NOT reliably centre a bare label
        // against taller siblings (pixel-measured ~2.5px too high), so paint the
        // label galley ourselves, centred in a cell the EXACT height of a button
        // (= button text row-height + 2·vertical button padding, floored at the
        // interactive size).
        let btn_h = (ui.text_style_height(&egui::TextStyle::Button)
            + 2.0 * ui.spacing().button_padding.y)
            .max(ui.spacing().interact_size.y);
        let lbl_color = ui.visuals().strong_text_color();
        let galley = ui.painter().layout_no_wrap(
            text.desktop_settings.hotkeys_section.to_string(),
            egui::TextStyle::Body.resolve(ui.style()),
            lbl_color,
        );
        let gsize = galley.size();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(gsize.x, btn_h), egui::Sense::hover());
        ui.painter().galley(
            egui::pos2(rect.left(), rect.center().y - gsize.y / 2.0),
            galley,
            lbl_color,
        );

        let theme = AppTheme::from_ui(ui);

        if *recording_hotkey_for_preset == Some(preset_idx) {
            ui.colored_label(theme.warning(), text.preset_basics.press_keys);

            if filled_button(
                ui,
                text.preset_basics.cancel_label,
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
            text.preset_basics.add_hotkey_button,
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
            let response =
                removable_chip(ui, &hotkey.name, hotkey_item_fill, egui::Color32::WHITE, 10);
            if response.clicked() {
                hotkey_to_remove = Some(h_idx);
            }
        }
        if let Some(hotkey_idx) = hotkey_to_remove {
            hotkeys.remove(hotkey_idx);
            changed = true;
        }
    });

    if let Some(conflict) = hotkey_conflict_msg
        && *recording_hotkey_for_preset == Some(preset_idx)
    {
        let theme = AppTheme::from_ui(ui);
        ui.colored_label(theme.danger_text(), text.hotkey_conflict_message(conflict));
    }

    changed
}
