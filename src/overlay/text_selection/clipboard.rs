// --- TEXT SELECTION CLIPBOARD & PROCESSING ---
// Clipboard operations, text processing, and keyboard hook.

use super::state::*;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Get text from clipboard
pub unsafe fn get_clipboard_text() -> String {
    let mut result = String::new();
    if OpenClipboard(Some(HWND::default())).is_ok() {
        if let Ok(h_data) = GetClipboardData(13u32) {
            let h_global: HGLOBAL = std::mem::transmute(h_data);
            let ptr = GlobalLock(h_global);
            if !ptr.is_null() {
                let size = GlobalSize(h_global);
                let wide_slice = std::slice::from_raw_parts(ptr as *const u16, size / 2);
                if let Some(end) = wide_slice.iter().position(|&c| c == 0) {
                    result = String::from_utf16_lossy(&wide_slice[..end]);
                }
            }
            let _ = GlobalUnlock(h_global);
        }
        let _ = CloseClipboard();
    }
    result
}

/// Process selected text with the given preset
pub fn process_selected_text(preset_idx: usize, clipboard_text: String) {
    unsafe {
        let (is_master, _original_mode) = {
            let app = APP.lock().unwrap();
            let p = &app.config.presets[preset_idx];
            (p.is_master, p.text_input_mode.clone())
        };

        let final_preset_idx = if is_master {
            let mut cursor_pos = POINT { x: 0, y: 0 };
            let _ = GetCursorPos(&mut cursor_pos);
            let selected =
                crate::overlay::preset_wheel::show_preset_wheel("text", Some("select"), cursor_pos);
            if let Some(idx) = selected {
                idx
            } else {
                return;
            }
        } else {
            preset_idx
        };

        let (config, mut preset, screen_w, screen_h) = {
            let mut app = APP.lock().unwrap();
            app.config.active_preset_idx = final_preset_idx;
            (
                app.config.clone(),
                app.config.presets[final_preset_idx].clone(),
                GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN),
            )
        };

        preset.text_input_mode = "select".to_string();

        let center_rect = RECT {
            left: (screen_w - 700) / 2,
            top: (screen_h - 300) / 2,
            right: (screen_w + 700) / 2,
            bottom: (screen_h + 300) / 2,
        };
        let localized_name =
            crate::gui::settings_ui::get_localized_preset_name(&preset.id, &config.ui_language);
        let cancel_hotkey = preset
            .hotkeys
            .first()
            .map(|h| h.name.clone())
            .unwrap_or_default();

        crate::overlay::process::start_text_processing(
            clipboard_text,
            center_rect,
            config,
            preset,
            localized_name,
            cancel_hotkey,
        );
    }
}

