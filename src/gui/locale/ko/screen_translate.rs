use crate::gui::locale::ScreenTranslateLocaleText;

pub(super) fn get() -> ScreenTranslateLocaleText {
    ScreenTranslateLocaleText {
        screen_translate_btn: "화면 번역",
        screen_translate_short_btn: "화면 번역",
        screen_translate_title: "화면 번역",
        screen_translate_intro: "화면 영역을 선택하면 글자를 인식하고 원래 위치에 번역해 표시합니다.",
        screen_translate_target_label: "번역 언어",
        screen_translate_recognition_label: "기기 내 텍스트·레이아웃 인식",
        screen_translate_recognition_hint: "다국어 텍스트를 읽고 레이아웃을 참고해 관련된 줄을 묶어 번역합니다.",
        screen_translate_setup_hint: "필요한 인식 구성 요소를 다운로드하고, 이 기능을 사용할 때 불러옵니다. 진행 상황은 화면에 표시됩니다.",
        screen_translate_presentation_label: "자동 오버레이 맞춤",
        screen_translate_presentation_hint: "글자 크기와 너비를 조절해 원문 영역 안에 번역을 배치합니다. 변경할 필요가 없는 텍스트는 원래 화면에 그대로 둡니다.",
        screen_translate_restore_label: "설정 초기화",
        screen_translate_restore_hint: "이 기능의 언어, 모델, 번역 지침과 불투명도를 초기화합니다. 단축키는 유지합니다.",
        screen_translate_model_label: "번역 모델",
        screen_translate_model_fallback_hint: "선택한 모델을 사용할 수 없거나 오류가 나면 텍스트 → 텍스트 우선순위 목록을 사용합니다.",
        screen_translate_prompt_label: "사용자 번역 지침",
        screen_translate_prompt_hint: "문체나 용어를 지정하세요. {target_language}는 선택한 언어로 바뀌며, 텍스트 배치는 자동으로 처리됩니다.",
        screen_translate_opacity_label: "기본 오버레이 불투명도",
        screen_translate_opacity_hint: "새 번역에 적용됩니다. 표시 중인 번역의 불투명도는 해당 오버레이에서 조절하세요.",
        screen_translate_hotkey_label: "단축키",
        screen_translate_hotkey_empty: "설정된 단축키 없음",
        screen_translate_locating: "선택 영역을 읽고 번역하는 중…",
        screen_translate_preparing: "첫 실행 준비 중...",
        screen_translate_no_text: "번역할 텍스트를 찾지 못했습니다",
        screen_translate_error: "번역 실패",
    }
}
