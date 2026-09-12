//! Canonical Windows virtual-key labels and modifier ordering.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, HKL, MAPVK_VK_TO_CHAR, MapVirtualKeyExW,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};

pub(crate) fn input_layout() -> HKL {
    unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        GetKeyboardLayout(thread)
    }
}

pub(crate) fn format(code: u32, modifiers: u32) -> String {
    with_modifiers(&key_name(code), modifiers)
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn with_escape(shortcut: &str) -> String {
    if shortcut.is_empty() || shortcut.eq_ignore_ascii_case("Esc") {
        key_name(0x1B)
    } else {
        format!("{} / {shortcut}", key_name(0x1B))
    }
}

pub(crate) fn with_modifiers(key: &str, modifiers: u32) -> String {
    let mut parts = Vec::with_capacity(5);
    for (flag, label) in [
        (MOD_CONTROL, "Ctrl"),
        (MOD_ALT, "Alt"),
        (MOD_SHIFT, "Shift"),
        (MOD_WIN, "Win"),
    ] {
        if modifiers & flag != 0 {
            parts.push(label);
        }
    }
    parts.push(key);
    parts.join(" + ")
}

pub(crate) fn is_modifier(code: u32) -> bool {
    matches!(code, 0x10..=0x12 | 0x5B..=0x5C | 0xA0..=0xA5)
}

pub(crate) fn key_name(code: u32) -> String {
    key_name_for_layout(code, input_layout())
}

pub(crate) fn key_name_for_layout(code: u32, layout: HKL) -> String {
    match code {
        0x30..=0x39 | 0x41..=0x5A => return char::from_u32(code).unwrap().to_string(),
        0x60..=0x69 => return format!("Numpad {}", code - 0x60),
        0x70..=0x87 => return format!("F{}", code - 0x6F),
        // OEM punctuation belongs to the active input layout, not a US-only table.
        0xBA..=0xC0 | 0xDB..=0xDF | 0xE1..=0xE2 => {
            let character = unsafe { MapVirtualKeyExW(code, MAPVK_VK_TO_CHAR, Some(layout)) };
            if let Some(character) = char::from_u32(character & 0x7FFF_FFFF)
                && !character.is_control()
                && !character.is_whitespace()
            {
                return character.to_string();
            }
        }
        _ => {}
    }
    let label = match code {
        0x01 => "Left Click",
        0x02 => "Right Click",
        0x03 => "Cancel",
        0x04 => "Middle Click",
        0x05 => "Mouse Back",
        0x06 => "Mouse Forward",
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0C => "Clear",
        0x0D => "Enter",
        0x10 | 0xA0 | 0xA1 => "Shift",
        0x11 | 0xA2 | 0xA3 => "Ctrl",
        0x12 | 0xA4 | 0xA5 => "Alt",
        0x13 => "Pause",
        0x14 => "Caps Lock",
        0x15 => "IME Kana/Hangul",
        0x16 => "IME On",
        0x17 => "IME Junja",
        0x18 => "IME Final",
        0x19 => "IME Kanji/Hanja",
        0x1A => "IME Off",
        0x1B => "Esc",
        0x1C => "IME Convert",
        0x1D => "IME Nonconvert",
        0x1E => "IME Accept",
        0x1F => "IME Mode Change",
        0x20 => "Space",
        0x21 => "Page Up",
        0x22 => "Page Down",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x29 => "Select",
        0x2A => "Print",
        0x2B => "Execute",
        0x2C => "Print Screen",
        0x2D => "Insert",
        0x2E => "Delete",
        0x2F => "Help",
        0x5B | 0x5C => "Win",
        0x5D => "Menu",
        0x5F => "Sleep",
        0x6A => "Numpad Multiply",
        0x6B => "Numpad Add",
        0x6C => "Numpad Separator",
        0x6D => "Numpad Subtract",
        0x6E => "Numpad Decimal",
        0x6F => "Numpad Divide",
        0x90 => "Num Lock",
        0x91 => "Scroll Lock",
        0xA6 => "Browser Back",
        0xA7 => "Browser Forward",
        0xA8 => "Browser Refresh",
        0xA9 => "Browser Stop",
        0xAA => "Browser Search",
        0xAB => "Browser Favorites",
        0xAC => "Browser Home",
        0xAD => "Volume Mute",
        0xAE => "Volume Down",
        0xAF => "Volume Up",
        0xB0 => "Media Next",
        0xB1 => "Media Previous",
        0xB2 => "Media Stop",
        0xB3 => "Media Play/Pause",
        0xB4 => "Launch Mail",
        0xB5 => "Launch Media",
        0xB6 => "Launch App 1",
        0xB7 => "Launch App 2",
        0xE5 => "IME Process",
        0xE7 => "Unicode Packet",
        0xF6 => "Attn",
        0xF7 => "CrSel",
        0xF8 => "ExSel",
        0xF9 => "Erase EOF",
        0xFA => "Play",
        0xFB => "Zoom",
        0xFC => "No Name",
        0xFD => "PA1",
        0xFE => "OEM Clear",
        _ => return format!("VK 0x{code:02X}"),
    };
    label.to_string()
}

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;
