// --- TEXT REFINEMENT ---
// Streaming text refinement with multiple LLM providers.

mod providers;

use crate::api::providers::Provider;
use crate::api::vision::translate_image_streaming as vision_translate_image_streaming;
use crate::config::types::CustomModelDefinition;
use crate::model_config::{ModelConfig, ModelType};
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
    pub on_model_selected: &'a mut dyn FnMut(&str, &str, &str),
}

struct RefineCatalogState {
    text_priority: Vec<String>,
    custom_models: Vec<CustomModelDefinition>,
    saved_openrouter_api_key: String,
    saved_nvidia_api_key: String,
    ollama_base_url: String,
    use_groq: bool,
    use_gemini: bool,
    use_openrouter: bool,
    use_nvidia: bool,
    use_ollama: bool,
}

impl Default for RefineCatalogState {
    fn default() -> Self {
        Self {
            text_priority: crate::model_config::default_text_to_text_priority_chain_ids()
                .iter()
                .map(|id| (*id).to_string())
                .collect(),
            custom_models: Vec::new(),
            saved_openrouter_api_key: String::new(),
            saved_nvidia_api_key: String::new(),
            ollama_base_url: "http://localhost:11434".to_string(),
            use_groq: crate::model_config::DEFAULT_USE_GROQ,
            use_gemini: crate::model_config::DEFAULT_USE_GEMINI,
            use_openrouter: crate::model_config::DEFAULT_USE_OPENROUTER,
            use_nvidia: crate::model_config::DEFAULT_USE_NVIDIA,
            use_ollama: crate::model_config::DEFAULT_USE_OLLAMA,
        }
    }
}

impl RefineCatalogState {
    fn load() -> Self {
        crate::APP
            .lock()
            .ok()
            .map(|app| Self {
                text_priority: crate::retry_model_chain::RetryChainKind::TextToText
                    .effective_chain(&app.config),
                custom_models: app.config.custom_models.clone(),
                saved_openrouter_api_key: app.config.openrouter_api_key.clone(),
                saved_nvidia_api_key: app.config.nvidia_api_key.clone(),
                ollama_base_url: app.config.ollama_base_url.clone(),
                use_groq: app.config.use_groq,
                use_gemini: app.config.use_gemini,
                use_openrouter: app.config.use_openrouter,
                use_nvidia: app.config.use_nvidia,
                use_ollama: app.config.use_ollama,
            })
            .unwrap_or_default()
    }

    fn preferred_text_model(
        &self,
        groq_api_key: &str,
        gemini_api_key: &str,
        openrouter_api_key: &str,
        nvidia_api_key: &str,
    ) -> Option<(String, String)> {
        self.text_priority
            .iter()
            .filter_map(|id| {
                crate::model_config::get_model_by_id_with_custom(id, &self.custom_models)
            })
            .find(|model| {
                model.enabled
                    && model.model_type == ModelType::Text
                    && self.provider_is_available(
                        model,
                        groq_api_key,
                        gemini_api_key,
                        openrouter_api_key,
                        nvidia_api_key,
                    )
            })
            .map(|model| (model.id, model.provider))
    }

    fn provider_enabled(&self, provider: &str) -> bool {
        match Provider::from_wire(provider) {
            Some(Provider::Google | Provider::GeminiLive) => self.use_gemini,
            Some(Provider::Groq) => self.use_groq,
            Some(Provider::OpenRouter) => self.use_openrouter,
            Some(Provider::Nvidia) => self.use_nvidia,
            Some(Provider::Ollama) => self.use_ollama,
            Some(Provider::Taalas | Provider::GoogleGtx | Provider::Qrserver) | None => true,
        }
    }

    fn provider_is_available(
        &self,
        model: &ModelConfig,
        groq_api_key: &str,
        gemini_api_key: &str,
        openrouter_api_key: &str,
        nvidia_api_key: &str,
    ) -> bool {
        match Provider::from_wire(&model.provider) {
            Some(Provider::Google | Provider::GeminiLive) => {
                self.use_gemini && !gemini_api_key.trim().is_empty()
            }
            Some(Provider::OpenRouter) => {
                self.use_openrouter && !openrouter_api_key.trim().is_empty()
            }
            Some(Provider::Nvidia) => self.use_nvidia && !nvidia_api_key.trim().is_empty(),
            Some(Provider::Groq) => self.use_groq && !groq_api_key.trim().is_empty(),
            Some(Provider::Taalas) => true,
            Some(Provider::Ollama) => self.use_ollama,
            _ => false,
        }
    }
}

