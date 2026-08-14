//! Shared conversion for keyboard events sent by WebView mini apps.

use crate::config::Hotkey;

use super::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};

pub(crate) fn from_web_event(
    key: &str,
    code: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
) -> Option<Hotkey> {
    let key_name = normalize_key_name(key, code)?;
    let vk = map_virtual_key(code, key)?;
    let modifiers = (if ctrl { MOD_CONTROL } else { 0 })
        | (if alt { MOD_ALT } else { 0 })
        | (if shift { MOD_SHIFT } else { 0 })
        | (if meta { MOD_WIN } else { 0 });

    Some(Hotkey {
        code: vk,
        name: label(modifiers, &key_name),
        modifiers,
    })
}

pub(crate) fn label(modifiers: u32, key_name: &str) -> String {
    let mut parts = Vec::new();
    if modifiers & MOD_CONTROL != 0 {
        parts.push("Ctrl");
    }
    if modifiers & MOD_ALT != 0 {
        parts.push("Alt");
    }
    if modifiers & MOD_SHIFT != 0 {
        parts.push("Shift");
    }
    if modifiers & MOD_WIN != 0 {
        parts.push("Win");
    }
    parts.push(key_name);
    parts.join("+")
}

fn normalize_key_name(key: &str, code: &str) -> Option<String> {
    let code = code.trim();
    if let Some(letter) = code.strip_prefix("Key") {
        return Some(letter.to_string());
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        return Some(digit.to_string());
    }
    if let Some(function) = code.strip_prefix('F')
        && function.parse::<u8>().is_ok()
    {
        return Some(code.to_string());
    }

    match code {
        "Space" => Some("Space".to_string()),
        "Minus" => Some("-".to_string()),
        "Equal" => Some("=".to_string()),
        "BracketLeft" => Some("[".to_string()),
        "BracketRight" => Some("]".to_string()),
        "Backslash" => Some("\\".to_string()),
        "Semicolon" => Some(";".to_string()),
        "Quote" => Some("'".to_string()),
        "Comma" => Some(",".to_string()),
        "Period" => Some(".".to_string()),
        "Slash" => Some("/".to_string()),
        "Backquote" => Some("`".to_string()),
        "Escape" => Some("Esc".to_string()),
        "Tab" => Some("Tab".to_string()),
        "Enter" => Some("Enter".to_string()),
        "ArrowUp" => Some("Up".to_string()),
        "ArrowDown" => Some("Down".to_string()),
        "ArrowLeft" => Some("Left".to_string()),
        "ArrowRight" => Some("Right".to_string()),
        "Insert" => Some("Insert".to_string()),
        "Delete" => Some("Delete".to_string()),
        "Home" => Some("Home".to_string()),
        "End" => Some("End".to_string()),
        "PageUp" => Some("PageUp".to_string()),
        "PageDown" => Some("PageDown".to_string()),
        _ => match key {
            "Control" | "Shift" | "Alt" | "Meta" => None,
            _ if key.chars().count() == 1 => Some(key.to_uppercase()),
            _ => None,
        },
    }
}

fn map_virtual_key(code: &str, key: &str) -> Option<u32> {
    if let Some(letter) = code.strip_prefix("Key") {
        return letter.as_bytes().first().copied().map(u32::from);
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        return digit.as_bytes().first().copied().map(u32::from);
    }
    if let Some(number) = code.strip_prefix('F')
        && let Ok(index) = number.parse::<u32>()
    {
        return (1..=24).contains(&index).then_some(111 + index);
    }

    Some(match code {
        "Space" => 0x20,
        "Tab" => 0x09,
        "Enter" => 0x0D,
        "Escape" => 0x1B,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "Insert" => 0x2D,
        "Delete" => 0x2E,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "Minus" => 0xBD,
        "Equal" => 0xBB,
        "BracketLeft" => 0xDB,
        "BracketRight" => 0xDD,
        "Backslash" => 0xDC,
        "Semicolon" => 0xBA,
        "Quote" => 0xDE,
        "Comma" => 0xBC,
        "Period" => 0xBE,
        "Slash" => 0xBF,
        "Backquote" => 0xC0,
        _ => {
            return normalize_key_name(key, code)
                .and_then(|name| name.as_bytes().first().copied())
                .map(u32::from);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::from_web_event;
    use crate::hotkey::{MOD_CONTROL, MOD_SHIFT};

    #[test]
    fn web_event_maps_to_windows_binding_and_canonical_label() {
        let hotkey = from_web_event("k", "KeyK", true, false, true, false).unwrap();
        assert_eq!(hotkey.code, 0x4B);
        assert_eq!(hotkey.modifiers, MOD_CONTROL | MOD_SHIFT);
        assert_eq!(hotkey.name, "Ctrl+Shift+K");
    }

    #[test]
    fn modifier_only_and_out_of_range_function_keys_are_rejected() {
        assert!(from_web_event("Control", "ControlLeft", true, false, false, false).is_none());
        assert!(from_web_event("F25", "F25", false, false, false, false).is_none());
    }
}
