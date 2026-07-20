// --- HOTKEY MODULE ---
// Hotkey registration, listener, and mouse hook.

mod processor;

pub use processor::hotkey_proc;

use crate::APP;
use crate::config::Hotkey;
use crate::win_types::{SendHhook, SendHwnd};
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

// Windows RegisterHotKey modifier bits (MOD_ALT/CONTROL/SHIFT/WIN). Canonical
// definition for the whole crate — imported by gui/app, translation_gummy, and
// the screen-record IPC hotkey parser. NOTE: distinct from the u8 wire encoding
// in screen_record/input_capture.rs.
pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;

// Message constants
pub const WM_RELOAD_HOTKEYS: u32 = WM_USER + 101;
pub const WM_UNREGISTER_HOTKEYS: u32 = WM_USER + 103;
pub const WM_REGISTER_HOTKEYS: u32 = WM_USER + 104;
pub const COMPUTER_CONTROL_HOTKEY_ID: i32 = 9700;
pub const TRANSLATION_GUMMY_HOTKEY_ID: i32 = 9800;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HotkeyRegistrationGroup {
    Preset(usize),
    ComputerControl,
    ScreenRecord,
    TranslationGummy,
}

impl HotkeyRegistrationGroup {
    fn log_name(self) -> String {
        match self {
            Self::Preset(index) => format!("preset[{index}]"),
            Self::ComputerControl => "computer_control".to_string(),
            Self::ScreenRecord => "screen_record".to_string(),
            Self::TranslationGummy => "translation_gummy".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HotkeyRegistrationFailure {
    group: HotkeyRegistrationGroup,
    registration_id: i32,
    hotkey_name: String,
    os_error: u32,
}

fn registration_outcome(
    group: HotkeyRegistrationGroup,
    registration_id: i32,
    hotkey: &Hotkey,
    os_error: Option<u32>,
) -> std::result::Result<i32, HotkeyRegistrationFailure> {
    match os_error {
        None => Ok(registration_id),
        Some(os_error) => Err(HotkeyRegistrationFailure {
            group,
            registration_id,
            hotkey_name: hotkey.name.clone(),
            os_error,
        }),
    }
}

fn register_configured_hotkey(
    hwnd: HWND,
    group: HotkeyRegistrationGroup,
    registration_id: i32,
    hotkey: &Hotkey,
) -> std::result::Result<i32, HotkeyRegistrationFailure> {
    let result = unsafe {
        RegisterHotKey(
            Some(hwnd),
            registration_id,
            HOT_KEY_MODIFIERS(hotkey.modifiers),
            hotkey.code,
        )
    };
    let os_error = result.err().map(|_| unsafe { GetLastError().0 });
    registration_outcome(group, registration_id, hotkey, os_error)
}

fn register_and_track(
    hwnd: HWND,
    group: HotkeyRegistrationGroup,
    registration_id: i32,
    hotkey: &Hotkey,
    registered_ids: &mut Vec<i32>,
) {
    match register_configured_hotkey(hwnd, group, registration_id, hotkey) {
        Ok(id) => registered_ids.push(id),
        Err(failure) => crate::log_info!(
            "[Hotkey] registration failed: group={} id={} key='{}' os_error={}",
            failure.group.log_name(),
            failure.registration_id,
            failure.hotkey_name,
            failure.os_error
        ),
    }
}

/// Global handle for the listener window (for the mouse hook to post messages to).
static LISTENER_HWND: LazyLock<Mutex<SendHwnd>> = LazyLock::new(|| Mutex::new(SendHwnd::default()));
/// Global handle for the mouse hook.
static MOUSE_HOOK: LazyLock<Mutex<SendHhook>> = LazyLock::new(|| Mutex::new(SendHhook::default()));

/// Register all hotkeys from config.
pub fn register_all_hotkeys(hwnd: HWND) {
    let mut app = APP.lock().unwrap();
    let presets = &app.config.presets;

    let mut registered_ids = Vec::new();
    for (p_idx, preset) in presets.iter().enumerate() {
        for (h_idx, hotkey) in preset.hotkeys.iter().enumerate() {
            // Skip Mouse Buttons for RegisterHotKey (handled via hook)
            if [0x04, 0x05, 0x06].contains(&hotkey.code) {
                continue;
            }

            let id = (p_idx as i32 * 1000) + (h_idx as i32) + 1;
            register_and_track(
                hwnd,
                HotkeyRegistrationGroup::Preset(p_idx),
                id,
                hotkey,
                &mut registered_ids,
            );
        }
    }

    for (idx, hotkey) in app.config.computer_control_hotkeys.iter().enumerate() {
        if idx >= 100 || [0x04, 0x05, 0x06].contains(&hotkey.code) {
            continue;
        }
        register_and_track(
            hwnd,
            HotkeyRegistrationGroup::ComputerControl,
            COMPUTER_CONTROL_HOTKEY_ID + idx as i32,
            hotkey,
            &mut registered_ids,
        );
    }

    // Register Global Screen Record Hotkeys (IDs: 9900-9999)
    for (idx, sr_hotkey) in app.config.screen_record_hotkeys.iter().enumerate() {
        if idx >= 100 {
            break;
        }
        let id = 9900 + idx as i32;
        if [0x04, 0x05, 0x06].contains(&sr_hotkey.code) {
            continue;
        }
        register_and_track(
            hwnd,
            HotkeyRegistrationGroup::ScreenRecord,
            id,
            sr_hotkey,
            &mut registered_ids,
        );
    }

    for (idx, hotkey) in app.config.translation_gummy.hotkeys.iter().enumerate() {
        if idx >= 100 || [0x04, 0x05, 0x06].contains(&hotkey.code) {
            continue;
        }
        register_and_track(
            hwnd,
            HotkeyRegistrationGroup::TranslationGummy,
            TRANSLATION_GUMMY_HOTKEY_ID + idx as i32,
            hotkey,
            &mut registered_ids,
        );
    }

    app.registered_hotkey_ids = registered_ids;
}

/// Unregister all hotkeys.
pub fn unregister_all_hotkeys(hwnd: HWND) {
    let registered_ids = {
        let mut app = APP.lock().unwrap();
        std::mem::take(&mut app.registered_hotkey_ids)
    };
    for id in registered_ids {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), id);
        }
    }
}

