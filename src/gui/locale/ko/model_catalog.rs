use crate::gui::locale::ModelCatalogLocaleText;

pub(super) fn get() -> ModelCatalogLocaleText {
    ModelCatalogLocaleText {
        model_priority_button: "모델 우선순위",
        model_priority_title: "모델 우선순위 체인",
        model_priority_image_chain_title: "이미지 → 텍스트",
        model_priority_text_chain_title: "텍스트 → 텍스트",
        model_priority_chosen_model: "선택된 모델",
        model_priority_fixed_hint: "항상 첫 시도",
        model_priority_add_model: "+ 모델 추가",
        model_priority_auto: "자동",
        model_priority_auto_hint: "스마트 폴백 순서로 계속",
        model_priority_skip_hint: "비활성 공급자, 누락된 키, 잘못된 키, 지원되지 않는 모델은 재시도 시 즉시 건너뜁니다.",
        custom_models_button: "사용자 모델",
        custom_models_title: "사용자 모델",
        custom_models_desc: "직접 추가한 모델을 관리합니다. 기본 모델은 보기 전용입니다.",
        custom_models_builtin_locked: "기본 - 잠김",
        custom_models_discovered_models: "검색됨",
        custom_models_add_openrouter: "+ OpenRouter 추가",
        custom_models_import_openrouter: "OpenRouter 검색",
        custom_models_scan_ollama: "Ollama 검색",
        custom_models_no_models: "모델 없음",
        custom_models_display_name: "표시 이름",
        custom_models_api_model: "API 모델",
        custom_models_type: "유형",
        custom_models_text_type: "텍스트",
        custom_models_vision_type: "비전",
        custom_models_search: "검색",
        custom_models_enabled: "활성",
    }
}
