use super::*;
use crate::config::Hotkey;

#[test]
fn every_modifier_combination_has_one_order_and_ignores_registration_flags() {
    for mask in 0..16 {
        let expected: Vec<_> = [(2, "Ctrl"), (1, "Alt"), (4, "Shift"), (8, "Win")]
            .into_iter()
            .filter(|(bit, _)| mask & bit != 0)
            .map(|(_, label)| label)
            .chain(["F24"])
            .collect();
        assert_eq!(format(0x87, mask), expected.join(" + "));
        assert_eq!(format(0x87, mask | 0x4000), format(0x87, mask));
    }
}

#[test]
fn labels_distinguish_main_row_numpad_mouse_and_media_keys() {
    for (code, expected) in [
        (0x31, "1"),
        (0x61, "Numpad 1"),
        (0x6B, "Numpad Add"),
        (0x04, "Middle Click"),
        (0x05, "Mouse Back"),
        (0x06, "Mouse Forward"),
        (0x25, "Left"),
        (0x21, "Page Up"),
        (0xAD, "Volume Mute"),
        (0xB3, "Media Play/Pause"),
        (0x87, "F24"),
        (0xFFFF, "VK 0xFFFF"),
    ] {
        assert_eq!(key_name(code), expected);
    }
    for code in 0..=255 {
        assert!(!key_name(code).is_empty());
    }
}

#[test]
fn saved_names_are_not_binding_identity_and_serialization_refreshes_them() {
    let a: Hotkey = serde_json::from_str(r#"{"code":97,"modifiers":10,"name":"Num1"}"#).unwrap();
    let b: Hotkey = serde_json::from_str(r#"{"code":97,"modifiers":10}"#).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.display_name(), "Ctrl + Win + Numpad 1");
    let json = serde_json::to_value(&a).unwrap();
    assert_eq!(json["name"], "Ctrl + Win + Numpad 1");
    assert_eq!(json["code"], 97);
    assert_eq!(json["modifiers"], 10);
    assert_eq!(serde_json::from_value::<Hotkey>(json).unwrap(), a);
}

#[test]
fn oem_labels_follow_installed_layouts_without_activating_them() {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayoutList;
    let mut layouts = [HKL::default(); 64];
    let count = unsafe { GetKeyboardLayoutList(Some(&mut layouts)) };
    assert!(count > 0);
    for layout in layouts.into_iter().take(count as usize) {
        for code in [
            0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0, 0xDB, 0xDC, 0xDD, 0xDE, 0xE2,
        ] {
            let raw = unsafe { MapVirtualKeyExW(code, MAPVK_VK_TO_CHAR, Some(layout)) };
            if let Some(character) = char::from_u32(raw & 0x7FFF_FFFF)
                && !character.is_control()
                && !character.is_whitespace()
            {
                assert_eq!(key_name_for_layout(code, layout), character.to_string());
            }
        }
    }
}
