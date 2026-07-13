use crate::gui::locale::GlobalSettingsLocaleText;

pub(super) fn get() -> GlobalSettingsLocaleText {
    GlobalSettingsLocaleText {
        controller_checkbox_label: "컨트롤러",
        api_keys_header: "API 키",
        groq_label: "Groq API 키:",
        software_update_header: "소프트웨어 업데이트",
        donate_header: "개발자 후원",
        donate_body: "SGT는 무료 소프트웨어입니다. 유용하셨다면 베트남 은행 송금(VietQR)으로 개발자를 후원하실 수 있습니다. 감사합니다!",
        donate_note: "은행 송금은 베트남 후원자만 이용할 수 있습니다.",
        donate_vietnamese: false,
        startup_display_header: "시작 및 표시",
        favorite_overlay_opacity_label: "결과 오버레이 기본 불투명도",
        model_thinking: "생각 중...",
        ollama_url_guide: "올라마 설명서 보기",
    }
}
