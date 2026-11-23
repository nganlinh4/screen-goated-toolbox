use crate::gui::locale::TtsAdvancedLocaleText;

pub(super) fn get() -> TtsAdvancedLocaleText {
    TtsAdvancedLocaleText {
        tts_reference_voice_label: "참조 음성:",
        tts_reference_stop_mic: "마이크 중지",
        tts_reference_record_mic: "마이크 녹음",
        tts_source_pick_audio: "원본 오디오 선택",
        tts_source_none: "선택된 원본 오디오 없음",
        tts_playground_char_count: "{n}자",
        tts_s2s_target_label: "대상",
        remove_label: "삭제",
    }
}
