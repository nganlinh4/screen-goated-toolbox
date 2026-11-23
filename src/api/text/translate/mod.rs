// --- TEXT TRANSLATION ---
// Streaming text translation with multiple LLM providers.

mod groq;
#[cfg(test)]
mod live_tests;
mod providers;
#[cfg(test)]
mod recovery_tests;

use crate::api::providers::Provider;
use anyhow::Result;
use providers::{translate_gemini, translate_nvidia, translate_openrouter, translate_taalas};
use std::sync::{Arc, atomic::AtomicBool};

pub fn supports_structured_translation(provider: &str) -> bool {
    matches!(
        Provider::from_wire(provider),
        Some(Provider::Groq | Provider::Google | Provider::OpenRouter | Provider::Nvidia)
    )
}

#[derive(Clone, Copy)]
pub enum TranslationSchema<'a> {
    /// The caller validates completed items and retries unresolved items.
    /// Providers without schema streaming may use unconstrained text transport.
    LocallyValidated(&'a serde_json::Value),
}

pub struct TranslateTextRequest<'a> {
    pub groq_api_key: &'a str,
    pub gemini_api_key: &'a str,
    pub text: String,
    pub instruction: String,
    pub model: String,
    pub provider: String,
    pub streaming_enabled: bool,
    /// Best-effort hint that the caller expects a JSON object. Only a provider
    /// with a model-independent JSON-object mode (Groq) applies it; others
    /// receive the prompt unchanged. The reply is never reshaped, so the caller
    /// always validates it. Use `response_schema` when the shape must be enforced.
    pub use_json_format: bool,
    pub response_schema: Option<TranslationSchema<'a>>,
    /// Optional output cap for OpenAI-compatible translation providers.
    pub max_output_tokens: Option<u32>,
    /// Explicit authorization to invoke a provider web-search tool.
    pub search_enabled: bool,
    /// Capability of the exact selected model; absent callers use catalog endpoint data.
    pub search_capability: Option<bool>,
    /// Whether the selected endpoint invokes search without an explicit tool request.
    pub search_tool_enabled_by_default: Option<bool>,
    pub search_label: Option<String>,
    pub ui_language: &'a str,
    pub cancel_token: Option<Arc<AtomicBool>>,
    /// Optional end-to-end transport budget for latency-sensitive callers.
    /// `None` preserves the provider's normal shared-agent timeout.
    pub request_timeout: Option<crate::api::client::RequestTimeouts>,
    pub target_language: Option<String>,
}

#[derive(Clone, Copy)]
struct TranslateTransportOptions<'a> {
    locally_validated_schema: bool,
    max_output_tokens: Option<u32>,
    streaming_enabled: bool,
    ui_language: &'a str,
    cancel_token: &'a Option<Arc<AtomicBool>>,
    request_timeout: Option<crate::api::client::RequestTimeouts>,
    search_enabled: bool,
}

pub fn translate_text_streaming<F>(
    request: TranslateTextRequest<'_>,
    mut on_chunk: F,
) -> Result<String>
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
        translate_text_streaming_inner(request, &mut normalized_on_chunk)
    };
    result.map(|output| normalizer.finish(output))
}