pub fn refine_text_streaming<F>(request: RefineTextRequest<'_>, mut on_chunk: F) -> Result<String>
where
    F: FnMut(&str),
{
    let mut normalizer = crate::api::output_normalization::InitialLineBreakNormalizer::default();
    let result = {
        let mut normalized_on_chunk = |chunk: &str| match normalizer.observe(chunk) {
            crate::api::output_normalization::NormalizedChunk::Paint(text) => on_chunk(text),
            crate::api::output_normalization::NormalizedChunk::Replace(text) => {
                on_chunk(&format!("{}{text}", crate::api::WIPE_SIGNAL));
            }
            crate::api::output_normalization::NormalizedChunk::Suppress => {}
        };
        refine_text_streaming_inner(request, &mut normalized_on_chunk)
    };
    result.map(|output| normalizer.finish(output))
}

fn refine_text_streaming_inner<F>(request: RefineTextRequest<'_>, on_chunk: F) -> Result<String>
where
    F: FnMut(&str),
{
    refine_text_with_catalog(request, &RefineCatalogState::load(), on_chunk)
}

fn refine_text_with_catalog<F>(
    request: RefineTextRequest<'_>,
    catalog_state: &RefineCatalogState,
    mut on_chunk: F,
) -> Result<String>
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
        on_model_selected,
    } = request;

    if !original_model_id.trim().is_empty()
        && crate::model_config::get_model_by_id_with_custom(
            original_model_id,
            &catalog_state.custom_models,
        )
        .is_none()
    {
        anyhow::bail!("MODEL_UNAVAILABLE:{original_model_id}");
    }

    let nvidia_api_key = crate::api::provider_credentials::resolve(
        "NVIDIA_API_KEY",
        &catalog_state.saved_nvidia_api_key,
    );
    let openrouter_api_key = crate::api::provider_credentials::resolve(
        "OPENROUTER_API_KEY",
        &catalog_state.saved_openrouter_api_key,
    );

    let final_prompt = format!(
        "Content:\n{}\n\nInstruction:\n{}\n\nOutput ONLY the result.",
        previous_text, user_prompt
    );

    let source_non_llm = original_model_is_non_llm(original_model_id, original_provider);
    let (mut target_id_or_name, mut target_provider) = match context {
        RefineContext::Image(_) if !source_non_llm => {
            (original_model_id.to_string(), original_provider.to_string())
        }
        _ => {
            if original_model_can_refine_text(original_model_id, original_provider) {
                (original_model_id.to_string(), original_provider.to_string())
            } else if let Some(model) = catalog_state.preferred_text_model(
                groq_api_key,
                gemini_api_key,
                &openrouter_api_key,
                &nvidia_api_key,
            ) {
                model
            } else {
                (original_model_id.to_string(), original_provider.to_string())
            }
        }
    };

    let target_model_id = target_id_or_name.clone();
    if let Some(conf) = crate::model_config::get_model_by_id_with_custom(
        &target_id_or_name,
        &catalog_state.custom_models,
    ) {
        target_id_or_name = conf.full_name;
        target_provider = conf.provider;
    }

    on_model_selected(&target_model_id, &target_id_or_name, &target_provider);
    if !catalog_state.provider_enabled(&target_provider) {
        anyhow::bail!("PROVIDER_DISABLED:{target_provider}");
    }

    let mut exec_text_only = |p_model: String, p_provider: String| -> Result<String> {
        refine_text_only(RefineTextOnlyRequest {
            groq_api_key,
            gemini_api_key,
            openrouter_api_key: &openrouter_api_key,
            nvidia_api_key: &nvidia_api_key,
            ollama_base_url: &catalog_state.ollama_base_url,
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
        RefineContext::Image(_) if source_non_llm => {
            exec_text_only(target_id_or_name, target_provider)
        }
        RefineContext::Image(img_bytes) => {
            if Provider::from_wire(&target_provider) == Some(Provider::Google) {
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
                        response_schema: None,
                        cancel_token,
                        request_timeout: None,
                    },
                    on_chunk,
                )
            } else if Provider::from_wire(&target_provider) == Some(Provider::GeminiLive) {
                let mime = original_image_mime_type(&img_bytes)?.to_string();
                crate::api::gemini_live::gemini_live_generate(
                    crate::api::gemini_live::GeminiLiveGenerateRequest {
                        api_key: gemini_api_key.to_string(),
                        model: target_id_or_name.clone(),
                        text: final_prompt,
                        instruction: String::new(),
                        image_data: Some((img_bytes.clone(), mime)),
                        audio_data: None,
                        streaming_enabled,
                        ui_language,
                        cancel_token: cancel_token.clone(),
                        request_timeout: None,
                    },
                    &mut on_chunk,
                )
            } else {
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
                        response_schema: None,
                        cancel_token,
                        request_timeout: None,
                    },
                    on_chunk,
                )
            }
        }
        RefineContext::Audio(_) => exec_text_only(target_id_or_name, target_provider),
        RefineContext::None => exec_text_only(target_id_or_name, target_provider),
    }
}

