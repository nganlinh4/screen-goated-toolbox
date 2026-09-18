use crate::gui::locale::ScreenTranslateLocaleText;

pub(super) fn get() -> ScreenTranslateLocaleText {
    ScreenTranslateLocaleText {
        screen_translate_btn: "Screen Translate",
        screen_translate_short_btn: "Screen Translate",
        screen_translate_title: "Screen Translate",
        screen_translate_intro: "Draw a box or use a saved region to translate text in place.",
        screen_translate_target_label: "Translate into",
        screen_translate_recognition_label: "Recognition · PP-OCRv6 + PP-DocLayoutV3",
        screen_translate_recognition_hint: "Reads multilingual text and uses layout to group related lines before translation.",
        screen_translate_setup_hint: "Recognition components download when needed and load when you use this feature. Progress is shown on screen.",
        screen_translate_restore_label: "Reset settings",
        screen_translate_restore_hint: "Reset language, model, instructions, opacity and capture region. Keep both shortcut lists.",
        screen_translate_model_label: "Translation model",
        screen_translate_model_fallback_hint: "If the selected model is unavailable or fails, SGT uses your Text → Text priority list.",
        screen_translate_prompt_label: "Adjust the translation prompt if you wish",
        screen_translate_prompt_hint: "Set tone or terminology. {target_language} inserts the selected language; text placement is handled automatically.",
        screen_translate_opacity_label: "Default overlay opacity",
        screen_translate_opacity_hint: "For new translations. Use an overlay's controls to change its opacity while viewing it.",
        screen_translate_hotkey_label: "Draw-box shortcuts",
        screen_translate_fullscreen_hotkey_label: "Full-screen shortcuts",
        screen_translate_adjust_region: "Adjust region",
        screen_translate_fullscreen_hint: "Translate the saved region instantly; defaults to the screen under the pointer. Drag the eight handles to adjust, check to save, or Esc to cancel.",
        screen_translate_hotkey_empty: "No shortcut set",
        screen_translate_locating: "Reading and translating the selected area…",
        screen_translate_preparing: "Preparing for the first run...",
        screen_translate_no_text: "No translatable text found",
        screen_translate_error: "Translation failed",
    }
}
