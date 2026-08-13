use crate::gui::locale::ScreenTranslateLocaleText;

pub(super) fn get() -> ScreenTranslateLocaleText {
    ScreenTranslateLocaleText {
        screen_translate_btn: "화면 번역",
        screen_translate_title: "화면 번역",
        screen_translate_intro: "화면 영역을 드래그하세요. SGT가 보이는 모든 글줄을 찾아 번역하고 제자리에 덮어씁니다.",
        screen_translate_target_label: "번역 언어",
        screen_translate_hotkey_label: "단축키",
        screen_translate_hotkey_empty: "설정된 단축키 없음",
        screen_translate_locating: "텍스트를 찾고 번역하는 중…",
        screen_translate_no_text: "번역할 텍스트를 찾지 못했습니다",
        screen_translate_error: "번역 실패",
    }
}
