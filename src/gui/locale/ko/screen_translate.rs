use crate::gui::locale::ScreenTranslateLocaleText;

pub(super) fn get() -> ScreenTranslateLocaleText {
    ScreenTranslateLocaleText {
        screen_translate_btn: "화면 번역 (Circle to Search)",
        screen_translate_short_btn: "화면 번역",
        screen_translate_title: "화면 번역 (Circle to Search)",
        screen_translate_intro: "화면 영역을 지정하면 SGT가 각 텍스트 줄을 찾아 번역해 제자리에 표시합니다.",
        screen_translate_target_label: "번역 언어",
        screen_translate_locator_label: "텍스트 찾기",
        screen_translate_locator_model: "PaddleOCR 다국어",
        screen_translate_fixed_badge: "고정",
        screen_translate_model_label: "번역 모델",
        screen_translate_model_fallback_hint: "실패하면 텍스트 → 텍스트 우선순위 목록을 이어서 시도합니다.",
        screen_translate_prompt_label: "번역 프롬프트",
        screen_translate_prompt_hint: "선택한 대상 언어에는 {target_language}를 사용하세요.",
        screen_translate_hotkey_label: "단축키",
        screen_translate_hotkey_empty: "설정된 단축키 없음",
        screen_translate_locating: "텍스트를 찾고 번역하는 중…",
        screen_translate_no_text: "번역할 텍스트를 찾지 못했습니다",
        screen_translate_error: "번역 실패",
    }
}
