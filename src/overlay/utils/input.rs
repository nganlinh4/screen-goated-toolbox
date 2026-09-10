//! Input simulation: paste (Ctrl+V) and type text to target windows.

use super::{is_text_input_focused, warn_no_caret};
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Returns the foreground window to attempt paste on.
/// We don't filter by caret here anymore, deferring the check to force_focus_and_paste
/// where we use more robust UI Automation.
pub fn get_target_window_for_paste() -> Option<HWND> {
    unsafe {
        let hwnd_foreground = GetForegroundWindow();
        if hwnd_foreground.is_invalid() {
            return None;
        }
        Some(hwnd_foreground)
    }
}

pub fn force_focus_and_paste(hwnd_target: HWND) {
    unsafe {
        // 1. Force focus back to the target window
        if IsWindow(Some(hwnd_target)).as_bool() {
            let cur_thread = GetCurrentThreadId();
            let target_thread = GetWindowThreadProcessId(hwnd_target, None);

            if cur_thread != target_thread {
                let _ = AttachThreadInput(cur_thread, target_thread, true);
                let _ = SetForegroundWindow(hwnd_target);
                // Important: Bring window to top so it receives input
                let _ = BringWindowToTop(hwnd_target);
                let _ = SetFocus(Some(hwnd_target));
                let _ = AttachThreadInput(cur_thread, target_thread, false);
            } else {
                let _ = SetForegroundWindow(hwnd_target);
            }
        } else {
            return;
        }

        // 2. Wait for focus to settle
        std::thread::sleep(std::time::Duration::from_millis(350));

        // 2.5 Warn if no writable area detected (but never block — detection can miss valid targets)
        if !is_text_input_focused() {
            warn_no_caret();
            // Don't return — proceed with paste anyway since detection can be wrong
        }

        // 3. CLEANUP MODIFIERS SMARTLY
        // Only send KeyUp if the key is actually physically pressed to avoid side effects
        let release_if_pressed = |vk: u16| {
            let state = GetAsyncKeyState(vk as i32);
            if (state as u16 & 0x8000) != 0 {
                let input = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(vk),
                            dwFlags: KEYEVENTF_KEYUP,
                            ..Default::default()
                        },
                    },
                };
                SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            }
        };

        release_if_pressed(VK_MENU.0); // Alt
        release_if_pressed(VK_SHIFT.0); // Shift
        release_if_pressed(VK_LWIN.0); // Win Left
        release_if_pressed(VK_RWIN.0); // Win Right
        release_if_pressed(VK_CONTROL.0); // Ctrl

        std::thread::sleep(std::time::Duration::from_millis(50));

        // 4. Send Ctrl+V Sequence
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

        // Ctrl Down
        send_input_event(VK_CONTROL.0, KEYBD_EVENT_FLAGS(0));
        std::thread::sleep(std::time::Duration::from_millis(50));

        // V Down
        send_input_event(VK_V.0, KEYBD_EVENT_FLAGS(0));
        std::thread::sleep(std::time::Duration::from_millis(50));

        // V Up
        send_input_event(VK_V.0, KEYEVENTF_KEYUP);
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Ctrl Up
        send_input_event(VK_CONTROL.0, KEYEVENTF_KEYUP);
    }
}
