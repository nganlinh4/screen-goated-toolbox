use crate::gui::locale::ScreenTranslateLocaleText;

pub(super) fn get() -> ScreenTranslateLocaleText {
    ScreenTranslateLocaleText {
        screen_translate_btn: "Screen Translate",
        screen_translate_title: "Screen Translate",
        screen_translate_intro: "Draw around any screen region. SGT locates and translates every visible line, then paints it back in place.",
        screen_translate_target_label: "Translate into",
        screen_translate_hotkey_label: "Shortcuts",
        screen_translate_hotkey_empty: "No shortcut set",
        screen_translate_locating: "Locating and translating text…",
        screen_translate_no_text: "No translatable text found",
        screen_translate_error: "Translation failed",
    }
}
