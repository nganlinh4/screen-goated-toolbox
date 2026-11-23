//! `key_combination` token grammar: `,` separates sequential chords, `+`
//! joins the keys of one chord, and each key is a name or a single character.

use anyhow::{Result, anyhow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_APPS, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1,
    VK_HOME, VK_INSERT, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_NEXT,
    VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_COMMA,
    VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_PAUSE, VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SNAPSHOT, VK_SPACE, VK_TAB, VK_UP, VkKeyScanW,
};

pub(super) fn parse_key_sequence(combo: &str) -> Result<Vec<Vec<VIRTUAL_KEY>>> {
    if combo.trim().is_empty() {
        return Err(anyhow!("empty key combination"));
    }
    combo
        .split(',')
        .enumerate()
        .map(|(group_index, group)| {
            if group.trim().is_empty() {
                return Err(anyhow!("empty key group at position {}", group_index + 1));
            }
            group
                .split('+')
                .map(str::trim)
                .map(|token| {
                    if token.is_empty() {
                        Err(anyhow!("empty key in group {}", group_index + 1))
                    } else {
                        token_to_vk(token).ok_or_else(|| anyhow!("unknown key: {token}"))
                    }
                })
                .collect()
        })
        .collect()
}

/// Resolve one key token. Punctuation keys are named by their physical US
/// position (the `KeyboardEvent.code` names: `minus`, `bracketleft`, ...),
/// because `,` and `+` are grammar; any other single character resolves
/// through the active layout and must not need a modifier.
fn token_to_vk(token: &str) -> Option<VIRTUAL_KEY> {
    let lower = token.to_ascii_lowercase();
    let vk = match lower.as_str() {
        "ctrl" | "control" => VK_CONTROL,
        "alt" => VK_MENU,
        "shift" => VK_SHIFT,
        "lctrl" | "leftctrl" => VK_LCONTROL,
        "rctrl" | "rightctrl" => VK_RCONTROL,
        "lalt" | "leftalt" => VK_LMENU,
        "ralt" | "rightalt" | "altgr" => VK_RMENU,
        "lshift" | "leftshift" => VK_LSHIFT,
        "rshift" | "rightshift" => VK_RSHIFT,
        "win" | "super" | "meta" | "cmd" | "lwin" => VK_LWIN,
        "rwin" => VK_RWIN,
        "apps" | "menu" | "contextmenu" => VK_APPS,
        "enter" | "return" => VK_RETURN,
        "tab" => VK_TAB,
        "esc" | "escape" => VK_ESCAPE,
        "space" | "spacebar" => VK_SPACE,
        "backspace" | "back" => VK_BACK,
        "delete" | "del" => VK_DELETE,
        "insert" | "ins" => VK_INSERT,
        "up" => VK_UP,
        "down" => VK_DOWN,
        "left" => VK_LEFT,
        "right" => VK_RIGHT,
        "home" => VK_HOME,
        "end" => VK_END,
        "pageup" | "pgup" => VK_PRIOR,
        "pagedown" | "pgdn" => VK_NEXT,
        "printscreen" | "prtsc" => VK_SNAPSHOT,
        "pause" => VK_PAUSE,
        "minus" => VK_OEM_MINUS,
        "equal" | "equals" => VK_OEM_PLUS,
        "comma" => VK_OEM_COMMA,
        "period" | "dot" => VK_OEM_PERIOD,
        "slash" => VK_OEM_2,
        "semicolon" => VK_OEM_1,
        "quote" | "apostrophe" => VK_OEM_7,
        "backquote" | "grave" | "backtick" => VK_OEM_3,
        "bracketleft" => VK_OEM_4,
        "bracketright" => VK_OEM_6,
        "backslash" => VK_OEM_5,
        _ => return function_key(&lower).or_else(|| single_character(&lower)),
    };
    Some(vk)
}

fn function_key(lower: &str) -> Option<VIRTUAL_KEY> {
    let number: u16 = lower.strip_prefix('f')?.parse().ok()?;
    (1..=24)
        .contains(&number)
        .then(|| VIRTUAL_KEY(VK_F1.0 + number - 1))
}