fn original_model_can_refine_text(model_id: &str, provider: &str) -> bool {
    !model_id.trim().is_empty()
        && model_id != crate::model_config::DEFAULT_IMAGE_MODEL_ID
        && !original_model_is_non_llm(model_id, provider)
}

fn original_model_is_non_llm(model_id: &str, provider: &str) -> bool {
    crate::model_config::model_is_non_llm(model_id)
        || matches!(
            Provider::from_wire(provider),
            Some(Provider::GoogleGtx | Provider::Qrserver)
        )
}

fn original_image_mime_type(bytes: &[u8]) -> Result<&'static str> {
    Ok(image::guess_format(bytes)?.to_mime_type())
}

// --- TEXT-ONLY REFINEMENT ---
struct RefineTextOnlyRequest<'a, F> {
    groq_api_key: &'a str,
    gemini_api_key: &'a str,
    openrouter_api_key: &'a str,
    nvidia_api_key: &'a str,
    ollama_base_url: &'a str,
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
        nvidia_api_key,
        ollama_base_url,
        final_prompt,
        model,
        provider,
        streaming_enabled,
        ui_language,
        cancel_token,
        on_chunk,
    } = request;

    if Provider::from_wire(&provider) == Some(Provider::Google) {
        providers::refine_gemini(
            gemini_api_key,
            final_prompt,
            &model,
            streaming_enabled,
            ui_language,
            cancel_token,
            on_chunk,
        )
    } else if Provider::from_wire(&provider) == Some(Provider::GeminiLive) {
        crate::api::gemini_live::gemini_live_generate(
            crate::api::gemini_live::GeminiLiveGenerateRequest {
                api_key: gemini_api_key.to_string(),
                model,
                text: final_prompt.to_string(),
                instruction: String::new(),
                image_data: None,
                audio_data: None,
                streaming_enabled,
                ui_language,
                cancel_token: cancel_token.clone(),
                request_timeout: None,
            },
            on_chunk,
        )
    } else if Provider::from_wire(&provider) == Some(Provider::Taalas) {
        providers::refine_taalas(final_prompt, cancel_token, on_chunk)
    } else if Provider::from_wire(&provider) == Some(Provider::Nvidia) {
        providers::refine_nvidia(
            nvidia_api_key,
            final_prompt,
            &model,
            streaming_enabled,
            ui_language,
            cancel_token,
            on_chunk,
        )
    } else if Provider::from_wire(&provider) == Some(Provider::OpenRouter) {
        providers::refine_openrouter(
            openrouter_api_key,
            final_prompt,
            &model,
            streaming_enabled,
            ui_language,
            cancel_token,
            on_chunk,
        )
    } else if Provider::from_wire(&provider) == Some(Provider::Ollama) {
        if cancel_token
            .as_ref()
            .is_some_and(|token| token.load(std::sync::atomic::Ordering::SeqCst))
        {
            anyhow::bail!("Refinement cancelled");
        }
        crate::api::ollama::ollama_generate_text_in_chain(
            crate::api::ollama::OllamaChainTextRequest {
                base_url: ollama_base_url,
                model: &model,
                prompt: final_prompt,
                streaming_enabled,
                ui_language,
                cancel_token,
                request_timeout: None,
            },
            on_chunk,
        )
    } else if Provider::from_wire(&provider) == Some(Provider::Groq) {
        providers::refine_groq(
            groq_api_key,
            final_prompt,
            &model,
            streaming_enabled,
            ui_language,
            cancel_token,
            on_chunk,
        )
    } else {
        anyhow::bail!("Unsupported refinement provider: {provider}")
    }
}

#[cfg(test)]
mod tests;
