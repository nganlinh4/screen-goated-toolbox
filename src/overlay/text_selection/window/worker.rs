use super::super::clipboard::{get_clipboard_text, process_selected_text};
use super::super::state::*;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub(super) fn spawn_worker_thread(hwnd_val: usize, preset_idx_for_thread: usize) {
    std::thread::spawn(move || unsafe {
        worker_thread(hwnd_val, preset_idx_for_thread);
    });
}

/// Worker thread for processing text selection.
unsafe fn worker_thread(hwnd_val: usize, preset_idx_for_thread: usize) {
    unsafe {
        let hwnd_copy = HWND(hwnd_val as *mut std::ffi::c_void);

        if TAG_ABORT_SIGNAL.load(Ordering::Relaxed) || !TEXT_BADGE_VISIBLE.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));

        if OpenClipboard(Some(HWND::default())).is_ok() {
            let _ = EmptyClipboard();
            let _ = CloseClipboard();
        }

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
        std::thread::sleep(std::time::Duration::from_millis(20));
        send_input_event(0x43, KEYBD_EVENT_FLAGS(0));
        std::thread::sleep(std::time::Duration::from_millis(20));
        send_input_event(0x43, KEYEVENTF_KEYUP);
        std::thread::sleep(std::time::Duration::from_millis(20));
        send_input_event(VK_CONTROL.0, KEYEVENTF_KEYUP);

        let mut clipboard_text = String::new();
        for _ in 0..10 {
            if TAG_ABORT_SIGNAL.load(Ordering::Relaxed)
                || !TEXT_BADGE_VISIBLE.load(Ordering::Relaxed)
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
            clipboard_text = get_clipboard_text();
            if !clipboard_text.is_empty() {
                break;
            }
        }

        if !clipboard_text.trim().is_empty()
            && !TAG_ABORT_SIGNAL.load(Ordering::Relaxed)
            && TEXT_BADGE_VISIBLE.load(Ordering::Relaxed)
        {
            let _ = PostMessageW(Some(hwnd_copy), WM_APP_HIDE, WPARAM(0), LPARAM(0));

            let mut preset_idx = preset_idx_for_thread;

            let continuous_active_before = crate::overlay::continuous_mode::is_active();
            let session_flag = CONTINUOUS_ACTIVATED_THIS_SESSION.load(Ordering::SeqCst);

            if !continuous_active_before && !session_flag {
                maybe_activate_continuous_mode(&mut preset_idx);
            }

            let continuous_active = crate::overlay::continuous_mode::is_active();
            let continuous_idx = crate::overlay::continuous_mode::get_preset_idx();
            if continuous_active && continuous_idx == preset_idx {
                let retrigger_idx = preset_idx;
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    if crate::overlay::continuous_mode::is_active() {
                        super::super::show_text_selection_tag(retrigger_idx);
                    }
                });
            }

            process_selected_text(preset_idx, clipboard_text);
        }

        let mut state = SELECTION_STATE.lock().unwrap();
        state.is_selecting = false;
        state.is_processing = false;
    }
}

fn maybe_activate_continuous_mode(preset_idx: &mut usize) {
    let trigger_modifiers = unsafe { TRIGGER_MODIFIERS };
    let mut held = if trigger_modifiers == 0 {
        IS_HOTKEY_HELD.load(Ordering::SeqCst)
    } else {
        crate::overlay::continuous_mode::are_modifiers_still_held()
    };

    if !held {
        held = crate::overlay::continuous_mode::was_triggered_recently(1500);
    }
    if !held {
        return;
    }

    let mut hotkey_name = crate::overlay::continuous_mode::get_hotkey_name();

    let dbg_latest = crate::overlay::continuous_mode::get_latest_hotkey_name();
    crate::log_info!(
        "[TextSelection] Late Check - Persistent: '{}', Latest: '{}'",
        hotkey_name,
        dbg_latest
    );

    if hotkey_name.is_empty() {
        hotkey_name = dbg_latest;
    }
    if hotkey_name.is_empty() {
        hotkey_name = "Hotkey".to_string();
    }

    let preset_name = {
        if let Ok(app) = APP.lock() {
            app.config
                .presets
                .get(*preset_idx)
                .map(|preset| preset.id.clone())
                .unwrap_or_default()
        } else {
            "Preset".to_string()
        }
    };

    let current_active_idx = crate::overlay::continuous_mode::get_preset_idx();
    if current_active_idx != *preset_idx {
        *preset_idx = current_active_idx;
    }
    crate::overlay::continuous_mode::activate(*preset_idx, hotkey_name.clone());
    crate::overlay::continuous_mode::show_activation_notification(&preset_name, &hotkey_name);
    CONTINUOUS_ACTIVATED_THIS_SESSION.store(true, Ordering::SeqCst);
    super::super::update_badge_for_continuous_mode();
}
