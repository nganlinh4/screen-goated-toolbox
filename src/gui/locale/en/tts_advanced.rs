use crate::gui::locale::TtsAdvancedLocaleText;

pub(super) fn get() -> TtsAdvancedLocaleText {
    TtsAdvancedLocaleText {
        tts_reference_voice_label: "Reference voice:",
        tts_reference_stop_mic: "Stop mic",
        tts_reference_record_mic: "Record mic",
        tts_source_pick_audio: "Pick source audio",
        tts_source_none: "No source audio selected",
        tts_playground_char_count: "{n} chars",
        tts_s2s_target_label: "Target",
        remove_label: "Remove",
    }
}
