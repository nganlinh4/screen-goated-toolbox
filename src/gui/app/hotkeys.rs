use super::types::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, SettingsApp};
use crate::config::Hotkey;
use crate::gui::key_mapping::{egui_key_to_vk, egui_pointer_to_vk};
use eframe::egui;

impl SettingsApp {
    pub(crate) fn update_hotkey_recording(&mut self, ctx: &egui::Context) {
        if let Some(preset_idx) = self.recording_hotkey_for_preset {
            let mut key_recorded: Option<(u32, u32, String)> = None;
            let mut cancel = false;

            ctx.input(|i| {
                if i.key_pressed(egui::Key::Escape) {
                    cancel = true;
                } else {
                    let modifiers_bitmap = current_modifiers_bitmap(i);
                    collect_keyboard_hotkey(i, modifiers_bitmap, &mut key_recorded);
                    if key_recorded.is_none() {
                        collect_mouse_hotkey(i, modifiers_bitmap, &mut key_recorded);
                    }
                }
            });

            if cancel {
                self.recording_hotkey_for_preset = None;
                self.hotkey_conflict_msg = None;
            } else if let Some((vk, mods, key_name)) = key_recorded {
                self.sync_screen_record_hotkeys();
                if let Some(msg) = self.check_hotkey_conflict(vk, mods, preset_idx) {
                    self.hotkey_conflict_msg = Some(msg);
                } else {
                    let new_hotkey = Hotkey {
                        code: vk,
                        modifiers: mods,
                        name: format_hotkey_name(mods, key_name),
                    };

                    if let Some(preset) = self.config.presets.get_mut(preset_idx)
                        && !preset
                            .hotkeys
                            .iter()
                            .any(|h| h.code == vk && h.modifiers == mods)
                    {
                        preset.hotkeys.push(new_hotkey);
                        self.save_and_sync();
                    }
                    self.recording_hotkey_for_preset = None;
                    self.hotkey_conflict_msg = None;
                }
            }
        }
    }

    pub(crate) fn update_sr_hotkey_recording(&mut self, ctx: &egui::Context) {
        if self.recording_sr_hotkey {
            let mut key_recorded: Option<(u32, u32, String)> = None;
            let mut cancel = false;

            ctx.input(|i| {
                if i.key_pressed(egui::Key::Escape) {
                    cancel = true;
                } else {
                    let modifiers_bitmap = current_modifiers_bitmap(i);
                    collect_keyboard_hotkey(i, modifiers_bitmap, &mut key_recorded);
                    if key_recorded.is_none() {
                        collect_mouse_hotkey(i, modifiers_bitmap, &mut key_recorded);
                    }
                }
            });

            if cancel {
                self.recording_sr_hotkey = false;
            } else if let Some((vk, mods, key_name)) = key_recorded {
                let new_hotkey = Hotkey {
                    code: vk,
                    modifiers: mods,
                    name: format_hotkey_name(mods, key_name),
                };

                self.sync_screen_record_hotkeys();
                if let Some(msg) = self.config.check_hotkey_conflict(vk, mods, None) {
                    crate::log_info!("Hotkey conflict: {}", msg);
                } else {
                    self.config.screen_record_hotkeys.push(new_hotkey);
                    self.save_and_sync();
                }
                self.recording_sr_hotkey = false;
            }
        }
    }
}

fn current_modifiers_bitmap(input: &egui::InputState) -> u32 {
    let mut modifiers_bitmap = 0;
    if input.modifiers.ctrl {
        modifiers_bitmap |= MOD_CONTROL;
    }
    if input.modifiers.alt {
        modifiers_bitmap |= MOD_ALT;
    }
    if input.modifiers.shift {
        modifiers_bitmap |= MOD_SHIFT;
    }
    modifiers_bitmap
}

fn collect_keyboard_hotkey(
    input: &egui::InputState,
    modifiers_bitmap: u32,
    key_recorded: &mut Option<(u32, u32, String)>,
) {
    for event in &input.events {
        if let egui::Event::Key {
            key, pressed: true, ..
        } = event
            && let Some(vk) = egui_key_to_vk(key)
            && !matches!(vk, 16 | 17 | 18 | 91 | 92)
        {
            let key_name = format!("{:?}", key).trim_start_matches("Key").to_string();
            *key_recorded = Some((vk, modifiers_bitmap, key_name));
        }
    }
}

fn collect_mouse_hotkey(
    input: &egui::InputState,
    modifiers_bitmap: u32,
    key_recorded: &mut Option<(u32, u32, String)>,
) {
    for btn in [
        egui::PointerButton::Middle,
        egui::PointerButton::Extra1,
        egui::PointerButton::Extra2,
    ] {
        if input.pointer.button_pressed(btn)
            && let Some(vk) = egui_pointer_to_vk(&btn)
        {
            let name = match btn {
                egui::PointerButton::Middle => "Middle Click",
                egui::PointerButton::Extra1 => "Mouse Back",
                egui::PointerButton::Extra2 => "Mouse Forward",
                _ => "Mouse",
            }
            .to_string();
            *key_recorded = Some((vk, modifiers_bitmap, name));
            break;
        }
    }
}

fn format_hotkey_name(mods: u32, key_name: String) -> String {
    let mut name_parts = Vec::new();
    if (mods & MOD_CONTROL) != 0 {
        name_parts.push("Ctrl".to_string());
    }
    if (mods & MOD_ALT) != 0 {
        name_parts.push("Alt".to_string());
    }
    if (mods & MOD_SHIFT) != 0 {
        name_parts.push("Shift".to_string());
    }
    if (mods & MOD_WIN) != 0 {
        name_parts.push("Win".to_string());
    }
    name_parts.push(key_name);
    name_parts.join(" + ")
}
