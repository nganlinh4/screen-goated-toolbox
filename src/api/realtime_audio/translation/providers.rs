use std::collections::HashSet;
use urlencoding;

use crate::api::client::{
    UREQ_AGENT, UREQ_RESPONSE_AGENT, record_groq_json_usage, record_usage_simple,
};
use crate::api::realtime_audio::state::TranslationRequest;
use crate::config::Config;

mod failure;
mod patch_response;
use failure::{ProviderFailure, ProviderResult, checked_response, settle_attempt, unusable};
use patch_response::parse_translation_response;

pub(super) struct TranslationKeys {
    pub(super) gemini: String,
    pub(super) groq: String,
    pub(super) nvidia: String,
}

#[derive(Debug, Clone)]
pub(super) struct ValidatedTranslationResponse {
    pub(super) finalized_translation: String,
    pub(super) draft_translation: String,
}

pub(super) fn translate_with_provider(
    current_model: &str,
    keys: &TranslationKeys,
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
    text_chain: &[String],
    config: &Config,
) -> Option<ValidatedTranslationResponse> {
    if current_model == crate::model_config::REALTIME_TRANSLATION_MODEL_GTX {
        return translate_with_google_gtx_request(request, target_language);
    }

    translate_with_llm_chain(
        keys,
        request,
        target_language,
        history_entries,
        text_chain,
        config,
    )
}

fn translate_with_llm_chain(
    keys: &TranslationKeys,
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
    text_chain: &[String],
    config: &Config,
) -> Option<ValidatedTranslationResponse> {
    let blocked_providers = HashSet::new();
    for model_id in text_chain {
        let Some(model) = crate::model_config::get_model_by_id(model_id) else {
            continue;
        };
        if !model.enabled {
            continue;
        }
        if !matches!(
            model.provider.as_str(),
            "google" | "gemini-live" | "groq" | "nvidia"
        ) {
            continue;
        }
        if crate::retry_model_chain::preflight_skip_reason(
            &model.id,
            &model.provider,
            config,
            &blocked_providers,
        )
        .is_some()
            || crate::retry_model_chain::claim_model_attempt(&model.id).is_some()
        {
            continue;
        }
        let history_bytes = history_entries
            .iter()
            .fold(0_u64, |total, (source, translated)| {
                total
                    .saturating_add(source.len() as u64)
                    .saturating_add(translated.len() as u64)
            });
        let request_bytes = history_bytes
            .saturating_add(request.pending_source.len() as u64)
            .saturating_add(request.finalized_source.len() as u64)
            .saturating_add(request.draft_source.len() as u64)
            .saturating_add(request.previous_draft_translation.len() as u64);
        let request_timeout = Some(crate::retry_model_chain::interactive_request_timeouts(
            &model.id,
            config,
            crate::retry_model_chain::InteractiveRequestWorkload {
                encoded_request_bytes: request_bytes,
                ..Default::default()
            },
        ));

        let result = match model.provider.as_str() {
            "google" => translate_with_google_model(
                &keys.gemini,
                &model.full_name,
                request,
                target_language,
                history_entries,
                request_timeout,
            ),
            "gemini-live" => translate_with_gemini_live(
                &keys.gemini,
                &model.full_name,
                request,
                target_language,
                history_entries,
                request_timeout,
            ),
            "groq" => translate_with_groq(
                &keys.groq,
                &model.full_name,
                request,
                target_language,
                history_entries,
                request_timeout,
            ),
            "nvidia" => translate_with_nvidia(
                &keys.nvidia,
                &model.full_name,
                request,
                target_language,
                history_entries,
                request_timeout,
            ),
            _ => continue,
        };

        if let Some(response) = settle_attempt(&model.id, result) {
            return Some(response);
        }
    }
    None
}

fn translate_with_google_model(
    api_key: &str,
    model_name: &str,
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
    request_timeout: Option<crate::api::client::RequestTimeouts>,
) -> ProviderResult {
    if api_key.trim().is_empty() {
        return Err(ProviderFailure::Unusable);
    }

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model_name
    );
    let mut payload = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [{
                "text": build_structured_prompt(request, target_language, history_entries)
            }]
        }],
        "generationConfig": {
            "responseMimeType": "application/json"
        }
    });
    if let Some(thinking_config) = crate::api::gemini_thinking_config(model_name) {
        payload["generationConfig"]["thinkingConfig"] = thinking_config;
    }

    let http_request = UREQ_RESPONSE_AGENT
        .post(&url)
        .header("x-goog-api-key", api_key);
    let http_request = crate::api::client::with_request_timeouts(http_request, request_timeout);
    let resp = checked_response("Gemini", http_request.send_json(payload))?;
    crate::api::client::record_usage_headers("google", model_name, resp.headers());

    let root: serde_json::Value = unusable(resp.into_body().read_json().ok())?;
    let parts = unusable(
        root.get("candidates")
            .and_then(|candidates| candidates.as_array()?.first()?.get("content")?.get("parts"))
            .and_then(serde_json::Value::as_array),
    )?;

    let mut json_text = String::new();
    for part in parts {
        if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
            json_text.push_str(text);
        }
    }

    unusable(parse_translation_response(&json_text, request))
}

