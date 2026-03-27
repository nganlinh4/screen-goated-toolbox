// --- HOTKEY MANAGEMENT ---
// Hotkey registration/unregistration, JS key-code to VK mapping,
// and hotkey reload signaling via the hidden listener window.

use crate::APP;
use crate::config::Hotkey;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_RELOAD_HOTKEYS: u32 = WM_USER + 101;
const WM_UNREGISTER_HOTKEYS: u32 = WM_USER + 103;
const WM_REGISTER_HOTKEYS: u32 = WM_USER + 104;

const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_WIN: u32 = 0x0008;

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

pub(super) fn js_code_to_vk(code: &str) -> Option<u32> {
    match code {
        c if c.starts_with("Key") => {
            let chars: Vec<char> = c.chars().collect();
            if chars.len() == 4 {
                Some(chars[3] as u32)
            } else {
                None
            }
        }
        c if c.starts_with("Digit") => {
            let chars: Vec<char> = c.chars().collect();
            if chars.len() == 6 {
                Some(chars[5] as u32)
            } else {
                None
            }
        }
        c if c.starts_with("F") && c.len() <= 3 => c[1..].parse::<u32>().ok().map(|n| 0x70 + n - 1),
        "Space" => Some(0x20),
        "Enter" => Some(0x0D),
        "Escape" => Some(0x1B),
        "Backspace" => Some(0x08),
        "Tab" => Some(0x09),
        "Delete" => Some(0x2E),
        "Insert" => Some(0x2D),
        "Home" => Some(0x24),
        "End" => Some(0x23),
        "PageUp" => Some(0x21),
        "PageDown" => Some(0x22),
        "ArrowUp" => Some(0x26),
        "ArrowDown" => Some(0x28),
        "ArrowLeft" => Some(0x25),
        "ArrowRight" => Some(0x27),
        "Backquote" => Some(0xC0),
        "Minus" => Some(0xBD),
        "Equal" => Some(0xBB),
        "BracketLeft" => Some(0xDB),
        "BracketRight" => Some(0xDD),
        "Backslash" => Some(0xDC),
        "Semicolon" => Some(0xBA),
        "Quote" => Some(0xDE),
        "Comma" => Some(0xBC),
        "Period" => Some(0xBE),
        "Slash" => Some(0xBF),
        c if c.starts_with("Numpad") => {
            let chars: Vec<char> = c.chars().collect();
            if chars.len() == 7 {
                Some(chars[6] as u32 + 0x30)
            } else {
                None
            }
        }
        _ => None,
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
    let key_name = args["key"].as_str().unwrap_or("Unknown");

    let vk_code = js_code_to_vk(code_str).ok_or(format!("Unsupported key code: {}", code_str))?;

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

    {
        let app = APP.lock().unwrap();
        if let Some(msg) = app.config.check_hotkey_conflict(vk_code, modifiers, None) {
            return Err(msg);
        }
    }

    let mut name_parts = Vec::new();
    if (modifiers & MOD_CONTROL) != 0 {
        name_parts.push("Ctrl");
    }
    if (modifiers & MOD_ALT) != 0 {
        name_parts.push("Alt");
    }
    if (modifiers & MOD_SHIFT) != 0 {
        name_parts.push("Shift");
    }
    if (modifiers & MOD_WIN) != 0 {
        name_parts.push("Win");
    }

    let formatted_key = if key_name.len() == 1 {
        key_name.to_uppercase()
    } else {
        match key_name {
            " " => "Space".to_string(),
            _ => key_name.to_string(),
        }
    };
    name_parts.push(&formatted_key);

    let hotkey = Hotkey {
        code: vk_code,
        modifiers,
        name: name_parts.join(" + "),
    };

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
