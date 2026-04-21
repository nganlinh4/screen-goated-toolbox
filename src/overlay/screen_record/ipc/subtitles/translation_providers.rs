use crate::api::client::UREQ_AGENT;
use crate::api::gemini_live::gemini_live_generate;
use crate::api::ollama::ollama_generate_text;
use crate::config::Config;
use crate::model_config::ModelConfig;

use super::types::{
    SubtitleTranslationItemRequest, SubtitleTranslationResultItem,
};

#[derive(Clone, Debug)]
pub struct TranslationConversationTurn {
    pub user_payload: String,
    pub assistant_payload: String,
}

#[derive(Clone, Debug)]
pub struct ValidatedSubtitleTranslationResponse {
    pub items: Vec<SubtitleTranslationResultItem>,
    pub user_payload: String,
    pub assistant_payload: String,
}

#[derive(serde::Deserialize)]
struct TranslationResponsePayload {
    items: Vec<TranslationResponseItem>,
}

#[derive(serde::Deserialize)]
struct TranslationResponseItem {
    id: String,
    #[serde(rename = "translatedText")]
    translated_text: String,
}

pub fn translate_subtitle_chunk(
    config: &Config,
    model: &ModelConfig,
    target_language: &str,
    items: &[SubtitleTranslationItemRequest],
    history: &[TranslationConversationTurn],
) -> Result<ValidatedSubtitleTranslationResponse, String> {
    let user_payload = build_user_payload(target_language, items);
    let assistant_payload = match model.provider.as_str() {
        "google" => translate_with_google(config, model, target_language, &user_payload, history)?,
        "gemini-live" => {
            translate_with_gemini_live(config, model, target_language, &user_payload, history)?
        }
        "cerebras" => {
            translate_with_cerebras(config, model, target_language, &user_payload, history)?
        }
        "openrouter" => {
            translate_with_openrouter(config, model, target_language, &user_payload, history)?
        }
        "ollama" => translate_with_ollama(config, model, target_language, &user_payload, history)?,
        _ => translate_with_groq(config, model, target_language, &user_payload, history)?,
    };
    let validated = validate_translation_payload(&assistant_payload, items)?;
    Ok(ValidatedSubtitleTranslationResponse {
        items: validated,
        user_payload,
        assistant_payload,
    })
}

fn build_system_instruction(target_language: &str) -> String {
    format!(
        "You are translating subtitle segments inside one continuous conversation.\n\
Keep wording, terminology, named entities, tone, and references consistent with prior chunks.\n\
Translate every item into {target_language}.\n\
Return JSON only with this exact shape: {{\"items\":[{{\"id\":\"...\",\"translatedText\":\"...\"}}]}}.\n\
Rules:\n\
- Return every requested id exactly once, in the same order.\n\
- Do not omit items.\n\
- Do not add commentary or markdown.\n\
- translatedText must never be empty.\n\
- Preserve natural subtitle phrasing and punctuation."
    )
}

fn build_user_payload(target_language: &str, items: &[SubtitleTranslationItemRequest]) -> String {
    let request_items: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            serde_json::json!({
                "id": item.id,
                "text": item.text,
            })
        })
        .collect();
    format!(
        "Target language: {target_language}\n\
Translate the following subtitle items from the same ongoing conversation.\n\
Return JSON only.\n\
Current chunk:\n{}",
        serde_json::json!({ "items": request_items })
    )
}

fn build_chat_messages(
    target_language: &str,
    user_payload: &str,
    history: &[TranslationConversationTurn],
) -> Vec<serde_json::Value> {
    let mut messages = Vec::new();
    messages.push(serde_json::json!({
        "role": "system",
        "content": build_system_instruction(target_language),
    }));
    for turn in history {
        messages.push(serde_json::json!({
            "role": "user",
            "content": turn.user_payload,
        }));
        messages.push(serde_json::json!({
            "role": "assistant",
            "content": turn.assistant_payload,
        }));
    }
    messages.push(serde_json::json!({
        "role": "user",
        "content": user_payload,
    }));
    messages
}

fn strip_json_fence(payload: &str) -> &str {
    let trimmed = payload.trim();
    let without_prefix = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    without_prefix
        .strip_suffix("```")
        .unwrap_or(without_prefix)
        .trim()
}

