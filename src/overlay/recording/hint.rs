use crate::config::Hotkey;

pub(super) fn stop_hint(template: &str, hotkeys: &[Hotkey]) -> String {
    let shortcut = hotkeys
        .first()
        .map(Hotkey::display_name)
        .unwrap_or_default();
    let keys = crate::hotkey::names::with_escape(&shortcut);
    template.replace("{hotkey}", &keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_hint_uses_configured_combination_and_esc_fallback() {
        let template = "Press {hotkey} to stop";
        let hotkeys = [Hotkey::new(0x4B, 6)];
        assert_eq!(
            stop_hint(template, &hotkeys),
            "Press Esc / Ctrl + Shift + K to stop"
        );
        assert_eq!(stop_hint(template, &[]), "Press Esc to stop");
        assert_eq!(
            stop_hint(template, &[Hotkey::new(0x1B, 0)]),
            "Press Esc to stop"
        );
        assert_eq!(
            stop_hint(template, &[Hotkey::new(0x1B, 2)]),
            "Press Esc / Ctrl + Esc to stop"
        );
    }

    #[test]
    fn stop_hint_preserves_mouse_and_punctuation_labels_in_every_locale() {
        for language in ["en", "vi", "ko"] {
            let locale = crate::gui::locale::LocaleText::get(language);
            for hotkey in [
                Hotkey::new(0x05, 2),
                Hotkey::new(0x6B, 4),
                Hotkey::new(0xDC, 2),
            ] {
                let name = hotkey.display_name();
                let hint = stop_hint(locale.shell.recording_subtext, &[hotkey]);
                assert!(hint.contains(&name));
                assert!(!hint.contains("{hotkey}"));
            }
        }
    }
}
