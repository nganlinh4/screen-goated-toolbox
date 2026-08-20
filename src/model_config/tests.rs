use super::*;

#[test]
fn benchmark_balanced_vision_winner_is_default_and_first_fallback() {
    assert_eq!(DEFAULT_IMAGE_MODEL_ID, "groq-qwen-3-6-27b-vision");
    assert_eq!(
        default_image_to_text_priority_chain_ids().first().copied(),
        Some(DEFAULT_IMAGE_MODEL_ID)
    );
    let model = get_model_by_id(DEFAULT_IMAGE_MODEL_ID).expect("default vision model exists");
    assert_eq!(model.provider, "groq");
    assert_eq!(model.full_name, "qwen/qwen3.6-27b");
    assert_eq!(model.intelligence_tier, Some(4));
    assert_eq!(model.typical_latency_ms, Some(846));
    assert_eq!(
        model.performance_source.as_deref(),
        Some("benchmark-2026-08-20-protocol11-clean:ocr-small-1024")
    );
}

#[test]
fn recommended_defaults_match_the_shared_retry_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/preset-system/retry-runtime.json"
    )))
    .expect("preset retry fixture parses");
    let providers = &fixture["provider_settings"];
    let models = &fixture["recommended_model_defaults"];
    let update = &fixture["recommended_settings_update"];

    assert_eq!(providers["use_groq"].as_bool(), Some(DEFAULT_USE_GROQ));
    assert_eq!(providers["use_gemini"].as_bool(), Some(DEFAULT_USE_GEMINI));
    assert_eq!(
        providers["use_openrouter"].as_bool(),
        Some(DEFAULT_USE_OPENROUTER)
    );
    assert_eq!(providers["use_ollama"].as_bool(), Some(DEFAULT_USE_OLLAMA));
    assert_eq!(
        models["generic_image"].as_str(),
        Some(DEFAULT_IMAGE_MODEL_ID)
    );
    // Image presets all track the default; there is no separate pin to assert.
    assert!(models.get("accurate_image").is_none());
    assert!(models.get("image_translate").is_none());
    assert_eq!(models["image_ask"].as_str(), Some(DEFAULT_IMAGE_MODEL_ID));
    assert_eq!(models["generic_text"].as_str(), Some(DEFAULT_TEXT_MODEL_ID));
    assert_eq!(
        models["text_arena_fast"].as_str(),
        Some(PRESET_TEXT_ARENA_FAST_MODEL_ID)
    );
    assert_eq!(
        models["text_game"].as_str(),
        Some(PRESET_TEXT_GAME_MODEL_ID)
    );
    assert_eq!(models["search"].as_str(), Some(PRESET_SEARCH_MODEL_ID));
    assert_eq!(
        models["audio_transcribe"].as_str(),
        Some(PRESET_AUDIO_TRANSCRIBE_MODEL_ID)
    );
    assert_eq!(
        models["audio_continuous"].as_str(),
        Some(PRESET_AUDIO_CONTINUOUS_MODEL_ID)
    );
    assert_eq!(
        models["audio_direct_translate"].as_str(),
        Some(PRESET_AUDIO_DIRECT_TRANSLATE_MODEL_ID)
    );
    assert_eq!(
        models["audio_offline_transcribe"].as_str(),
        Some(PRESET_AUDIO_OFFLINE_TRANSCRIBE_MODEL_ID)
    );
    assert_eq!(update["enable_recommended_providers"].as_bool(), Some(true));
    assert_eq!(update["disable_other_providers"].as_bool(), Some(false));
}