fn translate_with_gemini_live(
    api_key: &str,
    model_name: &str,
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
    request_timeout: Option<crate::api::client::RequestTimeouts>,
) -> ProviderResult {
    let prompt = build_structured_prompt(request, target_language, history_entries);
    let text = crate::api::gemini_live::gemini_live_generate(
        crate::api::gemini_live::GeminiLiveGenerateRequest {
            api_key: api_key.to_string(),
            model: model_name.to_string(),
            text: prompt,
            instruction: String::new(),
            image_data: None,
            audio_data: None,
            streaming_enabled: false,
            ui_language: "",
            cancel_token: None,
            request_timeout: request_timeout.map(|timeouts| timeouts.total),
        },
        |_| {},
    )
    .map_err(|error| ProviderFailure::Request(error.to_string()))?;
    unusable(parse_translation_response(&text, request))
}

fn translate_with_groq(
    api_key: &str,
    model_name: &str,
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
    request_timeout: Option<crate::api::client::RequestTimeouts>,
) -> ProviderResult {
    if api_key.trim().is_empty() {
        return Err(ProviderFailure::Unusable);
    }

    let schema = live_translate_response_format()["json_schema"]["schema"].clone();
    let mut payload = serde_json::json!({
        "model": model_name,
        "messages": build_chat_messages(request, target_language, history_entries),
        "stream": false,
        "max_tokens": 512,
        "response_format": crate::api::groq::structured_response_format(
            model_name,
            "live_translate_patches",
            schema,
        ),
    });
    crate::api::apply_ordinary_openai_reasoning_policy(&mut payload, "groq", model_name);

    let http_request = UREQ_RESPONSE_AGENT
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", api_key))
        .header("Content-Type", "application/json");
    let http_request = crate::api::client::with_request_timeouts(http_request, request_timeout);
    let sent = http_request.send_json(payload);
    if let Ok(resp) = &sent {
        record_usage_simple(resp.headers(), model_name);
    }
    let resp = checked_response("Groq", sent)?;
    let root: serde_json::Value = unusable(resp.into_body().read_json().ok())?;
    record_groq_json_usage(model_name, &root);
    let content = unusable(chat_message_content(&root))?;

    unusable(parse_translation_response(content, request))
}

fn chat_message_content(root: &serde_json::Value) -> Option<&str> {
    root.get("choices")?
        .as_array()?
        .first()?
        .get("message")?
        .get("content")?
        .as_str()
}

fn translate_with_nvidia(
    api_key: &str,
    model_name: &str,
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
    request_timeout: Option<crate::api::client::RequestTimeouts>,
) -> ProviderResult {
    if api_key.trim().is_empty() {
        return Err(ProviderFailure::Unusable);
    }

    let mut payload = serde_json::json!({
        "model": model_name,
        "messages": build_chat_messages(request, target_language, history_entries),
        "stream": false,
        "max_tokens": 512,
        "temperature": 0,
        "response_format": live_translate_response_format(),
    });
    crate::api::apply_ordinary_openai_reasoning_policy(&mut payload, "nvidia", model_name);

    let http_request = UREQ_RESPONSE_AGENT
        .post(crate::api::NVIDIA_CHAT_COMPLETIONS_URL)
        .header("Authorization", &format!("Bearer {api_key}"))
        .header("Content-Type", "application/json");
    let http_request = crate::api::client::with_request_timeouts(http_request, request_timeout);
    let resp = checked_response("NVIDIA", http_request.send_json(payload))?;
    crate::api::client::record_usage_headers("nvidia", model_name, resp.headers());
    let root: serde_json::Value = unusable(resp.into_body().read_json().ok())?;
    let content = unusable(chat_message_content(&root))?;
    unusable(parse_translation_response(content, request))
}

fn translate_with_google_gtx_request(
    request: &TranslationRequest,
    target_language: &str,
) -> Option<ValidatedTranslationResponse> {
    let finalized_translation = if request.finalized_source.is_empty() {
        String::new()
    } else {
        translate_with_google_gtx(&request.finalized_source, target_language)?
    };

    let draft_translation = if request.draft_source.is_empty() {
        String::new()
    } else {
        translate_with_google_gtx(&request.draft_source, target_language)?
    };

    Some(ValidatedTranslationResponse {
        finalized_translation,
        draft_translation,
    })
}

fn build_chat_messages(
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
) -> Vec<serde_json::Value> {
    let mut messages = Vec::new();
    messages.push(serde_json::json!({
        "role": "system",
        "content": "You translate live transcript windows into JSON source patches. Respond with JSON only."
    }));
    for (source, translation) in history_entries {
        messages.push(serde_json::json!({
            "role": "user",
            "content": format!("Translate to {}:\n{}", target_language, source)
        }));
        messages.push(serde_json::json!({
            "role": "assistant",
            "content": translation
        }));
    }
    messages.push(serde_json::json!({
        "role": "user",
        "content": build_structured_prompt(request, target_language, history_entries)
    }));
    messages
}