/// Keep the provider router monomorphic. Callers still get a zero-cost generic
/// boundary, while the large transport/provider match is emitted only once.
#[inline(never)]
fn translate_text_streaming_inner(
    request: TranslateTextRequest<'_>,
    mut on_chunk: &mut dyn FnMut(&str),
) -> Result<String> {
    let TranslateTextRequest {
        groq_api_key,
        gemini_api_key,
        text,
        instruction,
        model,
        provider,
        streaming_enabled,
        use_json_format,
        response_schema,
        max_output_tokens,
        search_enabled,
        search_capability,
        search_tool_enabled_by_default,
        search_label,
        ui_language,
        cancel_token,
        request_timeout,
        target_language,
    } = request;

    // One lock, and only the fields this request reads: a full Config clone
    // copies every preset while the global APP lock is held.
    let (saved_openrouter_api_key, saved_nvidia_api_key, saved_ollama_base_url) = crate::APP
        .lock()
        .map(|app| {
            (
                app.config.openrouter_api_key.clone(),
                app.config.nvidia_api_key.clone(),
                Some(app.config.ollama_base_url.clone()),
            )
        })
        .unwrap_or_default();
    let openrouter_api_key =
        crate::api::provider_credentials::resolve("OPENROUTER_API_KEY", &saved_openrouter_api_key);

    let nvidia_api_key =
        crate::api::provider_credentials::resolve("NVIDIA_API_KEY", &saved_nvidia_api_key);

    let full_content;
    let prompt = format!("{}\n\n{}", instruction, text);
    let locally_validated_schema = matches!(
        response_schema,
        Some(TranslationSchema::LocallyValidated(_))
    );
    let response_schema = response_schema.map(|schema| match schema {
        TranslationSchema::LocallyValidated(schema) => schema,
    });
    let transport = TranslateTransportOptions {
        locally_validated_schema,
        max_output_tokens,
        streaming_enabled,
        ui_language,
        cancel_token: &cancel_token,
        request_timeout,
        search_enabled,
    };

    println!(
        "[translate] provider={} model={} instruction_bytes={} text_bytes={} prompt_bytes={}",
        provider,
        model,
        instruction.len(),
        text.len(),
        prompt.len()
    );

    if response_schema.is_some() && !supports_structured_translation(&provider) {
        return Err(anyhow::anyhow!("STRUCTURED_OUTPUT_UNSUPPORTED:{provider}"));
    }
    if !search_enabled
        && search_tool_enabled_by_default.unwrap_or_else(|| {
            crate::model_config::get_all_models().iter().any(|entry| {
                entry.provider == provider
                    && entry.full_name == model
                    && entry.search_tool_enabled_by_default
            })
        })
    {
        return Err(anyhow::anyhow!(
            "SEARCH_REQUIRES_EXPLICIT_OPT_IN:{provider}:{model}"
        ));
    }
    if search_enabled && !search_request_supported(&provider, &model, search_capability) {
        return Err(anyhow::anyhow!("SEARCH_UNSUPPORTED:{provider}:{model}"));
    }
    if search_enabled && provider == "groq" && (response_schema.is_some() || use_json_format) {
        return Err(anyhow::anyhow!("SEARCH_STRUCTURED_OUTPUT_UNSUPPORTED:groq"));
    }
    match Provider::from_wire(&provider) {
        Some(Provider::Ollama) => {
            // --- OLLAMA LOCAL API ---
            let ollama_base_url =
                saved_ollama_base_url.unwrap_or_else(|| "http://localhost:11434".to_string());

            return crate::api::ollama::ollama_generate_text_in_chain(
                crate::api::ollama::OllamaChainTextRequest {
                    base_url: &ollama_base_url,
                    model: &model,
                    prompt: &prompt,
                    streaming_enabled,
                    ui_language,
                    cancel_token: &cancel_token,
                    request_timeout,
                },
                on_chunk,
            );
        }
        Some(Provider::GeminiLive) => {
            // --- GEMINI LIVE API (WebSocket-based low-latency streaming) ---
            return crate::api::gemini_live::gemini_live_generate(
                crate::api::gemini_live::GeminiLiveGenerateRequest {
                    api_key: gemini_api_key.to_string(),
                    model,
                    text,
                    instruction,
                    image_data: None,
                    audio_data: None,
                    streaming_enabled,
                    ui_language,
                    cancel_token,
                    request_timeout: request_timeout.map(|timeouts| timeouts.total),
                },
                on_chunk,
            );
        }
        Some(Provider::GoogleGtx) => {
            // --- GOOGLE TRANSLATE (GTX) API ---
            let target_lang = target_language
                .filter(|lang| !lang.trim().is_empty())
                .unwrap_or_else(|| {
                    instruction
                        .to_lowercase()
                        .split("translate to ")
                        .nth(1)
                        .and_then(|s| s.split('.').next())
                        .and_then(|s| s.split(',').next())
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|| "English".to_string())
                });

            let target_lang = target_lang
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    if i == 0 {
                        c.to_uppercase().next().unwrap_or(c)
                    } else {
                        c
                    }
                })
                .collect::<String>();

            match crate::api::realtime_audio::translate_with_google_gtx(&text, &target_lang) {
                Some(translated) => {
                    on_chunk(&translated);
                    return Ok(translated);
                }
                None => {
                    return Err(anyhow::anyhow!("GTX translation failed"));
                }
            }
        }
        Some(Provider::Taalas) => {
            // --- TAALAS API (chatjimmy.ai / HC1 silicon) ---
            full_content =
                translate_taalas(&prompt, &cancel_token, request_timeout, &mut on_chunk)?;
        }
        Some(Provider::Google) => {
            // --- GEMINI TEXT API ---
            full_content = translate_gemini(
                gemini_api_key,
                &model,
                &prompt,
                response_schema,
                transport,
                &mut on_chunk,
            )?;
        }
        Some(Provider::Nvidia) => {
            // --- NVIDIA NIM API ---
            full_content = translate_nvidia(
                &nvidia_api_key,
                &model,
                &prompt,
                response_schema,
                transport,
                &mut on_chunk,
            )?;
        }
        Some(Provider::OpenRouter) => {
            // --- OPENROUTER API ---
            full_content = translate_openrouter(
                &openrouter_api_key,
                &model,
                &prompt,
                response_schema,
                transport,
                &mut on_chunk,
            )?;
        }
        _ => {
            // --- GROQ API (Default) ---
            if groq_api_key.trim().is_empty() {
                return Err(anyhow::anyhow!("NO_API_KEY:groq"));
            }

            let is_compound = model.starts_with("groq/compound");

            if is_compound {
                return groq::translate_groq_compound(
                    groq_api_key,
                    &model,
                    &prompt,
                    search_label,
                    transport,
                    on_chunk,
                );
            } else {
                return groq::translate_groq_standard(
                    groq_api_key,
                    &model,
                    &prompt,
                    use_json_format,
                    response_schema,
                    transport,
                    on_chunk,
                );
            }
        }
    }

    // Deliberately unjudged. This function runs every text block of every
    // preset -- summarize, rewrite, answer, and whatever a user writes -- so the
    // instruction is arbitrary and the shape of a correct reply is unknowable
    // from here. A check that assumed translation once rejected a correct reply
    // that named a Japanese title for an English article, and there is no
    // shortage of further intents nobody has thought of yet.
    //
    // Correctness is judged where the request is controlled instead: the monitor
    // sends prompts it wrote and can hold replies to them, provider-wide and
    // offline, where a mistake costs a model an eligibility streak rather than
    // costing a user their answer.
    Ok(full_content)
}

