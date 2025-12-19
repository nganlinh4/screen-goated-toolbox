use anyhow::Result;
use image::{ImageBuffer, Rgba};
use base64::{Engine as _, engine::general_purpose};
use std::io::{Cursor, BufRead, BufReader};
use crate::APP;
use crate::gui::locale::LocaleText;
use super::client::UREQ_AGENT;
use super::types::{StreamChunk, ChatCompletionResponse};

pub fn translate_image_streaming<F>(
    groq_api_key: &str,
    gemini_api_key: &str,
    prompt: String,
    model: String,
    provider: String,
    image: ImageBuffer<Rgba<u8>, Vec<u8>>,
    streaming_enabled: bool,
    use_json_format: bool,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let openrouter_api_key = crate::APP.lock()
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

    let mut image_data = Vec::new();
    image.write_to(&mut Cursor::new(&mut image_data), image::ImageFormat::Png)?;
    let b64_image = general_purpose::STANDARD.encode(&image_data);

    let mut full_content = String::new();

    if provider == "google" {
        // Gemini API
        if gemini_api_key.trim().is_empty() {
            return Err(anyhow::anyhow!("NO_API_KEY:gemini"));
        }

        let method = if streaming_enabled { "streamGenerateContent" } else { "generateContent" };
        let url = if streaming_enabled {
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:{}?alt=sse",
                model, method
            )
        } else {
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:{}",
                model, method
            )
        };

        let mut payload = serde_json::json!({
            "contents": [{
                "role": "user",
                "parts": [
                    { "text": prompt },
                    {
                        "inline_data": {
                            "mime_type": "image/png",
                            "data": b64_image
                        }
                    }
                ]
            }]
        });
        
        // Enable thinking for Gemini 2.5+ models (gemini-flash-latest and gemini-robotics-er)
        let supports_thinking = model.contains("gemini-flash-latest") || model.contains("gemini-robotics");
        if supports_thinking {
            payload["generationConfig"] = serde_json::json!({
                "thinkingConfig": {
                    "includeThoughts": true
                }
            });
        }
        
        if !model.contains("gemma-3-27b-it") {
            payload["tools"] = serde_json::json!([
                { "url_context": {} },
                { "google_search": {} }
            ]);
        }

        let resp = UREQ_AGENT.post(&url)
            .set("x-goog-api-key", gemini_api_key)
            .send_json(payload)
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("401") || err_str.contains("403") {
                    anyhow::anyhow!("INVALID_API_KEY")
                } else {
                    anyhow::anyhow!("{}", err_str)
                }
            })?;

        if streaming_enabled {
            let reader = BufReader::new(resp.into_reader());
            let mut thinking_shown = false;
            let mut content_started = false;
            
            // Get UI language from config for thinking indicator
            let ui_language = crate::APP.lock()
                .ok()
                .map(|app| app.config.ui_language.clone())
                .unwrap_or_else(|| "en".to_string());
            let locale = LocaleText::get(&ui_language);

            for line in reader.lines() {
                let line = line.map_err(|e| anyhow::anyhow!("Failed to read line: {}", e))?;
                if line.starts_with("data: ") {
                    let json_str = &line["data: ".len()..];
                    if json_str.trim() == "[DONE]" { break; }

                    if let Ok(chunk_resp) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(candidates) = chunk_resp.get("candidates").and_then(|c| c.as_array()) {
                            if let Some(first_candidate) = candidates.first() {
                                if let Some(parts) = first_candidate.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array()) {
                                    for part in parts {
                                        let is_thought = part.get("thought").and_then(|t| t.as_bool()).unwrap_or(false);
                                        
                                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                            if is_thought {
                                                // Model is thinking - show thinking indicator (only once)
                                                if !thinking_shown && !content_started {
                                                    on_chunk(locale.model_thinking);
                                                    thinking_shown = true;
                                                }
                                            } else {
                                                // Regular content
                                                if !content_started && thinking_shown {
                                                    content_started = true;
                                                    full_content.push_str(text);
                                                    let wipe_content = format!("{}{}", crate::api::WIPE_SIGNAL, full_content);
                                                    on_chunk(&wipe_content);
                                                } else {
                                                    content_started = true;
                                                    full_content.push_str(text);
                                                    on_chunk(text);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            let chat_resp: serde_json::Value = resp.into_json()
                .map_err(|e| anyhow::anyhow!("Failed to parse non-streaming response: {}", e))?;

            if let Some(candidates) = chat_resp.get("candidates").and_then(|c| c.as_array()) {
                if let Some(first_choice) = candidates.first() {
                    if let Some(parts) = first_choice.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array()) {
                        // Filter out thought parts and collect only content
                        full_content = parts.iter()
                            .filter(|p| !p.get("thought").and_then(|t| t.as_bool()).unwrap_or(false))
                            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                            .collect::<String>();
                        
                        on_chunk(&full_content);
                    }
                }
            }
        }

    } else if provider == "openrouter" {
        // --- OPENROUTER API ---
        if openrouter_api_key.trim().is_empty() {
            return Err(anyhow::anyhow!("NO_API_KEY:openrouter"));
        }

        let payload = serde_json::json!({
            "model": model,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        { "type": "text", "text": prompt },
                        { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{}", b64_image) } }
                    ]
                }
            ],
            "stream": streaming_enabled
        });

        let resp = UREQ_AGENT.post("https://openrouter.ai/api/v1/chat/completions")
            .set("Authorization", &format!("Bearer {}", openrouter_api_key))
            .set("Content-Type", "application/json")
            .send_json(payload)
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("401") || err_str.contains("403") {
                    anyhow::anyhow!("INVALID_API_KEY")
                } else {
                    anyhow::anyhow!("OpenRouter API Error: {}", err_str)
                }
            })?;

        if streaming_enabled {
            let reader = BufReader::new(resp.into_reader());
            let mut thinking_shown = false;
            let mut content_started = false;
            
            // Get UI language from config for thinking indicator
            let ui_language = crate::APP.lock()
                .ok()
                .map(|app| app.config.ui_language.clone())
                .unwrap_or_else(|| "en".to_string());
            let locale = LocaleText::get(&ui_language);
            
            for line in reader.lines() {
                let line = line?;
                if line.starts_with("data: ") {
                    let data = &line[6..];
                    if data == "[DONE]" { break; }
                    
                    match serde_json::from_str::<StreamChunk>(data) {
                        Ok(chunk) => {
                            // Check for reasoning tokens (thinking phase)
                            if let Some(reasoning) = chunk.choices.get(0)
                                .and_then(|c| c.delta.reasoning.as_ref())
                                .filter(|s| !s.is_empty()) {
                                if !thinking_shown && !content_started {
                                    on_chunk(locale.model_thinking);
                                    thinking_shown = true;
                                }
                                let _ = reasoning;
                            }
                            
                            // Check for content tokens (final result)
                            if let Some(content) = chunk.choices.get(0)
                                .and_then(|c| c.delta.content.as_ref())
                                .filter(|s| !s.is_empty()) {
                                if !content_started && thinking_shown {
                                    content_started = true;
                                    full_content.push_str(content);
                                    let wipe_content = format!("{}{}", crate::api::WIPE_SIGNAL, full_content);
                                    on_chunk(&wipe_content);
                                } else {
                                    content_started = true;
                                    full_content.push_str(content);
                                    on_chunk(content);
                                }
                            }
                        }
                        Err(_) => continue,
                    }
                }
            }
        } else {
            let chat_resp: ChatCompletionResponse = resp.into_json()
                .map_err(|e| anyhow::anyhow!("Failed to parse non-streaming response: {}", e))?;

            if let Some(choice) = chat_resp.choices.first() {
                full_content = choice.message.content.clone();
                on_chunk(&full_content);
            }
        }

    } else {
        // Groq API (default)
        if groq_api_key.trim().is_empty() {
            return Err(anyhow::anyhow!("NO_API_KEY:groq"));
        }

        let payload = if streaming_enabled {
            serde_json::json!({
                "model": model,
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            { "type": "text", "text": prompt },
                            { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{}", b64_image) } }
                        ]
                    }
                ],
                "temperature": 0.1,
                "max_completion_tokens": 8192,
                "stream": true
            })
        } else {
            let payload_obj = serde_json::json!({
                "model": model,
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            { "type": "text", "text": prompt },
                            { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{}", b64_image) } }
                        ]
                    }
                ],
                "temperature": 0.1,
                "max_completion_tokens": 8192,
                "stream": false
            });
            
            payload_obj
        };

        let resp = UREQ_AGENT.post("https://api.groq.com/openai/v1/chat/completions")
            .set("Authorization", &format!("Bearer {}", groq_api_key))
            .send_json(payload)
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("401") {
                    anyhow::anyhow!("INVALID_API_KEY")
                } else if err_str.contains("400") {
                    anyhow::anyhow!("Groq API 400: Bad request. Check model availability or API request format.")
                } else {
                    anyhow::anyhow!("Error: https://api.groq.com/openai/v1/chat/completions: {}", err_str)
                }
            })?;

        if let Some(remaining) = resp.header("x-ratelimit-remaining-requests") {
             let limit = resp.header("x-ratelimit-limit-requests").unwrap_or("?");
             let usage_str = format!("{} / {}", remaining, limit);
             
             if let Ok(mut app) = APP.lock() {
                 app.model_usage_stats.insert(model.clone(), usage_str);
             }
        }

        if streaming_enabled {
            let reader = BufReader::new(resp.into_reader());
            for line in reader.lines() {
                let line = line?;

                if line.starts_with("data: ") {
                    let data = &line[6..];

                    if data == "[DONE]" {
                        break;
                    }

                    match serde_json::from_str::<StreamChunk>(data) {
                        Ok(chunk) => {
                            if let Some(content) = chunk.choices.get(0)
                                .and_then(|c| c.delta.content.as_ref()) {
                                full_content.push_str(content);
                                on_chunk(content);
                            }
                        }
                        Err(_) => continue,
                    }
                }
            }
        } else {
            let chat_resp: ChatCompletionResponse = resp.into_json()
                .map_err(|e| anyhow::anyhow!("Failed to parse non-streaming response: {}", e))?;

            if let Some(choice) = chat_resp.choices.first() {
                let content_str = &choice.message.content;
                
                if use_json_format {
                    if let Ok(json_obj) = serde_json::from_str::<serde_json::Value>(content_str) {
                        if let Some(translation) = json_obj.get("translation").and_then(|v| v.as_str()) {
                            full_content = translation.to_string();
                        } else {
                            full_content = content_str.clone();
                        }
                    } else {
                        full_content = content_str.clone();
                    }
                } else {
                    full_content = content_str.clone();
                }
                
                on_chunk(&full_content);
            }
        }
    }

    Ok(full_content)
}