fn build_structured_prompt(
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
) -> String {
    let history_json: Vec<serde_json::Value> = history_entries
        .iter()
        .map(|(source, translation)| {
            serde_json::json!({
                "source": source,
                "translation": translation,
            })
        })
        .collect();

    let mut expected_patches = Vec::new();
    if !request.finalized_source.is_empty() {
        expected_patches.push(serde_json::json!({
            "sourceStart": request.source_start,
            "sourceEnd": request.finalized_source_end,
            "state": "final",
        }));
    }
    if !request.draft_source.is_empty() {
        expected_patches.push(serde_json::json!({
            "sourceStart": request.draft_source_start(),
            "sourceEnd": request.source_end,
            "state": "draft",
        }));
    }

    let window_json = serde_json::json!({
        "sourceStart": request.source_start,
        "sourceEnd": request.source_end,
        "pendingSource": request.pending_source,
        "finalizedSource": request.finalized_source,
        "draftSource": request.draft_source,
        "previousDraftTranslation": request.previous_draft_translation,
    });

    format!(
        "You are a professional live translator.\n\
Translate only the provided source window into {target_language}.\n\
Return JSON with a single key named patches.\n\
Each patch must keep the exact sourceStart/sourceEnd values from expectedPatches.\n\
Use state=\"final\" for the finalized source span and state=\"draft\" for the trailing unfinished span.\n\
Do not add commentary, markdown, or extra keys.\n\n\
Recent committed context:\n{history}\n\n\
Current source window:\n{window}\n\n\
Expected patches:\n{patches}",
        target_language = target_language,
        history = serde_json::Value::Array(history_json),
        window = window_json,
        patches = serde_json::Value::Array(expected_patches),
    )
}

fn live_translate_response_format() -> serde_json::Value {
    serde_json::json!({
        "type": "json_schema",
        "json_schema": {
            "name": "live_translate_patches",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {
                    "patches": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "sourceStart": { "type": "integer" },
                                "sourceEnd": { "type": "integer" },
                                "state": { "type": "string", "enum": ["final", "draft"] },
                                "translation": { "type": "string" }
                            },
                            "required": ["sourceStart", "sourceEnd", "state", "translation"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["patches"],
                "additionalProperties": false
            }
        }
    })
}

/// Unofficial Google Translate (GTX) fallback.
/// Google's `tl` code for a picker name or a typed code. Names resolve first:
/// "Lao", "Ewe" and "Twi" are also three-letter shapes, and Google expects
/// their two-letter codes.
fn gtx_target_code(target_lang: &str) -> String {
    let trimmed_target = target_lang.trim();
    if let Some(code) =
        crate::config::language_from_name(trimmed_target).and_then(|lang| lang.to_639_1())
    {
        return code.to_string();
    }
    if (2..=3).contains(&trimmed_target.len())
        && trimmed_target.chars().all(|ch| ch.is_ascii_alphabetic())
    {
        return trimmed_target.to_ascii_lowercase();
    }
    "en".to_string()
}

pub fn translate_with_google_gtx(text: &str, target_lang: &str) -> Option<String> {
    let target_code = gtx_target_code(target_lang);

    let encoded_text = urlencoding::encode(text);
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={}&dt=t&q={}",
        target_code, encoded_text
    );

    if let Ok(resp) = UREQ_AGENT
        .get(&url)
        .header("User-Agent", "Mozilla/5.0")
        .call()
        && let Ok(json) = resp.into_body().read_json::<serde_json::Value>()
        && let Some(sentences) = json.get(0).and_then(|v| v.as_array())
    {
        let mut full_text = String::new();
        for sentence_node in sentences {
            if let Some(segment) = sentence_node.get(0).and_then(|s| s.as_str()) {
                full_text.push_str(segment);
            }
        }
        if !full_text.is_empty() {
            return Some(full_text);
        }
    }
    None
}

#[cfg(test)]
mod gtx_tests {
    use super::gtx_target_code;

    #[test]
    fn short_language_names_use_their_two_letter_google_code() {
        for (name, code) in [
            ("Lao", "lo"),
            ("Ewe", "ee"),
            ("Twi", "tw"),
            ("Korean", "ko"),
        ] {
            assert_eq!(gtx_target_code(name), code, "{name}");
        }
        assert_eq!(gtx_target_code(" ko "), "ko");
        assert_eq!(gtx_target_code("Klingonish"), "en");
    }

    #[test]
    fn every_picker_name_uses_the_two_letter_code_of_the_listed_language() {
        for name in crate::config::get_all_languages() {
            let listed = crate::config::language_from_name(name)
                .and_then(|language| language.to_639_1())
                .expect("picker languages have a two-letter code");
            assert_eq!(gtx_target_code(name), listed, "{name}");
        }
    }
}