#[test]
fn benchmark_balanced_text_winner_is_default_and_first_fallback() {
    assert_eq!(DEFAULT_TEXT_MODEL_ID, "groq-qwen-3-6-27b-text");
    assert_eq!(
        default_text_to_text_priority_chain_ids().first().copied(),
        Some(DEFAULT_TEXT_MODEL_ID)
    );
    let model = get_model_by_id(DEFAULT_TEXT_MODEL_ID).expect("default text model exists");
    assert_eq!(model.provider, "groq");
    assert_eq!(model.full_name, "qwen/qwen3.6-27b");
    assert_eq!(model.intelligence_tier, Some(4));
    assert_eq!(model.typical_latency_ms, Some(266));
    assert_eq!(
        model.performance_source.as_deref(),
        Some("benchmark-2026-08-19-protocol10:text")
    );
    // Ordered on measured merit: the leader is the fastest text endpoint at
    // 0.27s, and the 100%-reliable Groq row sits directly behind it so a
    // rejected first call is absorbed without leaving the provider.
    for (index, expected) in [
        (1, "groq-gpt-oss-120b-text"),
        (2, "google-gemini-3-5-flash-lite-text"),
        (3, "google-gemini-robotics-er-2-text"),
        (4, "openrouter-nemotron-3-super-120b-text"),
    ] {
        assert_eq!(
            default_text_to_text_priority_chain_ids()
                .get(index)
                .copied(),
            Some(expected)
        );
    }
    // Lowest measured translation quality of any enabled text row, so speed
    // alone does not buy it a forward seat.
    assert_eq!(
        default_text_to_text_priority_chain_ids().get(8).copied(),
        Some("groq-gpt-oss-20b-text")
    );
    let openrouter = get_model_by_id("openrouter-nemotron-3-super-120b-text")
        .expect("OpenRouter text fallback exists");
    assert_eq!(
        openrouter.full_name,
        "nvidia/nemotron-3-super-120b-a12b:free"
    );
    assert_eq!(
        ordinary_reasoning_policy("openrouter", "nvidia/nemotron-3-super-120b-a12b:free"),
        OrdinaryReasoningPolicy::OpenAiEffort("none")
    );
}

#[test]
fn live_thinking_schema_follows_exact_endpoint() {
    assert_eq!(
        live_endpoint_profile(GEMINI_LIVE_API_MODEL_2_5).and_then(|profile| profile.thinking),
        Some(LiveThinkingConfig::Budget(0))
    );
    assert_eq!(
        live_endpoint_profile(GEMINI_LIVE_API_MODEL_3_1).and_then(|profile| profile.thinking),
        Some(LiveThinkingConfig::Level("minimal"))
    );
}

#[test]
fn live_output_limits_match_shared_parity_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/preset-system/gemini-live-socket-protocol.json"
    )))
    .expect("Gemini Live socket fixture parses");
    let limits = fixture["modelOutputLimits"]
        .as_object()
        .expect("modelOutputLimits must be an object");
    assert!(!limits.is_empty(), "modelOutputLimits must not be empty");

    for (api_model, expected) in limits {
        let expected = u32::try_from(
            expected
                .as_u64()
                .expect("model output limit must be an unsigned integer"),
        )
        .expect("model output limit must fit u32");
        assert_eq!(
            live_endpoint_profile(api_model).and_then(|profile| profile.max_output_tokens),
            Some(expected),
            "catalog output limit drifted for {api_model}"
        );
    }
}

#[test]
fn tts_model_normalization_uses_catalog_default() {
    for persisted in ["", "gemini", "unknown-live-model"] {
        assert_eq!(
            normalize_tts_gemini_model(persisted),
            DEFAULT_GEMINI_LIVE_TTS_MODEL,
            "legacy or invalid TTS model must use the catalog default"
        );
    }

    for (api_model, _) in tts_gemini_model_options() {
        assert_eq!(normalize_tts_gemini_model(api_model), *api_model);
    }
}

#[test]
fn live_translate_routing_comes_from_the_endpoint_profile() {
    assert_eq!(
        realtime_transcription_live_protocol(GEMINI_LIVE_TRANSLATE_MODEL_ID),
        Some("live-translate")
    );
    assert!(is_gemini_live_translate_model_id(
        GEMINI_LIVE_TRANSLATE_MODEL_ID
    ));
    assert!(!is_gemini_live_translate_model_id(
        GEMINI_LIVE_AUDIO_MODEL_ID_3_1
    ));
}