/// Low-Level Mouse Hook Procedure.
unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let msg = wparam.0 as u32;
            let vk_code = match msg {
                WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
                    if msg == WM_MBUTTONDOWN {
                        Some(0x04)
                    } else {
                        None
                    }
                }
                WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => None,
                WM_XBUTTONDOWN => {
                    let info = *(lparam.0 as *const MSLLHOOKSTRUCT);
                    let xbutton = (info.mouseData >> 16) & 0xFFFF;
                    if xbutton == 1 {
                        Some(0x05) // VK_XBUTTON1
                    } else if xbutton == 2 {
                        Some(0x06) // VK_XBUTTON2
                    } else {
                        None
                    }
                }
                WM_XBUTTONUP => None,
                _ => None,
            };

            if let Some(vk) = vk_code {
                // Check modifiers using GetAsyncKeyState for real-time state
                let mut mods = 0;
                if (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0 {
                    mods |= MOD_ALT;
                }
                if (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 {
                    mods |= MOD_CONTROL;
                }
                if (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 {
                    mods |= MOD_SHIFT;
                }
                if (GetAsyncKeyState(VK_LWIN.0 as i32) as u16 & 0x8000) != 0
                    || (GetAsyncKeyState(VK_RWIN.0 as i32) as u16 & 0x8000) != 0
                {
                    mods |= MOD_WIN;
                }

                // Check config for a match
                let mut found_id = None;
                if let Ok(app) = APP.lock() {
                    for (p_idx, preset) in app.config.presets.iter().enumerate() {
                        for (h_idx, hotkey) in preset.hotkeys.iter().enumerate() {
                            if hotkey.code == vk && hotkey.modifiers == mods {
                                found_id = Some((p_idx as i32 * 1000) + (h_idx as i32) + 1);
                                break;
                            }
                        }
                        if found_id.is_some() {
                            break;
                        }
                    }

                    // Check global app hotkeys.
                    if found_id.is_none() {
                        for (idx, hk) in app
                            .config
                            .computer_control_hotkeys
                            .iter()
                            .take(100)
                            .enumerate()
                        {
                            if hk.code == vk && hk.modifiers == mods {
                                found_id = Some(COMPUTER_CONTROL_HOTKEY_ID + idx as i32);
                                break;
                            }
                        }
                    }

                    if found_id.is_none() {
                        for (idx, sr_hk) in app.config.screen_record_hotkeys.iter().enumerate() {
                            if sr_hk.code == vk && sr_hk.modifiers == mods {
                                found_id = Some(9900 + idx as i32);
                                break;
                            }
                        }
                    }

                    if found_id.is_none() {
                        for (idx, hk) in app.config.translation_gummy.hotkeys.iter().enumerate() {
                            if hk.code == vk && hk.modifiers == mods {
                                found_id = Some(TRANSLATION_GUMMY_HOTKEY_ID + idx as i32);
                                break;
                            }
                        }
                    }
                }

                if let Some(id) = found_id
                    && let Ok(hwnd_target) = LISTENER_HWND.lock()
                    && !hwnd_target.0.is_invalid()
                {
                    let _ = PostMessageW(
                        Some(hwnd_target.0),
                        WM_HOTKEY,
                        WPARAM(id as usize),
                        LPARAM(0),
                    );
                    return LRESULT(1); // Consume/Block input
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}

/// Run the hotkey listener message loop.
pub fn run_hotkey_listener() {
    unsafe {
        let instance = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(_) => {
                eprintln!("Error: Failed to get module handle for hotkey listener");
                return;
            }
        };

        let class_name = w!("HotkeyListenerClass");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(hotkey_proc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };

        let _ = RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("Listener"),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        if hwnd.is_invalid() {
            eprintln!("Error: Failed to create hotkey listener window");
            return;
        }

        // Store HWND for the hook
        if let Ok(mut guard) = LISTENER_HWND.lock() {
            *guard = SendHwnd(hwnd);
        }

        // Install Mouse Hook
        if let Ok(hhook) =
            SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), Some(instance.into()), 0)
        {
            println!("DEBUG: Mouse hook installed successfully");
            if let Ok(mut hook_guard) = MOUSE_HOOK.lock() {
                *hook_guard = SendHhook(hhook);
            }
        } else {
            eprintln!("Warning: Failed to install low-level mouse hook");
        }

        // Unregister first to clear any stale registrations from previous crash
        unregister_all_hotkeys(hwnd);
        register_all_hotkeys(hwnd);

        let mut msg = MSG::default();
        loop {
            if GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_RELOAD_HOTKEYS {
                    unregister_all_hotkeys(hwnd);
                    register_all_hotkeys(hwnd);

                    if let Ok(mut app) = APP.lock() {
                        app.hotkeys_updated = false;
                    }
                } else if msg.message == WM_UNREGISTER_HOTKEYS {
                    unregister_all_hotkeys(hwnd);
                } else if msg.message == WM_REGISTER_HOTKEYS {
                    unregister_all_hotkeys(hwnd);
                    register_all_hotkeys(hwnd);
                } else {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HotkeyRegistrationFailure, HotkeyRegistrationGroup, MOD_CONTROL, registration_outcome,
    };
    use crate::config::Hotkey;

    #[test]
    fn registration_outcome_tracks_success_and_structured_failure() {
        let hotkey = Hotkey::new(0x75, "Ctrl + F6", MOD_CONTROL);

        assert_eq!(
            registration_outcome(
                HotkeyRegistrationGroup::ComputerControl,
                9700,
                &hotkey,
                None,
            ),
            Ok(9700)
        );
        assert_eq!(
            registration_outcome(
                HotkeyRegistrationGroup::ComputerControl,
                9700,
                &hotkey,
                Some(1409),
            ),
            Err(HotkeyRegistrationFailure {
                group: HotkeyRegistrationGroup::ComputerControl,
                registration_id: 9700,
                hotkey_name: "Ctrl + F6".to_string(),
                os_error: 1409,
            })
        );
    }

    #[test]
    fn every_registration_group_has_a_stable_log_name() {
        let cases = [
            (HotkeyRegistrationGroup::Preset(3), "preset[3]"),
            (HotkeyRegistrationGroup::ComputerControl, "computer_control"),
            (HotkeyRegistrationGroup::ScreenRecord, "screen_record"),
            (
                HotkeyRegistrationGroup::TranslationGummy,
                "translation_gummy",
            ),
        ];

        for (group, expected) in cases {
            assert_eq!(group.log_name(), expected);
        }
    }
}
