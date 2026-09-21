use super::types::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, SettingsApp};
use crate::config::Hotkey;
use crate::gui::key_mapping::{egui_key_to_vk, egui_pointer_to_vk};
use eframe::egui;

impl SettingsApp {
    pub(crate) fn sync_hotkey_capture(&self, ctx: &egui::Context) {
        let owner = if self.recording_computer_control_hotkey {
            Some(usize::MAX)
        } else if self.recording_screen_translate_hotkey {
            Some(usize::MAX - 1)
        } else if self.recording_live_translate_hotkey {
            Some(usize::MAX - 2)
        } else if self.recording_sr_hotkey {
            Some(usize::MAX - 3)
        } else {
            self.recording_hotkey_for_preset
        };
        let owner = owner.filter(|_| ctx.input(|input| input.focused));
        crate::hotkey::set_binding_capture(owner.is_some());
        super::hotkey_capture::sync(unsafe { super::utils::main_window_hwnd() }, owner, ctx);
    }

    pub(crate) fn update_hotkey_recording(&mut self, ctx: &egui::Context) {
        if let Some(preset_idx) = self.recording_hotkey_for_preset {
            let mut key_recorded: Option<(u32, u32)> = None;
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
            } else if let Some((vk, mods)) = key_recorded {
                self.sync_global_hotkeys();
                if let Some(conflict) = self.check_hotkey_conflict(vk, mods, preset_idx) {
                    self.hotkey_conflict_msg = Some(conflict);
                } else {
                    let new_hotkey = Hotkey {
                        code: vk,
                        modifiers: mods,
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
            let mut key_recorded: Option<(u32, u32)> = None;
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
            } else if let Some((vk, mods)) = key_recorded {
                let new_hotkey = Hotkey {
                    code: vk,
                    modifiers: mods,
                };

                self.sync_global_hotkeys();
                if let Some(conflict) = self.config.check_hotkey_conflict(vk, mods, None) {
                    crate::log_info!("[Hotkey] configuration conflict: {:?}", conflict);
                } else {
                    self.config.screen_record_hotkeys.push(new_hotkey);
                    self.save_and_sync();
                }
                self.recording_sr_hotkey = false;
            }
        }
    }

    pub(crate) fn update_computer_control_hotkey_recording(&mut self, ctx: &egui::Context) {
        if !self.recording_computer_control_hotkey {
            return;
        }

        let mut key_recorded: Option<(u32, u32)> = None;
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
            self.recording_computer_control_hotkey = false;
            self.computer_control_hotkey_conflict_msg = None;
            return;
        }

        let Some((vk, mods)) = key_recorded else {
            return;
        };

        self.sync_global_hotkeys();
        if let Some(conflict) = self.config.check_hotkey_conflict(vk, mods, None) {
            self.computer_control_hotkey_conflict_msg = Some(conflict);
            return;
        }

        self.config.computer_control_hotkeys.push(Hotkey {
            code: vk,
            modifiers: mods,
        });
        self.recording_computer_control_hotkey = false;
        self.computer_control_hotkey_conflict_msg = None;
        self.save_and_sync();
    }

    pub(crate) fn update_screen_translate_hotkey_recording(&mut self, ctx: &egui::Context) {
        if !self.recording_screen_translate_hotkey {
            return;
        }

        let mut key_recorded = None;
        let mut cancel = false;
        ctx.input(|input| {
            if input.key_pressed(egui::Key::Escape) {
                cancel = true;
            } else {
                let modifiers = current_modifiers_bitmap(input);
                collect_keyboard_hotkey(input, modifiers, &mut key_recorded);
                if key_recorded.is_none() {
                    collect_mouse_hotkey(input, modifiers, &mut key_recorded);
                }
            }
        });

        if cancel {
            self.recording_screen_translate_hotkey = false;
            self.screen_translate_hotkey_conflict_msg = None;
            return;
        }
        let Some((code, modifiers)) = key_recorded else {
            return;
        };

        self.sync_global_hotkeys();
        if let Some(conflict) = self.config.check_hotkey_conflict(code, modifiers, None) {
            self.screen_translate_hotkey_conflict_msg = Some(conflict);
            return;
        }
        let hotkeys = if self.recording_screen_translate_subtitles {
            &mut self.config.screen_translate.subtitle_hotkeys
        } else if self.recording_screen_translate_fullscreen {
            &mut self.config.screen_translate.fullscreen_hotkeys
        } else {
            &mut self.config.screen_translate.hotkeys
        };
        hotkeys.push(Hotkey { code, modifiers });
        self.recording_screen_translate_hotkey = false;
        self.screen_translate_hotkey_conflict_msg = None;
        self.save_and_sync();
    }

    pub(crate) fn update_live_translate_hotkey_recording(&mut self, ctx: &egui::Context) {
        if !self.recording_live_translate_hotkey {
            return;
        }

        let mut key_recorded = None;
        let mut cancel = false;
        ctx.input(|input| {
            if input.key_pressed(egui::Key::Escape) {
                cancel = true;
            } else {
                let modifiers = current_modifiers_bitmap(input);
                collect_keyboard_hotkey(input, modifiers, &mut key_recorded);
                if key_recorded.is_none() {
                    collect_mouse_hotkey(input, modifiers, &mut key_recorded);
                }
            }
        });

        if cancel {
            self.recording_live_translate_hotkey = false;
            self.live_translate_hotkey_conflict_msg = None;
            return;
        }
        let Some((code, modifiers)) = key_recorded else {
            return;
        };

        self.sync_global_hotkeys();
        if let Some(conflict) = self.config.check_hotkey_conflict(code, modifiers, None) {
            self.live_translate_hotkey_conflict_msg = Some(conflict);
            return;
        }
        self.config
            .live_translate
            .hotkeys
            .push(Hotkey { code, modifiers });
        self.recording_live_translate_hotkey = false;
        self.live_translate_hotkey_conflict_msg = None;
        self.save_and_sync();
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
    if unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_LWIN, VK_RWIN};
        GetKeyState(VK_LWIN.0 as i32) < 0 || GetKeyState(VK_RWIN.0 as i32) < 0
    } {
        modifiers_bitmap |= MOD_WIN;
    }
    modifiers_bitmap
}

fn collect_keyboard_hotkey(
    input: &egui::InputState,
    modifiers_bitmap: u32,
    key_recorded: &mut Option<(u32, u32)>,
) {
    if let Some(binding) = super::hotkey_capture::take() {
        *key_recorded = Some(binding);
        return;
    }
    for event in &input.events {
        if let egui::Event::Key {
            key, pressed: true, ..
        } = event
            && let Some(vk) = egui_key_to_vk(key)
            && !matches!(vk, 16 | 17 | 18 | 91 | 92)
        {
            *key_recorded = Some((vk, modifiers_bitmap));
        }
    }
}

fn collect_mouse_hotkey(
    input: &egui::InputState,
    modifiers_bitmap: u32,
    key_recorded: &mut Option<(u32, u32)>,
) {
    for btn in [
        egui::PointerButton::Middle,
        egui::PointerButton::Extra1,
        egui::PointerButton::Extra2,
    ] {
        if input.pointer.button_pressed(btn)
            && let Some(vk) = egui_pointer_to_vk(&btn)
        {
            *key_recorded = Some((vk, modifiers_bitmap));
            break;
        }
    }
}
