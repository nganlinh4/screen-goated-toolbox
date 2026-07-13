use crate::gui::locale::ModelCatalogLocaleText;

pub(super) fn get() -> ModelCatalogLocaleText {
    ModelCatalogLocaleText {
        model_priority_button: "Model Priority",
        model_priority_title: "Model Priority Chains",
        model_priority_image_chain_title: "Image → Text",
        model_priority_text_chain_title: "Text → Text",
        model_priority_chosen_model: "Chosen model",
        model_priority_fixed_hint: "always first",
        model_priority_add_model: "+ Add model",
        model_priority_auto: "Auto",
        model_priority_auto_hint: "continue with smart fallback order",
        model_priority_skip_hint: "Unavailable providers, missing keys, invalid keys, and unsupported models are skipped immediately during retry.",
        custom_models_button: "Custom Models",
        custom_models_title: "Custom Models",
        custom_models_desc: "Manage user-added models. Built-in models are visible but locked.",
        custom_models_builtin_locked: "Built-in - locked",
        custom_models_discovered_models: "Discovered",
        custom_models_add_openrouter: "+ Add OpenRouter",
        custom_models_import_openrouter: "Scan OpenRouter",
        custom_models_scan_ollama: "Scan Ollama",
        custom_models_no_models: "No models yet",
        custom_models_display_name: "Display name",
        custom_models_api_model: "API model",
        custom_models_type: "Type",
        custom_models_text_type: "Text",
        custom_models_vision_type: "Vision",
        custom_models_search: "Search",
        custom_models_enabled: "Enabled",
    }
}
