// --- HOTKEY MANAGEMENT ---
// Hotkey registration/unregistration, JS key-code to VK mapping,
// and hotkey reload signaling via the hidden listener window.

use crate::APP;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_RELOAD_HOTKEYS: u32 = WM_USER + 101;
const WM_UNREGISTER_HOTKEYS: u32 = WM_USER + 103;
const WM_REGISTER_HOTKEYS: u32 = WM_USER + 104;

use crate::hotkey::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};

pub(super) fn trigger_hotkey_reload() {
    unsafe {
        if let Ok(hwnd) = FindWindowW(
            windows::core::w!("HotkeyListenerClass"),
            windows::core::w!("Listener"),
        ) && !hwnd.is_invalid()
        {
            let _ = PostMessageW(Some(hwnd), WM_RELOAD_HOTKEYS, WPARAM(0), LPARAM(0));
        }
    }
}

pub(super) fn handle_get_hotkeys() -> Result<serde_json::Value, String> {
    let app = APP.lock().unwrap();
    Ok(serde_json::to_value(&app.config.screen_record_hotkeys).unwrap())
}

pub(super) fn handle_remove_hotkey(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let index = args["index"].as_u64().ok_or("Missing index")? as usize;
    {
        let mut app = APP.lock().unwrap();
        if index < app.config.screen_record_hotkeys.len() {
            app.config.screen_record_hotkeys.remove(index);
            crate::config::save_config(&app.config);
        }
    }
    trigger_hotkey_reload();
    Ok(serde_json::Value::Null)
}

pub(super) fn handle_set_hotkey(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let code_str = args["code"].as_str().ok_or("Missing code")?;
    let mods_arr = args["modifiers"].as_array().ok_or("Missing modifiers")?;
    let key = args["key"].as_str().unwrap_or("");

    let mut modifiers = 0;
    for m in mods_arr {
        match m.as_str() {
            Some("Control") => modifiers |= MOD_CONTROL,
            Some("Alt") => modifiers |= MOD_ALT,
            Some("Shift") => modifiers |= MOD_SHIFT,
            Some("Meta") => modifiers |= MOD_WIN,
            _ => {}
        }
    }

    let hotkey = crate::hotkey::web_binding::from_web_event(
        key,
        code_str,
        modifiers & MOD_CONTROL != 0,
        modifiers & MOD_ALT != 0,
        modifiers & MOD_SHIFT != 0,
        modifiers & MOD_WIN != 0,
    )
    .ok_or_else(|| format!("Unsupported key code: {code_str}"))?;

    {
        let app = APP.lock().unwrap();
        if let Some(conflict) =
            app.config
                .check_hotkey_conflict(hotkey.code, hotkey.modifiers, None)
        {
            let text = crate::gui::locale::LocaleText::get(&app.config.ui_language);
            return Err(text.hotkey_conflict_message(&conflict));
        }
    }

    {
        let mut app = APP.lock().unwrap();
        app.config.screen_record_hotkeys.push(hotkey.clone());
        crate::config::save_config(&app.config);
    }

    trigger_hotkey_reload();

    Ok(serde_json::to_value(&hotkey).unwrap())
}

pub(super) fn handle_unregister_hotkeys() -> Result<serde_json::Value, String> {
    unsafe {
        if let Ok(hwnd) = FindWindowW(
            windows::core::w!("HotkeyListenerClass"),
            windows::core::w!("Listener"),
        ) && !hwnd.is_invalid()
        {
            let _ = PostMessageW(Some(hwnd), WM_UNREGISTER_HOTKEYS, WPARAM(0), LPARAM(0));
        }
    }
    Ok(serde_json::Value::Null)
}

pub(super) fn handle_register_hotkeys() -> Result<serde_json::Value, String> {
    unsafe {
        if let Ok(hwnd) = FindWindowW(
            windows::core::w!("HotkeyListenerClass"),
            windows::core::w!("Listener"),
        ) && !hwnd.is_invalid()
        {
            let _ = PostMessageW(Some(hwnd), WM_REGISTER_HOTKEYS, WPARAM(0), LPARAM(0));
        }
    }
    Ok(serde_json::Value::Null)
}