fn validate_translation_payload(
    payload: &str,
    expected_items: &[SubtitleTranslationItemRequest],
) -> Result<Vec<SubtitleTranslationResultItem>, String> {
    let cleaned = strip_json_fence(payload);
    if cleaned.is_empty() {
        return Err("Structured translation response was empty".to_string());
    }
    let parsed: TranslationResponsePayload = serde_json::from_str(cleaned)
        .map_err(|error| format!("Failed to parse structured translation JSON: {error}"))?;
    if parsed.items.len() != expected_items.len() {
        return Err(format!(
            "Structured translation returned {} item(s) for {} requested subtitle(s)",
            parsed.items.len(),
            expected_items.len()
        ));
    }

    let mut validated = Vec::with_capacity(parsed.items.len());
    for (expected, actual) in expected_items.iter().zip(parsed.items.iter()) {
        if actual.id != expected.id {
            return Err(format!(
                "Structured translation returned id '{}' but expected '{}'",
                actual.id, expected.id
            ));
        }
        let translated_text = actual.translated_text.trim();
        if translated_text.is_empty() {
            return Err(format!(
                "Structured translation returned empty text for subtitle '{}'",
                expected.id
            ));
        }
        validated.push(SubtitleTranslationResultItem {
            id: expected.id.clone(),
            clip_id: expected.clip_id.clone(),
            translated_text: translated_text.to_string(),
        });
    }

    Ok(validated)
}

fn translate_with_google(
    config: &Config,
    model: &ModelConfig,
    target_language: &str,
    user_payload: &str,
    history: &[TranslationConversationTurn],
) -> Result<String, String> {
    if config.gemini_api_key.trim().is_empty() {
        return Err("NO_API_KEY:google".to_string());
    }

    let mut contents = Vec::new();
    for turn in history {
        contents.push(serde_json::json!({
            "role": "user",
            "parts": [{ "text": turn.user_payload }],
        }));
        contents.push(serde_json::json!({
            "role": "model",
            "parts": [{ "text": turn.assistant_payload }],
        }));
    }
    contents.push(serde_json::json!({
        "role": "user",
        "parts": [{ "text": user_payload }],
    }));

    let payload = serde_json::json!({
        "systemInstruction": {
            "parts": [{ "text": build_system_instruction(target_language) }]
        },
        "contents": contents,
        "generationConfig": {
            "responseMimeType": "application/json"
        }
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model.full_name
    );
    let response = UREQ_AGENT
        .post(&url)
        .header("x-goog-api-key", &config.gemini_api_key)
        .send_json(payload)
        .map_err(|error| format!("Google subtitle translation failed: {error}"))?;
    let root: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|error| format!("Google subtitle translation JSON failed: {error}"))?;
    let parts = root
        .get("candidates")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|value| value.get("content"))
        .and_then(|value| value.get("parts"))
        .and_then(|value| value.as_array())
        .ok_or_else(|| "Google subtitle translation returned no content".to_string())?;
    let mut json_text = String::new();
    for part in parts {
        if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
            json_text.push_str(text);
        }
    }
    Ok(json_text)
}

fn translate_with_groq(
    config: &Config,
    model: &ModelConfig,
    target_language: &str,
    user_payload: &str,
    history: &[TranslationConversationTurn],
) -> Result<String, String> {
    if config.api_key.trim().is_empty() {
        return Err("NO_API_KEY:groq".to_string());
    }
    let payload = serde_json::json!({
        "model": model.full_name,
        "messages": build_chat_messages(target_language, user_payload, history),
        "stream": false,
        "response_format": { "type": "json_object" },
    });
    let response = UREQ_AGENT
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", config.api_key))
        .send_json(payload)
        .map_err(|error| format!("Groq subtitle translation failed: {error}"))?;
    let root: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|error| format!("Groq subtitle translation JSON failed: {error}"))?;
    root.get("choices")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|value| value.get("message"))
        .and_then(|value| value.get("content"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .ok_or_else(|| "Groq subtitle translation returned no content".to_string())
}

fn cerebras_response_format() -> serde_json::Value {
    serde_json::json!({
        "type": "json_schema",
        "json_schema": {
            "name": "subtitle_translation_items",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "translatedText": { "type": "string" }
                            },
                            "required": ["id", "translatedText"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["items"],
                "additionalProperties": false
            }
        }
    })
}

