// --- TEXT REFINEMENT ---
// Streaming text refinement with multiple LLM providers.

mod providers;

use crate::api::vision::translate_image_streaming as vision_translate_image_streaming;
use crate::overlay::result::RefineContext;
use anyhow::Result;
use std::sync::{Arc, atomic::AtomicBool};

pub struct RefineTextRequest<'a> {
    pub groq_api_key: &'a str,
    pub gemini_api_key: &'a str,
    pub context: RefineContext,
    pub previous_text: String,
    pub user_prompt: String,
    pub original_model_id: &'a str,
    pub original_provider: &'a str,
    pub streaming_enabled: bool,
    pub ui_language: &'a str,
    pub cancel_token: Option<Arc<AtomicBool>>,
}

pub fn refine_text_streaming<F>(request: RefineTextRequest<'_>, mut on_chunk: F) -> Result<String>
where
    F: FnMut(&str),
{
    let RefineTextRequest {
        groq_api_key,
        gemini_api_key,
        context,
        previous_text,
        user_prompt,
        original_model_id,
        original_provider,
        streaming_enabled,
        ui_language,
        cancel_token,
    } = request;

    let openrouter_api_key = crate::APP
        .lock()
        .ok()
        .and_then(|app| {
            let config = app.config.clone();
            if config.openrouter_api_key.is_empty() {
                None
            } else {
                Some(config.openrouter_api_key.clone())
            }
        })
        .unwrap_or_default();

    let cerebras_api_key = crate::APP
        .lock()
        .ok()
        .and_then(|app| {
            let config = app.config.clone();
            if config.cerebras_api_key.is_empty() {
                None
            } else {
                Some(config.cerebras_api_key.clone())
            }
        })
        .unwrap_or_default();

    let final_prompt = format!(
        "Content:\n{}\n\nInstruction:\n{}\n\nOutput ONLY the result.",
        previous_text, user_prompt
    );

    let (mut target_id_or_name, mut target_provider) = match context {
        RefineContext::Image(_) => (original_model_id.to_string(), original_provider.to_string()),
        _ => {
            if !original_model_id.trim().is_empty()
                && original_model_id != crate::model_config::DEFAULT_IMAGE_MODEL_ID
            {
                (original_model_id.to_string(), original_provider.to_string())
            } else if !gemini_api_key.trim().is_empty() {
                ("gemini-flash-lite".to_string(), "google".to_string())
            } else if !cerebras_api_key.trim().is_empty() {
                (
                    crate::model_config::DEFAULT_CEREBRAS_TEXT_API_MODEL.to_string(),
                    "cerebras".to_string(),
                )
            } else if !groq_api_key.trim().is_empty() {
                ("text_accurate_kimi".to_string(), "groq".to_string())
            } else {
                (original_model_id.to_string(), original_provider.to_string())
            }
        }
    };

    if let Some(conf) = crate::model_config::get_model_by_id(&target_id_or_name) {
        target_id_or_name = conf.full_name;
        target_provider = conf.provider;
    }

    let mut exec_text_only = |p_model: String, p_provider: String| -> Result<String> {
        refine_text_only(RefineTextOnlyRequest {
            groq_api_key,
            gemini_api_key,
            openrouter_api_key: &openrouter_api_key,
            cerebras_api_key: &cerebras_api_key,
            final_prompt: &final_prompt,
            model: p_model,
            provider: p_provider,
            streaming_enabled,
            ui_language,
            cancel_token: &cancel_token,
            on_chunk: &mut on_chunk,
        })
    };

    match context {
        RefineContext::Image(img_bytes) => {
            if target_provider == "google" {
                if gemini_api_key.trim().is_empty() {
                    return Err(anyhow::anyhow!("NO_API_KEY:gemini"));
                }
                let img = image::load_from_memory(&img_bytes)?.to_rgba8();
                vision_translate_image_streaming(
                    crate::api::TranslateImageRequest {
                        groq_api_key,
                        gemini_api_key,
                        prompt: final_prompt,
                        model: target_id_or_name,
                        provider: target_provider,
                        image: img,
                        original_bytes: Some(img_bytes.clone()),
                        streaming_enabled,
                        use_json_format: false,
                        cancel_token,
                    },
                    on_chunk,
                )
            } else if target_provider == "gemini-live" {
                let mime = "image/jpeg".to_string();
                crate::api::gemini_live::gemini_live_generate(
                    target_id_or_name.clone(),
                    final_prompt,
                    String::new(),
                    Some((img_bytes.clone(), mime)),
                    None,
                    streaming_enabled,
                    ui_language,
                    &mut on_chunk,
                )
            } else {
                if groq_api_key.trim().is_empty() {
                    return Err(anyhow::anyhow!("NO_API_KEY:groq"));
                }
                let img = image::load_from_memory(&img_bytes)?.to_rgba8();
                vision_translate_image_streaming(
                    crate::api::TranslateImageRequest {
                        groq_api_key,
                        gemini_api_key,
                        prompt: final_prompt,
                        model: target_id_or_name,
                        provider: target_provider,
                        image: img,
                        original_bytes: Some(img_bytes.clone()),
                        streaming_enabled,
                        use_json_format: false,
                        cancel_token,
                    },
                    on_chunk,
                )
            }
        }
        RefineContext::Audio(_) => exec_text_only(target_id_or_name, target_provider),
        RefineContext::None => exec_text_only(target_id_or_name, target_provider),
    }
}

// --- TEXT-ONLY REFINEMENT ---
struct RefineTextOnlyRequest<'a, F> {
    groq_api_key: &'a str,
    gemini_api_key: &'a str,
    openrouter_api_key: &'a str,
    cerebras_api_key: &'a str,
    final_prompt: &'a str,
    model: String,
    provider: String,
    streaming_enabled: bool,
    ui_language: &'a str,
    cancel_token: &'a Option<Arc<AtomicBool>>,
    on_chunk: &'a mut F,
}

fn refine_text_only<F>(request: RefineTextOnlyRequest<'_, F>) -> Result<String>
where
    F: FnMut(&str),
{
    let RefineTextOnlyRequest {
        groq_api_key,
        gemini_api_key,
        openrouter_api_key,
        cerebras_api_key,
        final_prompt,
        model,
        provider,
        streaming_enabled,
        ui_language,
        cancel_token,
        on_chunk,
    } = request;

    if provider == "google" {
        providers::refine_gemini(
            gemini_api_key,
            final_prompt,
            &model,
            streaming_enabled,
            ui_language,
            cancel_token,
            on_chunk,
        )
    } else if provider == "gemini-live" {
        crate::api::gemini_live::gemini_live_generate(
            model,
            final_prompt.to_string(),
            String::new(),
            None,
            None,
            streaming_enabled,
            ui_language,
            on_chunk,
        )
    } else if provider == "taalas" {
        providers::refine_taalas(final_prompt, cancel_token, on_chunk)
    } else if provider == "cerebras" {
        providers::refine_cerebras(
            cerebras_api_key,
            final_prompt,
            &model,
            streaming_enabled,
            ui_language,
            cancel_token,
            on_chunk,
        )
    } else if provider == "openrouter" {
        providers::refine_openrouter(
            openrouter_api_key,
            final_prompt,
            &model,
            streaming_enabled,
            ui_language,
            cancel_token,
            on_chunk,
        )
    } else {
        providers::refine_groq(
            groq_api_key,
            final_prompt,
            &model,
            streaming_enabled,
            ui_language,
            cancel_token,
            on_chunk,
        )
    }
}
