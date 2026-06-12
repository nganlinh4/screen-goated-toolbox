// --- TEXT TRANSLATION ---
// Streaming text translation with multiple LLM providers.

mod groq;
mod providers;

use anyhow::Result;
use providers::{translate_cerebras, translate_gemini, translate_openrouter, translate_taalas};
use std::sync::{Arc, atomic::AtomicBool};

pub struct TranslateTextRequest<'a> {
    pub groq_api_key: &'a str,
    pub gemini_api_key: &'a str,
    pub text: String,
    pub instruction: String,
    pub model: String,
    pub provider: String,
    pub streaming_enabled: bool,
    pub use_json_format: bool,
    pub search_label: Option<String>,
    pub ui_language: &'a str,
    pub cancel_token: Option<Arc<AtomicBool>>,
    pub target_language: Option<String>,
}

pub fn translate_text_streaming<F>(
    request: TranslateTextRequest<'_>,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let TranslateTextRequest {
        groq_api_key,
        gemini_api_key,
        text,
        instruction,
        model,
        provider,
        streaming_enabled,
        use_json_format,
        search_label,
        ui_language,
        cancel_token,
        target_language,
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

    let full_content;
    let prompt = format!("{}\n\n{}", instruction, text);

    // DEBUG: Log the instruction being sent to the API
    println!("[DEBUG translate] instruction=«{}»", instruction);
    println!(
        "[DEBUG translate] text_len={} prompt_len={}",
        text.len(),
        prompt.len()
    );

    if provider == "ollama" {
        // --- OLLAMA LOCAL API ---
        let ollama_base_url = crate::APP
            .lock()
            .ok()
            .map(|app| {
                let config = app.config.clone();
                config.ollama_base_url.clone()
            })
            .unwrap_or_else(|| "http://localhost:11434".to_string());

        return crate::api::ollama::ollama_generate_text(
            &ollama_base_url,
            &model,
            &prompt,
            streaming_enabled,
            ui_language,
            on_chunk,
        );
    } else if provider == "gemini-live" {
        // --- GEMINI LIVE API (WebSocket-based low-latency streaming) ---
        return crate::api::gemini_live::gemini_live_generate(
            crate::api::gemini_live::GeminiLiveGenerateRequest {
                model,
                text,
                instruction,
                image_data: None,
                audio_data: None,
                streaming_enabled,
                ui_language,
            },
            on_chunk,
        );
    } else if provider == "google-gtx" {
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
    } else if provider == "taalas" {
        // --- TAALAS API (chatjimmy.ai / HC1 silicon) ---
        full_content = translate_taalas(&prompt, &cancel_token, &mut on_chunk)?;
    } else if provider == "google" {
        // --- GEMINI TEXT API ---
        full_content = translate_gemini(
            gemini_api_key,
            &model,
            &prompt,
            streaming_enabled,
            ui_language,
            &cancel_token,
            &mut on_chunk,
        )?;
    } else if provider == "cerebras" {
        // --- CEREBRAS API ---
        full_content = translate_cerebras(
            &cerebras_api_key,
            &model,
            &prompt,
            streaming_enabled,
            ui_language,
            &cancel_token,
            &mut on_chunk,
        )?;
    } else if provider == "openrouter" {
        // --- OPENROUTER API ---
        full_content = translate_openrouter(
            &openrouter_api_key,
            &model,
            &prompt,
            streaming_enabled,
            ui_language,
            &cancel_token,
            &mut on_chunk,
        )?;
    } else {
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
                ui_language,
                on_chunk,
            );
        } else {
            return groq::translate_groq_standard(
                groq_api_key,
                &model,
                &prompt,
                streaming_enabled,
                use_json_format,
                cancel_token,
                on_chunk,
            );
        }
    }

    Ok(full_content)
}