fn translate_with_cerebras(
    config: &Config,
    model: &ModelConfig,
    target_language: &str,
    user_payload: &str,
    history: &[TranslationConversationTurn],
) -> Result<String, String> {
    if config.cerebras_api_key.trim().is_empty() {
        return Err("NO_API_KEY:cerebras".to_string());
    }
    let payload = serde_json::json!({
        "model": model.full_name,
        "messages": build_chat_messages(target_language, user_payload, history),
        "stream": false,
        "response_format": cerebras_response_format(),
    });
    let response = UREQ_AGENT
        .post("https://api.cerebras.ai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", config.cerebras_api_key))
        .header("Content-Type", "application/json")
        .send_json(payload)
        .map_err(|error| format!("Cerebras subtitle translation failed: {error}"))?;
    let root: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|error| format!("Cerebras subtitle translation JSON failed: {error}"))?;
    root.get("choices")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|value| value.get("message"))
        .and_then(|value| value.get("content"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .ok_or_else(|| "Cerebras subtitle translation returned no content".to_string())
}

fn translate_with_openrouter(
    config: &Config,
    model: &ModelConfig,
    target_language: &str,
    user_payload: &str,
    history: &[TranslationConversationTurn],
) -> Result<String, String> {
    if config.openrouter_api_key.trim().is_empty() {
        return Err("NO_API_KEY:openrouter".to_string());
    }
    let payload = serde_json::json!({
        "model": model.full_name,
        "messages": build_chat_messages(target_language, user_payload, history),
        "stream": false,
        "response_format": { "type": "json_object" },
    });
    let response = UREQ_AGENT
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", config.openrouter_api_key))
        .header("HTTP-Referer", "https://screen-goated-toolbox.local")
        .header("X-Title", "Screen Goated Toolbox")
        .send_json(payload)
        .map_err(|error| format!("OpenRouter subtitle translation failed: {error}"))?;
    let root: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|error| format!("OpenRouter subtitle translation JSON failed: {error}"))?;
    root.get("choices")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|value| value.get("message"))
        .and_then(|value| value.get("content"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .ok_or_else(|| "OpenRouter subtitle translation returned no content".to_string())
}

fn translate_with_gemini_live(
    config: &Config,
    model: &ModelConfig,
    target_language: &str,
    user_payload: &str,
    history: &[TranslationConversationTurn],
) -> Result<String, String> {
    if config.gemini_api_key.trim().is_empty() {
        return Err("NO_API_KEY:gemini-live".to_string());
    }
    let prompt = if history.is_empty() {
        user_payload.to_string()
    } else {
        let history_block = history
            .iter()
            .enumerate()
            .map(|(index, turn)| {
                format!(
                    "Previous chunk {} request:\n{}\nPrevious chunk {} response:\n{}",
                    index + 1,
                    turn.user_payload,
                    index + 1,
                    turn.assistant_payload
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        format!("{history_block}\n\nCurrent chunk request:\n{user_payload}")
    };
    gemini_live_generate(
        model.full_name.clone(),
        prompt,
        build_system_instruction(target_language),
        None,
        None,
        false,
        &config.ui_language,
        |_| {},
    )
    .map_err(|error| format!("Gemini Live subtitle translation failed: {error}"))
}

fn translate_with_ollama(
    config: &Config,
    model: &ModelConfig,
    target_language: &str,
    user_payload: &str,
    history: &[TranslationConversationTurn],
) -> Result<String, String> {
    let prompt = if history.is_empty() {
        format!(
            "{}\n\n{}",
            build_system_instruction(target_language),
            user_payload
        )
    } else {
        let history_block = history
            .iter()
            .enumerate()
            .map(|(index, turn)| {
                format!(
                    "Conversation turn {} user:\n{}\nConversation turn {} assistant:\n{}",
                    index + 1,
                    turn.user_payload,
                    index + 1,
                    turn.assistant_payload
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        format!(
            "{}\n\n{}\n\nCurrent chunk request:\n{}",
            build_system_instruction(target_language),
            history_block,
            user_payload
        )
    };
    ollama_generate_text(
        &config.ollama_base_url,
        &model.full_name,
        &prompt,
        false,
        &config.ui_language,
        |_| {},
    )
    .map_err(|error| format!("Ollama subtitle translation failed: {error}"))
}
