use super::client::UREQ_AGENT;
use super::types::{ChatCompletionResponse, StreamChunk};
use crate::gui::locale::LocaleText;
use crate::APP;
use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use image::{ImageBuffer, Rgba};
use std::io::{BufRead, BufReader, Cursor};

pub fn translate_image_streaming<F>(
    groq_api_key: &str,
    gemini_api_key: &str,
    prompt: String,
    model: String,
    provider: String,
    image: ImageBuffer<Rgba<u8>, Vec<u8>>,
    original_bytes: Option<Vec<u8>>, // Zero-Copy support
    streaming_enabled: bool,
    use_json_format: bool,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
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

    // Log image details
    println!(
        "DEBUG: Processing image for vision API. Original size: {}x{}",
        image.width(),
        image.height()
    );

    let mut b64_image = String::new();
    let mut image_data = Vec::new();
    let mut mime_type = "image/png".to_string();

    // Check for "Zero-Copy" path (Google provider + Original Bytes available)
    if provider == "google" && original_bytes.is_some() {
        println!("DEBUG: Zero-Copy optimization active for Google provider");
        // Use original bytes directly (e.g. JPEG) - no resize, no conversion
        let bytes = original_bytes.as_ref().unwrap();
        b64_image = general_purpose::STANDARD.encode(bytes);

        // Sniff mime type
        if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            mime_type = "image/jpeg".to_string();
        } else if bytes.starts_with(&[0x89, 0x50, 0x4e, 0x47]) {
            mime_type = "image/png".to_string();
        } else if bytes.starts_with(&[0x52, 0x49, 0x46, 0x46])
            && bytes[8..12] == [0x57, 0x45, 0x42, 0x50]
        {
            mime_type = "image/webp".to_string();
        }
        println!("DEBUG: Detected MIME type: {}", mime_type);
    } else {
        // Standard Processing Path (Resize + Convert to PNG)
        let mut final_image = image;
        let max_dim = 2048;

        // Resize if too large (Skip for Google as they handle large images well if we fall back to this path)
        if provider != "google" && (final_image.width() > max_dim || final_image.height() > max_dim)
        {
            println!("DEBUG: Image exceeds {}px, resizing...", max_dim);
            let (n_w, n_h) = if final_image.width() > final_image.height() {
                let ratio = max_dim as f32 / final_image.width() as f32;
                (max_dim, (final_image.height() as f32 * ratio) as u32)
            } else {
                let ratio = max_dim as f32 / final_image.height() as f32;
                ((final_image.width() as f32 * ratio) as u32, max_dim)
            };
            final_image = image::imageops::resize(
                &final_image,
                n_w,
                n_h,
                image::imageops::FilterType::Lanczos3,
            );
            println!(
                "DEBUG: Resized to: {}x{}",
                final_image.width(),
                final_image.height()
            );
        }

        final_image.write_to(&mut Cursor::new(&mut image_data), image::ImageFormat::Png)?;
        b64_image = general_purpose::STANDARD.encode(&image_data);
        mime_type = "image/png".to_string();
    }

    let mut full_content = String::new();

    if provider == "ollama" {
        // Ollama Local API
        let (ollama_base_url, ollama_vision_model, ui_language) = crate::APP
            .lock()
            .ok()
            .map(|app| {
                let config = app.config.clone();
                (
                    config.ollama_base_url.clone(),
                    config.ollama_vision_model.clone(),
                    config.ui_language.clone(),
                )
            })
            .unwrap_or_else(|| {
                (
                    "http://localhost:11434".to_string(),
                    model.clone(),
                    "en".to_string(),
                )
            });

        let actual_model = if ollama_vision_model.is_empty() {
            model.clone()
        } else {
            ollama_vision_model
        };

        // Reload image from PNG data
        let ollama_image = image::load_from_memory(&image_data)?.to_rgba8();

        return super::ollama::ollama_generate_vision(
            &ollama_base_url,
            &actual_model,
            &prompt,
            ollama_image,
            streaming_enabled,
            &ui_language,
            on_chunk,
        );
    } else if provider == "qrserver" {
        // --- QR SERVER API ---
        // Non-LLM QR Code scanner - no API key required
        // Uses multipart form upload to api.qrserver.com

        let boundary = format!(
            "----WebKitFormBoundary{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        let mut body = Vec::new();

        // MAX_FILE_SIZE field
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"MAX_FILE_SIZE\"\r\n\r\n");
        body.extend_from_slice(b"1048576\r\n");

        // File field
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"file\"; filename=\"qrcode.png\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
        body.extend_from_slice(&image_data);
        body.extend_from_slice(b"\r\n");

        // End boundary
        body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

        let resp = UREQ_AGENT
            .post("http://api.qrserver.com/v1/read-qr-code/")
            .set(
                "Content-Type",
                &format!("multipart/form-data; boundary={}", boundary),
            )
            .send_bytes(&body)
            .map_err(|e| anyhow::anyhow!("QR Server API Error: {}", e))?;

        let json: serde_json::Value = resp
            .into_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse QR response: {}", e))?;

        // Response format: [{"type":"qrcode","symbol":[{"seq":0,"data":"content","error":null}]}]
        if let Some(first) = json.as_array().and_then(|a| a.first()) {
            if let Some(symbols) = first.get("symbol").and_then(|s| s.as_array()) {
                if let Some(first_symbol) = symbols.first() {
                    if let Some(data) = first_symbol.get("data").and_then(|d| d.as_str()) {
                        if !data.is_empty() {
                            full_content = data.to_string();
                            on_chunk(&full_content);
                            return Ok(full_content);
                        }
                    }
                    // Check for error
                    if let Some(error) = first_symbol.get("error").and_then(|e| e.as_str()) {
                        if !error.is_empty() {
                            return Err(anyhow::anyhow!("QR_NOT_FOUND: {}", error));
                        }
                    }
                }
            }
        }

        return Err(anyhow::anyhow!(
            "QR_NOT_FOUND: No QR code detected in image"
        ));
    } else if provider == "google" {
        // Gemini API
        if gemini_api_key.trim().is_empty() {
            return Err(anyhow::anyhow!("NO_API_KEY:gemini"));
        }

        let method = if streaming_enabled {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
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
                            "mime_type": mime_type,
                            "data": b64_image
                        }
                    }
                ]
            }]
        });

        // Enable thinking for Gemini 2.5+ models (gemini-2.5-flash and gemini-robotics-er)
        // Enable thinking for Gemini 2.5+ models (gemini-2.5-flash, 3.0-flash, and gemini-robotics-er)
        let supports_thinking = (model.contains("gemini-2.5-flash") && !model.contains("lite"))
            || model.contains("gemini-3-flash-preview")
            || model.contains("gemini-robotics");
        if supports_thinking {
            payload["generationConfig"] = serde_json::json!({
                "thinkingConfig": {
                    "includeThoughts": true
                }
            });
        }

        if crate::model_config::model_supports_search_by_name(&model) {
            payload["tools"] = serde_json::json!([
                { "url_context": {} },
                { "google_search": {} }
            ]);
        }

        let resp = UREQ_AGENT
            .post(&url)
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
            let ui_language = crate::APP
                .lock()
                .ok()
                .map(|app| app.config.ui_language.clone())
                .unwrap_or_else(|| "en".to_string());
            let locale = LocaleText::get(&ui_language);

            for line in reader.lines() {
                let line = line.map_err(|e| anyhow::anyhow!("Failed to read line: {}", e))?;
                if line.starts_with("data: ") {
                    let json_str = &line["data: ".len()..];
                    if json_str.trim() == "[DONE]" {
                        break;
                    }

                    if let Ok(chunk_resp) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(candidates) =
                            chunk_resp.get("candidates").and_then(|c| c.as_array())
                        {
                            if let Some(first_candidate) = candidates.first() {
                                if let Some(parts) = first_candidate
                                    .get("content")
                                    .and_then(|c| c.get("parts"))
                                    .and_then(|p| p.as_array())
                                {
                                    for part in parts {
                                        let is_thought = part
                                            .get("thought")
                                            .and_then(|t| t.as_bool())
                                            .unwrap_or(false);

                                        if let Some(text) =
                                            part.get("text").and_then(|t| t.as_str())
                                        {
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
                                                    let wipe_content = format!(
                                                        "{}{}",
                                                        crate::api::WIPE_SIGNAL,
                                                        full_content
                                                    );
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
            let chat_resp: serde_json::Value = resp
                .into_json()
                .map_err(|e| anyhow::anyhow!("Failed to parse non-streaming response: {}", e))?;

            if let Some(candidates) = chat_resp.get("candidates").and_then(|c| c.as_array()) {
                if let Some(first_choice) = candidates.first() {
                    if let Some(parts) = first_choice
                        .get("content")
                        .and_then(|c| c.get("parts"))
                        .and_then(|p| p.as_array())
                    {
                        // Filter out thought parts and collect only content
                        full_content = parts
                            .iter()
                            .filter(|p| {
                                !p.get("thought").and_then(|t| t.as_bool()).unwrap_or(false)
                            })
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

        let resp = UREQ_AGENT
            .post("https://openrouter.ai/api/v1/chat/completions")
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
            let ui_language = crate::APP
                .lock()
                .ok()
                .map(|app| app.config.ui_language.clone())
                .unwrap_or_else(|| "en".to_string());
            let locale = LocaleText::get(&ui_language);

            for line in reader.lines() {
                let line = line?;
                if line.starts_with("data: ") {
                    let data = &line[6..];
                    if data == "[DONE]" {
                        break;
                    }

                    match serde_json::from_str::<StreamChunk>(data) {
                        Ok(chunk) => {
                            // Check for reasoning tokens (thinking phase)
                            if let Some(reasoning) = chunk
                                .choices
                                .get(0)
                                .and_then(|c| c.delta.reasoning.as_ref())
                                .filter(|s| !s.is_empty())
                            {
                                if !thinking_shown && !content_started {
                                    on_chunk(locale.model_thinking);
                                    thinking_shown = true;
                                }
                                let _ = reasoning;
                            }

                            // Check for content tokens (final result)
                            if let Some(content) = chunk
                                .choices
                                .get(0)
                                .and_then(|c| c.delta.content.as_ref())
                                .filter(|s| !s.is_empty())
                            {
                                if !content_started && thinking_shown {
                                    content_started = true;
                                    full_content.push_str(content);
                                    let wipe_content =
                                        format!("{}{}", crate::api::WIPE_SIGNAL, full_content);
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
            let chat_resp: ChatCompletionResponse = resp
                .into_json()
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
                            if let Some(content) =
                                chunk.choices.get(0).and_then(|c| c.delta.content.as_ref())
                            {
                                full_content.push_str(content);
                                on_chunk(content);
                            }
                        }
                        Err(_) => continue,
                    }
                }
            }
        } else {
            let chat_resp: ChatCompletionResponse = resp
                .into_json()
                .map_err(|e| anyhow::anyhow!("Failed to parse non-streaming response: {}", e))?;

            if let Some(choice) = chat_resp.choices.first() {
                let content_str = &choice.message.content;

                if use_json_format {
                    if let Ok(json_obj) = serde_json::from_str::<serde_json::Value>(content_str) {
                        if let Some(translation) =
                            json_obj.get("translation").and_then(|v| v.as_str())
                        {
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