#[test]
fn vision_request_shapes_are_exact_endpoint_profiles() {
    let google_gemma = vision_request_profile("google", "gemma-4-31b-it");
    assert_eq!(google_gemma.input_order, VisionInputOrder::ImageFirst);
    assert_eq!(
        google_gemma.media_resolution,
        VisionMediaResolutionPolicy::ProviderDefault
    );
    assert_eq!(
        google_gemma.structured_output,
        StructuredOutputPolicy::StrictJsonSchema
    );
    for model in ["gemini-3.5-flash-lite", "gemini-robotics-er-2-preview"] {
        let profile = vision_request_profile("google", model);
        assert_eq!(profile.input_order, VisionInputOrder::ImageFirst);
        assert_eq!(
            profile.structured_output,
            StructuredOutputPolicy::StrictJsonSchema
        );
    }

    let qwen = vision_request_profile("groq", "qwen/qwen3.6-27b");
    assert_eq!(qwen.sampling, VisionSamplingPolicy::Qwen3GroqNonThinking);
    assert_eq!(qwen.max_output_tokens, Some(512));
    assert_eq!(qwen.structured_output, StructuredOutputPolicy::JsonObject);
    let qwen_model = get_model_by_id("groq-qwen-3-6-27b-vision").expect("Qwen vision model exists");
    assert_eq!(qwen_model.typical_latency_ms, Some(846));
    assert_eq!(
        qwen_model.performance_source.as_deref(),
        Some("benchmark-2026-08-20-protocol11-clean:ocr-small-1024")
    );

    // The nemotron-omni row was removed after measuring 10% text and 0% vision
    // reliability; dots-3 Note is the surviving OpenRouter vision endpoint.
    let dots = vision_request_profile("openrouter", "dots-studio/dots-3-note-preview:free");
    assert_eq!(dots.input_order, VisionInputOrder::TextFirst);
    assert_eq!(
        ordinary_reasoning_policy("openrouter", "dots-studio/dots-3-note-preview:free"),
        OrdinaryReasoningPolicy::OpenAiEffort("none")
    );
    assert_eq!(
        &default_image_to_text_priority_chain_ids()[..5],
        &[
            "groq-qwen-3-6-27b-vision",
            "google-gemini-3-5-flash-lite-vision",
            "google-gemini-3-5-flash-vision",
            "google-gemini-3-flash-vision",
            "google-gemini-3-1-flash-lite-vision",
        ]
    );

    let future = vision_request_profile("future-provider", "future-model");
    assert_eq!(future, VisionRequestProfile::SAFE_DEFAULT);
}

#[test]
fn search_capability_uses_exact_catalog_profiles() {
    for id in [
        "google-gemini-3-flash-text",
        "google-gemini-3-1-flash-lite-text",
        "google-gemini-3-5-flash-lite-text",
        "google-gemini-3-6-flash-text",
        "google-gemini-robotics-er-1-6-vision",
        "groq-compound-mini-search",
    ] {
        assert!(model_supports_search_by_id_with_custom(id, &[]), "{id}");
    }
    for id in [
        "google-gemma-4-31b-text",
        "google-gemini-3-1-live-text",
        "groq-gpt-oss-120b-text",
        "unknown-compound-text",
    ] {
        assert!(!model_supports_search_by_id_with_custom(id, &[]), "{id}");
    }
}

#[test]
fn search_marker_requires_default_tool_execution_not_capability_alone() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/model-catalog/presentation.json"
    )))
    .expect("model catalog presentation fixture parses");
    let mut expected = fixture["search_marker"]["built_in_model_ids"]
        .as_array()
        .expect("search marker model IDs must be an array")
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    expected.sort();

    let mut actual = get_all_models()
        .iter()
        .filter(|model| model.search_tool_enabled_by_default)
        .map(|model| model.id.clone())
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(actual, expected);

    for id in [
        "google-gemini-3-1-flash-lite-text",
        "google-gemini-3-5-flash-lite-vision",
        "google-gemini-3-6-flash-text",
    ] {
        assert!(model_supports_search_by_id_with_custom(id, &[]), "{id}");
        assert!(
            !model_search_tool_enabled_by_default_by_id_with_custom(id, &[]),
            "{id}"
        );
    }
}

#[test]
fn behavioral_model_names_match_the_shared_presentation_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/model-catalog/presentation.json"
    )))
    .expect("model catalog presentation fixture parses");
    let cases = fixture["localized_name_cases"]
        .as_array()
        .expect("localized_name_cases must be an array");

    for case in cases {
        let model_id = case["model_id"]
            .as_str()
            .expect("model_id must be a string");
        let model = get_model_by_id(model_id).expect("fixture model must exist");
        for language in ["vi", "ko", "en"] {
            assert_eq!(
                model.localized_name(language),
                case[language]
                    .as_str()
                    .expect("localized fixture name must be a string"),
                "localized name drifted for {model_id} in {language}"
            );
        }
    }
}