/// Try to process already-selected text instantly
pub fn try_instant_process(preset_idx: usize) -> bool {
    // TIME-BASED DEBOUNCE: If we processed via instant process recently, skip
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let last_time = LAST_INSTANT_PROCESS_TIME.load(Ordering::SeqCst);
    if last_time > 0 && now - last_time < 2000 {
        return false;
    }

    // Set processing flag early to block other threads
    let _guard = {
        let mut state = SELECTION_STATE.lock().unwrap();
        if state.is_processing {
            return false;
        }
        state.is_processing = true;
        ProcessingGuard
    };

    // Update timestamp now that we're committed to processing
    LAST_INSTANT_PROCESS_TIME.store(now, Ordering::SeqCst);

    unsafe {
        // Step 1: Save clipboard
        let original_clipboard = get_clipboard_text();

        // Step 2: Clear & Copy
        if OpenClipboard(Some(HWND::default())).is_ok() {
            let _ = EmptyClipboard();
            let _ = CloseClipboard();
        }
        std::thread::sleep(std::time::Duration::from_millis(30));

        let send_input_event = |vk: u16, flags: KEYBD_EVENT_FLAGS| {
            let input = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                        wScan: 0,
                    },
                },
            };
            SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        };

        send_input_event(VK_CONTROL.0, KEYBD_EVENT_FLAGS(0));
        std::thread::sleep(std::time::Duration::from_millis(15));
        send_input_event(0x43, KEYBD_EVENT_FLAGS(0)); // 'C'
        std::thread::sleep(std::time::Duration::from_millis(15));
        send_input_event(0x43, KEYEVENTF_KEYUP);
        std::thread::sleep(std::time::Duration::from_millis(15));
        send_input_event(VK_CONTROL.0, KEYEVENTF_KEYUP);

        // Step 3: Wait & Check
        let mut clipboard_text = String::new();
        for _ in 0..6 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            clipboard_text = get_clipboard_text();
            if !clipboard_text.is_empty() {
                break;
            }
        }

        if clipboard_text.trim().is_empty() {
            if !original_clipboard.is_empty() {
                crate::overlay::utils::copy_to_clipboard(&original_clipboard, HWND::default());
            }
            return false;
        }

        // HIDE BADGE BEFORE PROCESSING
        super::cancel_selection();

        // CONTINUOUS MODE SUPPORT for instant process
        let mut final_preset_idx = preset_idx;
        if !crate::overlay::continuous_mode::is_active() {
            let heartbeat_held = crate::overlay::continuous_mode::was_triggered_recently(1500);
            let physically_held = crate::overlay::continuous_mode::are_modifiers_still_held();

            if heartbeat_held && physically_held {
                let persistent_name = crate::overlay::continuous_mode::get_hotkey_name();
                let latest_name = crate::overlay::continuous_mode::get_latest_hotkey_name();
                crate::log_info!(
                    "[TextSelection] Hotkey Resolution - Persistent: '{}', Latest: '{}'",
                    persistent_name,
                    latest_name
                );

                let mut hotkey_name = persistent_name;
                if hotkey_name.is_empty() {
                    hotkey_name = latest_name;
                }
                if hotkey_name.is_empty() {
                    hotkey_name = "Hotkey".to_string();
                }
                let preset_name = {
                    if let Ok(app) = APP.lock() {
                        app.config
                            .presets
                            .get(preset_idx)
                            .map(|p| p.id.clone())
                            .unwrap_or_default()
                    } else {
                        "Preset".to_string()
                    }
                };

                // Disable continuous mode for Master Preset
                if preset_name != "preset_text_select_master" {
                    crate::overlay::continuous_mode::activate(preset_idx, hotkey_name.clone());
                    crate::overlay::continuous_mode::show_activation_notification(
                        &preset_name,
                        &hotkey_name,
                    );
                    super::update_badge_for_continuous_mode();
                }
            }
        }

        // Continuous mode retrigger
        if crate::overlay::continuous_mode::is_active() {
            let current_idx = crate::overlay::continuous_mode::get_preset_idx();
            if current_idx == preset_idx {
                final_preset_idx = current_idx;
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    if crate::overlay::continuous_mode::is_active() {
                        let _ = super::show_text_selection_tag(current_idx);
                    }
                });
            }
        }

        process_selected_text(final_preset_idx, clipboard_text);
        true
    }
}

/// Keyboard hook procedure for ESC and hotkey tracking
pub unsafe extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kbd_struct = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize {
            // ESC always exits continuous mode
            if kbd_struct.vkCode == VK_ESCAPE.0 as u32 {
                crate::overlay::continuous_mode::deactivate();
                crate::overlay::image_continuous_mode::exit();
                super::cancel_selection();
                TAG_ABORT_SIGNAL.store(true, Ordering::SeqCst);
                return LRESULT(1);
            }
            // Track trigger key held state
            if kbd_struct.vkCode == TRIGGER_VK_CODE && TRIGGER_VK_CODE != 0 {
                IS_HOTKEY_HELD.store(true, Ordering::SeqCst);
            }
        } else if wparam.0 == WM_KEYUP as usize || wparam.0 == WM_SYSKEYUP as usize {
            if kbd_struct.vkCode == TRIGGER_VK_CODE {
                IS_HOTKEY_HELD.store(false, Ordering::SeqCst);
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
