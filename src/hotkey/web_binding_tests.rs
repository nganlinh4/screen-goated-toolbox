use super::*;

#[test]
fn named_keys_cover_numpad_function_media_and_navigation() {
    for (key, code, expected) in [
        ("1", "Numpad1", 0x61),
        ("End", "Numpad1", 0x23),
        ("+", "NumpadAdd", 0x6B),
        ("Enter", "NumpadEnter", 0x0D),
        ("Backspace", "Backspace", 0x08),
        (" ", "Space", 0x20),
        ("F24", "F24", 0x87),
        ("AudioVolumeMute", "AudioVolumeMute", 0xAD),
        ("MediaPlayPause", "MediaPlayPause", 0xB3),
        ("ArrowLeft", "ArrowLeft", 0x25),
    ] {
        let binding = from_web_event(key, code, true, false, true, true).unwrap();
        assert_eq!(binding.code, expected, "{code}");
        assert_eq!(binding.modifiers, MOD_CONTROL | MOD_SHIFT | MOD_WIN);
        assert_eq!(
            binding.display_name(),
            names::format(expected, binding.modifiers)
        );
    }
}

#[test]
fn malformed_dom_codes_cannot_alias_a_real_binding() {
    assert!(from_web_event("End", "NumpadX", false, false, false, false).is_none());
    for code in [
        "F0",
        "F25",
        "F01",
        "F4294967295",
        "Key",
        "KeyAB",
        "Keyé",
        "Keya",
        "Digit10",
        "DigitX",
        "NumpadX",
        "Numpad10",
        "Unknown",
        "ControlLeft",
    ] {
        assert!(
            from_web_event("x", code, false, false, false, false).is_none(),
            "{code}"
        );
    }
    for key in [
        "Control",
        "Shift",
        "Alt",
        "Meta",
        "Dead",
        "Unidentified",
        "😀",
    ] {
        assert!(
            from_web_event(key, "", false, false, false, false).is_none(),
            "{key}"
        );
    }
}

#[test]
fn physical_dom_keys_follow_the_selected_layout() {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayoutList;
    let mut layouts = [HKL::default(); 64];
    let count = unsafe { GetKeyboardLayoutList(Some(&mut layouts)) };
    for layout in layouts.into_iter().take(count as usize) {
        for (code, scan) in [
            ("KeyQ", 0x10),
            ("KeyA", 0x1E),
            ("Quote", 0x28),
            ("IntlBackslash", 0x56),
        ] {
            let expected = unsafe { MapVirtualKeyExW(scan, MAPVK_VSC_TO_VK_EX, Some(layout)) };
            assert_eq!(
                code_to_vk(code, layout),
                (expected != 0).then_some(expected)
            );
        }
    }
}
