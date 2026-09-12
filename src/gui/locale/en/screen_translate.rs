use crate::gui::locale::ScreenTranslateLocaleText;

pub(super) fn get() -> ScreenTranslateLocaleText {
    ScreenTranslateLocaleText {
        screen_translate_btn: "Screen Translate",
        screen_translate_short_btn: "Screen Translate",
        screen_translate_title: "Screen Translate",
        screen_translate_intro: "Select a screen area to recognize and translate text in place.",
        screen_translate_target_label: "Translate into",
        screen_translate_recognition_label: "On-device text & layout recognition",
        screen_translate_recognition_hint: "Reads multilingual text and uses layout to group related lines before translation.",
        screen_translate_setup_hint: "Recognition components download when needed and load when you use this feature. Progress is shown on screen.",
        screen_translate_presentation_label: "Automatic overlay fitting",
        screen_translate_presentation_hint: "Fits each translation within its source area, adjusting font size and width. Unchanged text stays visible on the original screen.",
        screen_translate_restore_label: "Reset settings",
        screen_translate_restore_hint: "Reset this feature's language, model, instructions and opacity. Keep your shortcuts.",
        screen_translate_model_label: "Translation model",
        screen_translate_model_fallback_hint: "If the selected model is unavailable or fails, SGT uses your Text → Text priority list.",
        screen_translate_prompt_label: "Custom translation instructions",
        screen_translate_prompt_hint: "Set tone or terminology. {target_language} inserts the selected language; text placement is handled automatically.",
        screen_translate_opacity_label: "Default overlay opacity",
        screen_translate_opacity_hint: "For new translations. Use an overlay's controls to change its opacity while viewing it.",
        screen_translate_hotkey_label: "Shortcuts",
        screen_translate_hotkey_empty: "No shortcut set",
        screen_translate_locating: "Reading and translating the selected area…",
        screen_translate_preparing: "Preparing for the first run...",
        screen_translate_no_text: "No translatable text found",
        screen_translate_error: "Translation failed",
    }
}
