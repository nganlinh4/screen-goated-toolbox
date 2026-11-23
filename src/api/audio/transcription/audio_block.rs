use super::{groq::upload_audio_to_whisper, transcribe_audio_gemini, transcribe_with_gemini_live};
use crate::api::providers::Provider;
use crate::model_config::{get_model_by_id, is_gemini_live_translate_model_id, model_is_non_llm};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(crate) fn execute_audio_block_cancellable(
    audio_block: &crate::config::ProcessingBlock,
    wav_data: &[u8],
    cancelled: Arc<AtomicBool>,
) -> anyhow::Result<String> {
    anyhow::ensure!(
        !cancelled.load(Ordering::SeqCst),
        "Audio processing cancelled"
    );
    let model_config = get_model_by_id(&audio_block.model);
    let model_config = match model_config {
        Some(c) => c,
        None => {
            return Err(anyhow::anyhow!(
                "Model config not found for audio model: {}",
                audio_block.model
            ));
        }
    };
    let model_name = model_config.full_name.clone();
    let provider = model_config.provider.clone();

    let (groq_api_key, gemini_api_key) = {
        let app = crate::APP.lock().unwrap();
        (
            app.config.api_key.clone(),
            app.config.gemini_api_key.clone(),
        )
    };

    // Use block's prompt and language settings
    let mut final_prompt = if model_is_non_llm(&audio_block.model) {
        String::new()
    } else {
        audio_block.prompt.clone()
    };

    for (key, value) in &audio_block.language_vars {
        let pattern = format!("{{{}}}", key);
        final_prompt = final_prompt.replace(&pattern, value);
    }

    if final_prompt.contains("{language1}") && !audio_block.language_vars.contains_key("language1")
    {
        final_prompt = final_prompt.replace("{language1}", &audio_block.selected_language);
    }

    final_prompt = final_prompt.replace("{language}", &audio_block.selected_language);

    if Provider::from_wire(&provider) == Some(Provider::Groq) {
        if groq_api_key.trim().is_empty() {
            Err(anyhow::anyhow!("NO_API_KEY:groq"))
        } else {
            upload_audio_to_whisper(
                &groq_api_key,
                &model_name,
                wav_data,
                audio_block
                    .language_vars
                    .get(crate::config::speech_languages::INPUT_LANGUAGE_KEY)
                    .map(String::as_str),
            )
        }
    } else if Provider::from_wire(&provider) == Some(Provider::Google) {
        if model_config.model_type != crate::model_config::ModelType::Audio {
            // Audio support is catalog metadata, not a model-name substring.
            Err(anyhow::anyhow!("Unsupported audio model: {}", model_name))
        } else if gemini_api_key.trim().is_empty() {
            Err(anyhow::anyhow!("NO_API_KEY:google"))
        } else {
            transcribe_audio_gemini(
                &gemini_api_key,
                final_prompt,
                model_name,
                wav_data,
                Some(cancelled.clone()),
            )
        }
    } else if Provider::from_wire(&provider) == Some(Provider::GeminiLive) {
        // Gemini Live API (WebSocket-based).
        if gemini_api_key.trim().is_empty() {
            Err(anyhow::anyhow!("NO_API_KEY:gemini"))
        } else {
            let (target_language, prompt) = live_audio_arguments(audio_block, &final_prompt);
            transcribe_with_gemini_live(
                &gemini_api_key,
                &model_name,
                target_language,
                prompt,
                wav_data,
                Some(&cancelled),
            )
        }
    } else {
        Err(anyhow::anyhow!("Unsupported audio provider: {}", provider))
    }
}

fn live_audio_arguments<'a>(
    block: &'a crate::config::ProcessingBlock,
    resolved_prompt: &'a str,
) -> (Option<&'a str>, Option<&'a str>) {
    if !model_is_non_llm(&block.model) {
        (None, Some(resolved_prompt))
    } else if is_gemini_live_translate_model_id(&block.model) {
        (Some(&block.selected_language), None)
    } else {
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_audio_route_follows_the_catalog_model_type() {
        let text_model = "google-gemma-4-31b-text";
        assert_eq!(
            get_model_by_id(text_model).map(|model| model.model_type),
            Some(crate::model_config::ModelType::Text)
        );
        let block = crate::config::ProcessingBlock {
            model: text_model.into(),
            ..Default::default()
        };
        let error = execute_audio_block_cancellable(&block, &[], Arc::new(AtomicBool::new(false)))
            .unwrap_err();
        assert!(
            error.to_string().starts_with("Unsupported audio model"),
            "{error}"
        );
    }

    #[test]
    fn finite_live_audio_keeps_the_selected_operation_and_arguments() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/preset-system/live-audio-routing.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let id = case["model_id"].as_str().unwrap();
            let model = get_model_by_id(id).unwrap();
            if model.provider != "gemini-live" {
                continue;
            }
            let block = crate::config::ProcessingBlock {
                model: id.into(),
                selected_language: fixture["target_language"].as_str().unwrap().into(),
                ..Default::default()
            };
            let resolved_prompt = "Resolved instruction with selected language";
            let (target, prompt) = live_audio_arguments(&block, resolved_prompt);
            if case["translate_to_selected_language"].as_bool().unwrap() {
                assert_eq!(target, Some(block.selected_language.as_str()), "{id}");
                assert_eq!(prompt, None, "{id}");
                let setup = crate::api::realtime_audio::websocket::build_live_translate_setup_value(
                    &model.full_name,
                    target.unwrap(),
                );
                assert_eq!(
                    setup["setup"]["generationConfig"]["translationConfig"]["targetLanguageCode"],
                    fixture["target_language_code"],
                    "{id}"
                );
                assert!(setup["setup"]["inputAudioTranscription"].is_object());
                assert!(setup["setup"]["outputAudioTranscription"].is_object());
            } else if case["gemini_input_stream"].as_bool().unwrap() {
                assert_eq!((target, prompt), (None, None), "{id}");
            } else {
                assert_eq!(target, None, "{id}");
                assert_eq!(prompt, Some(resolved_prompt), "{id}");
            }
        }
    }
}
