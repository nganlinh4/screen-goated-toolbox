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
}

struct RefineCatalogState {
    text_priority: Vec<String>,
    custom_models: Vec<CustomModelDefinition>,
    saved_openrouter_api_key: String,
    saved_cerebras_api_key: String,
    use_groq: bool,
    use_gemini: bool,
    use_openrouter: bool,
    use_cerebras: bool,
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
            saved_cerebras_api_key: String::new(),
            use_groq: crate::model_config::DEFAULT_USE_GROQ,
            use_gemini: crate::model_config::DEFAULT_USE_GEMINI,
            use_openrouter: crate::model_config::DEFAULT_USE_OPENROUTER,
            use_cerebras: crate::model_config::DEFAULT_USE_CEREBRAS,
        }
    }
}

impl RefineCatalogState {
    fn load() -> Self {
        crate::APP
            .lock()
            .ok()
            .map(|app| Self {
                text_priority: app.config.model_priority_chains.text_to_text.clone(),
                custom_models: app.config.custom_models.clone(),
                saved_openrouter_api_key: app.config.openrouter_api_key.clone(),
                saved_cerebras_api_key: app.config.cerebras_api_key.clone(),
                use_groq: app.config.use_groq,
                use_gemini: app.config.use_gemini,
                use_openrouter: app.config.use_openrouter,
                use_cerebras: app.config.use_cerebras,
            })
            .unwrap_or_default()
    }

    fn preferred_text_model(
        &self,
        groq_api_key: &str,
        gemini_api_key: &str,
        openrouter_api_key: &str,
        cerebras_api_key: &str,
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
                        cerebras_api_key,
                    )
            })
            .map(|model| (model.id, model.provider))
    }

    fn provider_is_available(
        &self,
        model: &ModelConfig,
        groq_api_key: &str,
        gemini_api_key: &str,
        openrouter_api_key: &str,
        cerebras_api_key: &str,
    ) -> bool {
        match Provider::from_wire(&model.provider) {
            Some(Provider::Google | Provider::GeminiLive) => {
                self.use_gemini && !gemini_api_key.trim().is_empty()
            }
            Some(Provider::Cerebras) => self.use_cerebras && !cerebras_api_key.trim().is_empty(),
            Some(Provider::OpenRouter) => {
                self.use_openrouter && !openrouter_api_key.trim().is_empty()
            }
            Some(Provider::Groq) => self.use_groq && !groq_api_key.trim().is_empty(),
            Some(Provider::Taalas) => true,
            _ => false,
        }
    }
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

    let catalog_state = RefineCatalogState::load();
    let openrouter_api_key = crate::api::provider_credentials::resolve(
        "OPENROUTER_API_KEY",
        &catalog_state.saved_openrouter_api_key,
    );
    let cerebras_api_key = crate::api::provider_credentials::resolve(
        "CEREBRAS_API_KEY",
        &catalog_state.saved_cerebras_api_key,
    );

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
            } else if let Some(model) = catalog_state.preferred_text_model(
                groq_api_key,
                gemini_api_key,
                &openrouter_api_key,
                &cerebras_api_key,
            ) {
                model
            } else {
                (original_model_id.to_string(), original_provider.to_string())
            }
        }
    };

    if let Some(conf) = crate::model_config::get_model_by_id_with_custom(
        &target_id_or_name,
        &catalog_state.custom_models,
    ) {
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
            previous_text: &previous_text,
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
                let mime = "image/jpeg".to_string();
                crate::api::gemini_live::gemini_live_generate(
                    crate::api::gemini_live::GeminiLiveGenerateRequest {
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

// --- TEXT-ONLY REFINEMENT ---
struct RefineTextOnlyRequest<'a, F> {
    groq_api_key: &'a str,
    gemini_api_key: &'a str,
    openrouter_api_key: &'a str,
    cerebras_api_key: &'a str,
    final_prompt: &'a str,
    previous_text: &'a str,
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
        previous_text,
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
    } else if Provider::from_wire(&provider) == Some(Provider::Cerebras) {
        providers::refine_cerebras(
            providers::RefineCerebrasRequest {
                cerebras_api_key,
                final_prompt,
                previous_text,
                model: &model,
                streaming_enabled,
                ui_language,
                cancel_token,
            },
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

#[cfg(test)]
mod tests {
    use super::RefineCatalogState;

    #[test]
    fn fallback_selection_follows_priority_and_provider_availability() {
        let mut state = RefineCatalogState::default();
        assert_eq!(
            state
                .preferred_text_model("groq", "gemini", "openrouter", "cerebras")
                .map(|model| model.0),
            Some("cerebras-zai-glm-4-7-text".to_string())
        );

        state.use_cerebras = false;
        assert_eq!(
            state
                .preferred_text_model("groq", "gemini", "openrouter", "")
                .map(|model| model.0),
            Some("groq-gpt-oss-20b-text".to_string())
        );

        state.use_groq = false;
        assert_eq!(
            state
                .preferred_text_model("", "gemini", "openrouter", "")
                .map(|model| model.0),
            Some("google-gemini-3-5-flash-lite-text".to_string())
        );

        state.use_gemini = false;
        assert_eq!(
            state
                .preferred_text_model("", "", "openrouter", "")
                .map(|model| model.0),
            Some("openrouter-nemotron-3-nano-omni-30b-a3b-text".to_string())
        );
    }
}
