use crate::gui::locale::LiveTranslateLocaleText;

pub(super) fn get() -> LiveTranslateLocaleText {
    LiveTranslateLocaleText {
        live_translate_title: "Live Translate",
        live_translate_btn: "Live Translate",
        live_translate_intro: "Listen to microphone or device audio and show a continuously updated translation.",
        live_translate_input_title: "Listen",
        live_translate_translation_title: "Translate",
        live_translate_display_title: "Overlay",
        live_translate_font_size: "Text size",
        live_translate_hotkey_label: "Global shortcut",
        live_translate_hotkey_unset: "No shortcut set",
        live_translate_start: "Start",
        live_translate_stop: "Stop",
    }
}
