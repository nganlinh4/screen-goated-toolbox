use crate::gui::locale::TtsAdvancedLocaleText;

pub(super) fn get() -> TtsAdvancedLocaleText {
    TtsAdvancedLocaleText {
        tts_reference_voice_label: "Giọng tham chiếu:",
        tts_reference_stop_mic: "Dừng mic",
        tts_reference_record_mic: "Ghi mic",
        tts_source_pick_audio: "Chọn audio nguồn",
        tts_source_none: "Chưa chọn audio nguồn",
        tts_playground_char_count: "{n} ký tự",
        tts_s2s_target_label: "Đích",
        remove_label: "Xóa",
    }
}