fn search_request_supported(provider: &str, model: &str, capability: Option<bool>) -> bool {
    matches!(provider, "google" | "groq")
        && capability.unwrap_or_else(|| {
            crate::model_config::model_supports_search_by_provider_and_name(provider, model)
        })
}

#[cfg(test)]
mod search_tests {
    #[test]
    fn custom_model_capability_is_honored_only_by_search_capable_adapters() {
        assert!(super::search_request_supported(
            "google",
            "custom",
            Some(true)
        ));
        assert!(super::search_request_supported(
            "groq",
            "custom",
            Some(true)
        ));
        assert!(!super::search_request_supported(
            "google",
            "custom",
            Some(false)
        ));
        assert!(!super::search_request_supported(
            "openrouter",
            "custom",
            Some(true)
        ));
        assert!(!super::search_request_supported(
            "ollama",
            "custom",
            Some(true)
        ));
    }

    #[test]
    fn initial_default_search_requires_explicit_opt_in_before_dispatch() {
        let request = super::TranslateTextRequest {
            groq_api_key: "",
            gemini_api_key: "",
            text: "private".to_string(),
            instruction: "".to_string(),
            model: "future-model".to_string(),
            provider: "google".to_string(),
            streaming_enabled: false,
            use_json_format: false,
            response_schema: None,
            max_output_tokens: None,
            search_enabled: false,
            search_capability: Some(true),
            search_tool_enabled_by_default: Some(true),
            search_label: None,
            ui_language: "en",
            cancel_token: None,
            request_timeout: None,
            target_language: None,
        };
        let error = super::translate_text_streaming(request, |_| panic!("unexpected dispatch"))
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("SEARCH_REQUIRES_EXPLICIT_OPT_IN:"));
    }
}
