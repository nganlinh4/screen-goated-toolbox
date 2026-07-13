// --- HOTKEY MODULE ---
// Hotkey registration, listener, and mouse hook.

mod processor;

pub use processor::hotkey_proc;

use crate::APP;
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
pub const TRANSLATION_GUMMY_HOTKEY_ID: i32 = 9800;

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
            unsafe {
                let res = RegisterHotKey(
                    Some(hwnd),
                    id,
                    HOT_KEY_MODIFIERS(hotkey.modifiers),
                    hotkey.code,
                );
                if res.is_err() {
                    let err_code = GetLastError().0;
                    crate::log_info!(
                        "[Hotkey] COLLISION: Failed to register hotkey '{}' for preset {}, ID {}. Error Code: {}",
                        hotkey.name,
                        p_idx,
                        id,
                        err_code
                    );
                } else {
                    registered_ids.push(id);
                }
            }
        }
    }
    app.registered_hotkey_ids = registered_ids;

    // Register Global Screen Record Hotkeys (IDs: 9900-9999)
    for (idx, sr_hotkey) in app.config.screen_record_hotkeys.iter().enumerate() {
        if idx >= 100 {
            break;
        }
        let id = 9900 + idx as i32;
        unsafe {
            let _ = RegisterHotKey(
                Some(hwnd),
                id,
                HOT_KEY_MODIFIERS(sr_hotkey.modifiers),
                sr_hotkey.code,
            );
        }
    }

    for (idx, hotkey) in app.config.translation_gummy.hotkeys.iter().enumerate() {
        if idx >= 100 || [0x04, 0x05, 0x06].contains(&hotkey.code) {
            continue;
        }
        unsafe {
            let _ = RegisterHotKey(
                Some(hwnd),
                TRANSLATION_GUMMY_HOTKEY_ID + idx as i32,
                HOT_KEY_MODIFIERS(hotkey.modifiers),
                hotkey.code,
            );
        }
    }
}

/// Unregister all hotkeys.
pub fn unregister_all_hotkeys(hwnd: HWND) {
    let app = APP.lock().unwrap();
    for &id in &app.registered_hotkey_ids {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), id);
        }
    }
    // Unregister Global SR Hotkeys
    for idx in 0..100 {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), 9900 + idx);
        }
    }
    // Unregister Translation Gummy Hotkeys
    for idx in 0..100 {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), TRANSLATION_GUMMY_HOTKEY_ID + idx);
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

                    // Check Global Screen Record Hotkeys
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
