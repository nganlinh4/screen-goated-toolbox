use super::*;

pub fn handle_get_narration_tts_metadata(
    _args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    crate::api::tts::edge_voices::load_edge_voices_async();
    let defaults = TtsPlaygroundSettings::default();

    let gemini_voices: Vec<serde_json::Value> = GEMINI_VOICES
        .iter()
        .map(|(name, gender)| serde_json::json!({ "name": name, "gender": gender }))
        .collect();

    let gemini_models: Vec<serde_json::Value> = tts_gemini_model_options()
        .iter()
        .map(|(api_model, label)| serde_json::json!({ "apiModel": api_model, "label": label }))
        .collect();

    let gemini_instruction_languages: Vec<serde_json::Value> =
        SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES
            .iter()
            .map(|(code, name)| serde_json::json!({ "languageCode": code, "languageName": name }))
            .collect();

    let default_language_conditions: Vec<serde_json::Value> = defaults
        .gemini_language_conditions
        .iter()
        .map(|condition| {
            serde_json::json!({
                "languageCode": condition.language_code,
                "languageName": condition.language_name,
                "instruction": condition.instruction,
            })
        })
        .collect();

    let default_edge_voice_configs: Vec<serde_json::Value> = defaults
        .edge_settings
        .voice_configs
        .iter()
        .map(|config| {
            serde_json::json!({
                "languageCode": config.language_code,
                "languageName": config.language_name,
                "voiceName": config.voice_name,
            })
        })
        .collect();

    let (edge_voice_state, edge_voice_error, edge_voice_languages, edge_voices_by_language) = {
        let cache = crate::api::tts::edge_voices::EDGE_VOICE_CACHE
            .lock()
            .map_err(|_| "Lock Edge TTS voice cache".to_string())?;
        let state = if cache.loaded {
            "loaded"
        } else if cache.loading {
            "loading"
        } else if cache.error.is_some() {
            "error"
        } else {
            "idle"
        };

        let mut language_names = std::collections::HashMap::<String, String>::new();
        for voice in &cache.voices {
            let lang_code = voice
                .locale
                .split('-')
                .next()
                .unwrap_or(&voice.locale)
                .to_lowercase();
            language_names.entry(lang_code).or_insert_with(|| {
                voice
                    .friendly_name
                    .rfind(" - ")
                    .and_then(|dash_pos| {
                        let lang_region = &voice.friendly_name[dash_pos + 3..];
                        lang_region
                            .find(" (")
                            .map(|paren_pos| lang_region[..paren_pos].to_string())
                            .or_else(|| Some(lang_region.to_string()))
                    })
                    .unwrap_or_else(|| voice.locale.clone())
            });
        }

        let mut languages: Vec<serde_json::Value> = language_names
            .iter()
            .map(|(code, name)| {
                serde_json::json!({
                    "languageCode": code,
                    "languageName": name,
                })
            })
            .collect();
        languages.sort_by(|left, right| {
            let left_name = left["languageName"].as_str().unwrap_or_default();
            let right_name = right["languageName"].as_str().unwrap_or_default();
            left_name.cmp(right_name)
        });

        let voices_by_language = cache
            .by_language
            .iter()
            .map(|(code, voices)| {
                let options: Vec<serde_json::Value> = voices
                    .iter()
                    .map(|voice| {
                        serde_json::json!({
                            "shortName": voice.short_name,
                            "gender": voice.gender,
                            "friendlyName": voice.friendly_name,
                            "locale": voice.locale,
                        })
                    })
                    .collect();
                (code.clone(), serde_json::Value::Array(options))
            })
            .collect::<serde_json::Map<_, _>>();

        (
            state,
            cache.error.clone(),
            languages,
            serde_json::Value::Object(voices_by_language),
        )
    };

    let providers: Vec<serde_json::Value> = narration_tts_providers()
        .map(|provider| {
            serde_json::json!({
                "method": provider.id,
                "label": provider.label,
            })
        })
        .collect();
    let kokoro_voice_languages: Vec<serde_json::Value> = SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES
        .iter()
        .filter(|(code, _)| normalize_kokoro_lang(code).is_some())
        .map(|(code, name)| serde_json::json!({ "languageCode": code, "languageName": name }))
        .collect();
    let kokoro_voices: Vec<serde_json::Value> = KOKORO_VOICES
        .iter()
        .map(|voice| {
            serde_json::json!({
                "id": voice.id,
                "label": voice.label,
                "languageCode": voice.language_code,
            })
        })
        .collect();
    let magpie_voices: Vec<serde_json::Value> = MAGPIE_VOICES
        .iter()
        .map(|voice| {
            serde_json::json!({
                "id": voice.id,
                "label": voice.label,
            })
        })
        .collect();
    let magpie_voice_languages: Vec<serde_json::Value> = MAGPIE_VOICE_LANGUAGES
        .iter()
        .map(|(code, name)| serde_json::json!({ "languageCode": code, "languageName": name }))
        .collect();
    let supertonic_languages: Vec<serde_json::Value> = SUPERTONIC_LANGUAGES
        .iter()
        .map(|lang| serde_json::json!({ "languageCode": lang.code, "languageName": lang.label }))
        .collect();
    let supertonic_voices: Vec<serde_json::Value> = SUPERTONIC_VOICES
        .iter()
        .map(|voice| {
            serde_json::json!({
                "id": voice.id,
                "label": voice.label,
            })
        })
        .collect();
    let step_audio_reference_voices: Vec<serde_json::Value> = crate::APP
        .lock()
        .map(|app| {
            app.config
                .step_audio_reference_voices
                .iter()
                .map(|reference| {
                    serde_json::json!({
                        "id": reference.id,
                        "label": reference.label,
                        "audioPath": reference.audio_path,
                        "transcript": reference.transcript,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let step_audio_voices: Vec<serde_json::Value> = step_audio_reference_voices
        .iter()
        .filter_map(|voice| {
            let id = voice.get("id")?.as_str()?;
            let label = voice
                .get("label")
                .and_then(|value| value.as_str())
                .unwrap_or(id);
            Some(serde_json::json!({
                "id": id,
                "label": if label.trim().is_empty() { "Untitled reference" } else { label },
            }))
        })
        .collect();
    let step_audio_voice_languages: Vec<serde_json::Value> = defaults
        .step_audio_settings
        .voice_configs
        .iter()
        .map(|config| {
            serde_json::json!({
                "languageCode": config.language_code,
                "languageName": config.language_name,
            })
        })
        .collect();
    let default_method = tts_method_id(&defaults.method);
    let default_magpie_voice_configs: Vec<serde_json::Value> = defaults
        .magpie_settings
        .voice_configs
        .iter()
        .map(|config| {
            serde_json::json!({
                "languageCode": config.language_code,
                "languageName": config.language_name,
                "voiceId": config.voice_id,
            })
        })
        .collect();
    let default_kokoro_voice_configs: Vec<serde_json::Value> = defaults
        .kokoro_settings
        .voice_configs
        .iter()
        .map(|config| {
            serde_json::json!({
                "languageCode": config.language_code,
                "languageName": config.language_name,
                "voiceId": config.voice_id,
            })
        })
        .collect();
    let default_supertonic_voice_configs: Vec<serde_json::Value> = defaults
        .supertonic_settings
        .voice_configs
        .iter()
        .map(|config| {
            serde_json::json!({
                "languageCode": config.language_code,
                "languageName": config.language_name,
                "voiceId": config.voice_id,
            })
        })
        .collect();
    let defaults_json = serde_json::json!({
        "method": default_method,
        "geminiModel": defaults.gemini_model,
        "geminiVoice": defaults.gemini_voice,
        "geminiSpeed": defaults.gemini_speed,
        "geminiInstruction": defaults.gemini_instruction,
        "geminiLanguageConditions": default_language_conditions,
        "geminiParallelRequests": default_gemini_parallel_requests(),
        "geminiS2sParallelRequests": default_gemini_s2s_parallel_requests(),
        "googleSpeed": defaults.google_speed,
        "edgeVoice": defaults.edge_voice,
        "edgePitch": defaults.edge_settings.pitch,
        "edgeRate": defaults.edge_settings.rate,
        "edgeVoiceConfigs": default_edge_voice_configs,
        "stepAudioVoice": defaults.step_audio_settings.voice,
        "stepAudioReferenceVoiceId": defaults.step_audio_settings.reference_voice_id,
        "stepAudioPromptText": defaults.step_audio_settings.style_prompt,
        "stepAudioUseCustomReference": defaults.step_audio_settings.use_custom_reference,
        "stepAudioReferenceAudioPath": defaults.step_audio_settings.reference_audio_path,
        "stepAudioReferenceText": defaults.step_audio_settings.reference_text,
        "stepAudioReferenceLabel": defaults.step_audio_settings.reference_label,
        "magpieVoice": defaults.magpie_settings.voice,
        "magpieVoiceConfigs": default_magpie_voice_configs,
        "kokoroVoice": defaults.kokoro_settings.voice,
        "kokoroSpeed": defaults.kokoro_settings.speed,
        "kokoroNumThreads": defaults.kokoro_settings.num_threads,
        "kokoroVoiceConfigs": default_kokoro_voice_configs,
        "supertonicSpeed": defaults.supertonic_settings.speed,
        "supertonicNumSteps": defaults.supertonic_settings.num_steps,
        "supertonicNumThreads": defaults.supertonic_settings.num_threads,
        "supertonicVoiceConfigs": default_supertonic_voice_configs,
        "vieneuVariant": defaults.vieneu_settings.variant,
        "vieneuEmotion": defaults.vieneu_settings.emotion,
        "vieneuReferenceVoiceId": defaults.vieneu_settings.reference_voice_id,
    });

    Ok(serde_json::json!({
        "providers": providers,
        "geminiVoices": gemini_voices,
        "geminiModels": gemini_models,
        "geminiInstructionLanguages": gemini_instruction_languages,
        "geminiSpeedOptions": ["Slow", "Normal", "Fast"],
        "googleSpeedOptions": ["Slow", "Normal"],
        "kokoroVoices": kokoro_voices,
        "kokoroVoiceLanguages": kokoro_voice_languages,
        "magpieVoices": magpie_voices,
        "magpieVoiceLanguages": magpie_voice_languages,
        "supertonicLanguages": supertonic_languages,
        "supertonicVoices": supertonic_voices,
        "stepAudioVoices": step_audio_voices,
        "stepAudioVoiceLanguages": step_audio_voice_languages,
        "stepAudioReferenceVoices": step_audio_reference_voices,
        "edgeVoiceState": edge_voice_state,
        "edgeVoiceError": edge_voice_error,
        "edgeVoiceLanguages": edge_voice_languages,
        "edgeVoicesByLanguage": edge_voices_by_language,
        "defaults": defaults_json,
    }))
}
