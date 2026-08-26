use crate::gui::locale::LiveTranslateLocaleText;

pub(super) fn get() -> LiveTranslateLocaleText {
    LiveTranslateLocaleText {
        live_translate_title: "실시간 번역",
        live_translate_btn: "실시간 번역",
        live_translate_intro: "마이크 또는 장치 오디오를 듣고 계속 업데이트되는 번역을 표시합니다.",
        live_translate_input_title: "듣기",
        live_translate_translation_title: "번역",
        live_translate_display_title: "오버레이",
        live_translate_font_size: "글자 크기",
        live_translate_hotkey_label: "전역 단축키",
        live_translate_hotkey_unset: "설정된 단축키 없음",
        live_translate_start: "시작",
        live_translate_stop: "중지",
    }
}
