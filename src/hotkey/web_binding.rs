//! DOM input adapters. Binding labels are owned by `names`.

use super::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, names};
use crate::config::Hotkey;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HKL, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW, VkKeyScanExW,
};

pub(crate) fn from_web_event(
    key: &str,
    code: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
) -> Option<Hotkey> {
    let modifiers = (if ctrl { MOD_CONTROL } else { 0 })
        | (if alt { MOD_ALT } else { 0 })
        | (if shift { MOD_SHIFT } else { 0 })
        | (if meta { MOD_WIN } else { 0 });
    let layout = names::input_layout();
    let vk = if code.is_empty() || code == "Unidentified" {
        logical_key(key, modifiers, layout)?
    } else {
        let code_vk = code_to_vk(code, layout)?;
        if matches!(code_vk, 0x60..=0x69 | 0x6E)
            && matches!(
                key,
                "Insert"
                    | "Delete"
                    | "End"
                    | "PageDown"
                    | "Clear"
                    | "Home"
                    | "PageUp"
                    | "ArrowDown"
                    | "ArrowLeft"
                    | "ArrowRight"
                    | "ArrowUp"
            )
        {
            named_key(key)?
        } else {
            code_vk
        }
    };
    (!names::is_modifier(vk)).then(|| Hotkey::new(vk, modifiers))
}

fn logical_key(key: &str, modifiers: u32, layout: HKL) -> Option<u32> {
    if let Some(vk) = named_key(key) {
        return Some(vk);
    }
    let mut chars = key.chars();
    let character = chars.next()?;
    if chars.next().is_some() || character.len_utf16() != 1 {
        return None;
    }
    let mapping = unsafe { VkKeyScanExW(character as u16, layout) };
    if mapping == -1 {
        return None;
    }
    let required = ((mapping as u16) >> 8) as u32;
    let required_modifiers = (if required & 1 != 0 { MOD_SHIFT } else { 0 })
        | (if required & 2 != 0 { MOD_CONTROL } else { 0 })
        | (if required & 4 != 0 { MOD_ALT } else { 0 });
    (required & !7 == 0 && modifiers & required_modifiers == required_modifiers)
        .then_some((mapping as u16 & 0xFF) as u32)
}

fn code_to_vk(code: &str, layout: HKL) -> Option<u32> {
    if let Some(vk) = named_key(code) {
        return Some(vk);
    }
    if let Some(digit) = code.strip_prefix("Numpad")
        && let [digit @ b'0'..=b'9'] = digit.as_bytes()
    {
        return Some(0x60 + u32::from(digit - b'0'));
    }
    let scan = if let Some(letter) = code.strip_prefix("Key") {
        let [letter @ b'A'..=b'Z'] = letter.as_bytes() else {
            return None;
        };
        const LETTER_SCANS: [u32; 26] = [
            0x1E, 0x30, 0x2E, 0x20, 0x12, 0x21, 0x22, 0x23, 0x17, 0x24, 0x25, 0x26, 0x32, 0x31,
            0x18, 0x19, 0x10, 0x13, 0x1F, 0x14, 0x16, 0x2F, 0x11, 0x2D, 0x15, 0x2C,
        ];
        LETTER_SCANS[(letter - b'A') as usize]
    } else if let Some(digit) = code.strip_prefix("Digit") {
        let [digit @ b'0'..=b'9'] = digit.as_bytes() else {
            return None;
        };
        if *digit == b'0' {
            0x0B
        } else {
            u32::from(digit - b'0') + 1
        }
    } else {
        match code {
            "Minus" => 0x0C,
            "Equal" => 0x0D,
            "BracketLeft" => 0x1A,
            "BracketRight" => 0x1B,
            "Semicolon" => 0x27,
            "Quote" => 0x28,
            "Backquote" => 0x29,
            "Backslash" => 0x2B,
            "Comma" => 0x33,
            "Period" => 0x34,
            "Slash" => 0x35,
            "IntlBackslash" => 0x56,
            "IntlRo" => 0x73,
            "IntlYen" => 0x7D,
            _ => return None,
        }
    };
    let vk = unsafe { MapVirtualKeyExW(scan, MAPVK_VSC_TO_VK_EX, Some(layout)) };
    (vk != 0).then_some(vk)
}

fn named_key(code: &str) -> Option<u32> {
    if let Some(number) = code.strip_prefix('F') {
        let index = number.parse::<u32>().ok()?;
        return ((1..=24).contains(&index) && number == index.to_string()).then(|| 0x6F + index);
    }
    Some(match code {
        "Backspace" => 0x08,
        "Tab" => 0x09,
        "Clear" => 0x0C,
        "Enter" | "NumpadEnter" => 0x0D,
        "Pause" => 0x13,
        "CapsLock" => 0x14,
        "KanaMode" => 0x15,
        "Escape" => 0x1B,
        "Convert" => 0x1C,
        "NonConvert" => 0x1D,
        "Space" | " " => 0x20,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "End" => 0x23,
        "Home" => 0x24,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "PrintScreen" => 0x2C,
        "Insert" => 0x2D,
        "Delete" => 0x2E,
        "Help" => 0x2F,
        "ContextMenu" => 0x5D,
        "Sleep" => 0x5F,
        "NumpadMultiply" => 0x6A,
        "NumpadAdd" => 0x6B,
        "NumpadComma" => 0x6C,
        "NumpadSubtract" => 0x6D,
        "NumpadDecimal" => 0x6E,
        "NumpadDivide" => 0x6F,
        "NumLock" => 0x90,
        "ScrollLock" => 0x91,
        "BrowserBack" => 0xA6,
        "BrowserForward" => 0xA7,
        "BrowserRefresh" => 0xA8,
        "BrowserStop" => 0xA9,
        "BrowserSearch" => 0xAA,
        "BrowserFavorites" => 0xAB,
        "BrowserHome" => 0xAC,
        "AudioVolumeMute" => 0xAD,
        "AudioVolumeDown" => 0xAE,
        "AudioVolumeUp" => 0xAF,
        "MediaTrackNext" => 0xB0,
        "MediaTrackPrevious" => 0xB1,
        "MediaStop" => 0xB2,
        "MediaPlayPause" => 0xB3,
        "LaunchMail" => 0xB4,
        "MediaSelect" => 0xB5,
        "LaunchApp1" => 0xB6,
        "LaunchApp2" => 0xB7,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "web_binding_tests.rs"]
mod tests;