fn single_character(lower: &str) -> Option<VIRTUAL_KEY> {
    let mut chars = lower.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if c.is_ascii_lowercase() {
        return Some(VIRTUAL_KEY(c.to_ascii_uppercase() as u16));
    }
    if c.is_ascii_digit() {
        return Some(VIRTUAL_KEY(c as u16));
    }
    let mut units = [0u16; 2];
    let [unit] = c.encode_utf16(&mut units) else {
        return None;
    };
    // Low byte: the key; high byte: the shift state it needs. A character that
    // needs Shift/Ctrl/Alt is a chord the caller must spell out.
    let scan = unsafe { VkKeyScanW(*unit) };
    let [vk, shift_state] = scan.to_le_bytes();
    (scan != -1 && shift_state == 0).then_some(VIRTUAL_KEY(vk as u16))
}

/// Keys in the "extended" set send an extra 0xE0 prefix on real hardware; the
/// EXTENDEDKEY flag reproduces that so `KeyboardEvent.code` resolves correctly.
pub(in crate::overlay::computer_control::executor) fn is_extended_key(vk: VIRTUAL_KEY) -> bool {
    matches!(
        vk,
        VK_LEFT
            | VK_UP
            | VK_RIGHT
            | VK_DOWN
            | VK_HOME
            | VK_END
            | VK_PRIOR
            | VK_NEXT
            | VK_INSERT
            | VK_DELETE
            | VK_LWIN
            | VK_RWIN
            | VK_RCONTROL
            | VK_RMENU
            | VK_APPS
            | VK_SNAPSHOT
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letter_and_named_keys() {
        assert_eq!(token_to_vk("c"), Some(VIRTUAL_KEY(b'C' as u16)));
        assert_eq!(token_to_vk("Ctrl"), Some(VK_CONTROL));
        assert_eq!(token_to_vk("enter"), Some(VK_RETURN));
        assert_eq!(token_to_vk("F5"), Some(VIRTUAL_KEY(VK_F1.0 + 4)));
        assert!(token_to_vk("nope").is_none());
    }

    #[test]
    fn expresses_punctuation_insert_apps_and_high_function_keys() {
        for (token, vk) in [
            ("minus", VK_OEM_MINUS),
            ("equal", VK_OEM_PLUS),
            ("comma", VK_OEM_COMMA),
            ("period", VK_OEM_PERIOD),
            ("slash", VK_OEM_2),
            ("bracketleft", VK_OEM_4),
            ("semicolon", VK_OEM_1),
            ("insert", VK_INSERT),
            ("contextmenu", VK_APPS),
            ("F13", VIRTUAL_KEY(VK_F1.0 + 12)),
            ("f24", VIRTUAL_KEY(VK_F1.0 + 23)),
        ] {
            assert_eq!(token_to_vk(token), Some(vk), "{token}");
        }
        assert!(token_to_vk("f25").is_none() && token_to_vk("f0").is_none());
    }

    #[test]
    fn menu_names_the_context_menu_key_not_alt() {
        assert_eq!(token_to_vk("menu"), Some(VK_APPS));
        assert_eq!(token_to_vk("alt"), Some(VK_MENU));
        let sequence = parse_key_sequence("shift+menu").unwrap();
        assert_eq!(sequence, [vec![VK_SHIFT, VK_APPS]]);
    }

    #[test]
    fn unshifted_layout_characters_resolve_and_shifted_ones_need_a_chord() {
        // Space resolves on every layout without a modifier.
        assert_eq!(single_character(" "), Some(VK_SPACE));
        // '!' needs Shift on every common layout, so it is not a bare key.
        assert_eq!(single_character("!"), None);
        assert_eq!(single_character("ab"), None);
    }

    #[test]
    fn windows_right_modifier_and_apps_keys_are_extended() {
        for vk in [VK_LWIN, VK_RWIN, VK_RCONTROL, VK_RMENU, VK_APPS, VK_INSERT] {
            assert!(is_extended_key(vk));
        }
        for vk in [VK_LCONTROL, VK_LMENU, VK_SHIFT, VK_RETURN] {
            assert!(!is_extended_key(vk));
        }
    }

    #[test]
    fn parses_sequential_chords_without_changing_plus_semantics() {
        let sequence = parse_key_sequence("Ctrl+Shift+K, Tab").unwrap();
        assert_eq!(sequence.len(), 2);
        assert_eq!(
            sequence[0],
            [VK_CONTROL, VK_SHIFT, VIRTUAL_KEY(b'K' as u16)]
        );
        assert_eq!(sequence[1], [VK_TAB]);
        assert_eq!(
            parse_key_sequence("ctrl+minus").unwrap(),
            [vec![VK_CONTROL, VK_OEM_MINUS]]
        );
    }

    #[test]
    fn rejects_empty_sequence_groups() {
        let error = parse_key_sequence("A, ,B").unwrap_err();
        assert!(error.to_string().contains("empty key group"));
    }
}
