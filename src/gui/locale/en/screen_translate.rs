use crate::gui::locale::ScreenTranslateLocaleText;

pub(super) fn get() -> ScreenTranslateLocaleText {
    ScreenTranslateLocaleText {
        screen_translate_btn: "Screen Translate (Circle to Search)",
        screen_translate_short_btn: "Screen Translate",
        screen_translate_title: "Screen Translate (Circle to Search)",
        screen_translate_intro: "Draw around a screen region. SGT finds, translates, and repaints each visible line in place.",
        screen_translate_target_label: "Translate into",
        screen_translate_locator_label: "Text locator",
        screen_translate_locator_model: "PaddleOCR multilingual",
        screen_translate_fixed_badge: "Fixed",
        screen_translate_model_label: "Translation model",
        screen_translate_model_fallback_hint: "Failures continue through the Text → Text priority list.",
        screen_translate_prompt_label: "Translation prompt",
        screen_translate_prompt_hint: "Use {target_language} for the selected destination language.",
        screen_translate_hotkey_label: "Shortcuts",
        screen_translate_hotkey_empty: "No shortcut set",
        screen_translate_locating: "Locating and translating text…",
        screen_translate_no_text: "No translatable text found",
        screen_translate_error: "Translation failed",
    }
}
