use super::client::{UREQ_AGENT, is_auth_error, record_usage_simple};
use super::gemini_generate::stream_gemini_generate;
use super::openai_compat::stream_openai_compat_chat;
use super::types::{ChatCompletionResponse, StreamChunk};
use anyhow::Result;
use image::{ImageBuffer, Rgba};
use std::io::{BufRead, BufReader};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

mod image_payload;
use image_payload::prepare_image_payload;

pub struct TranslateImageRequest<'a> {
    pub groq_api_key: &'a str,
    pub gemini_api_key: &'a str,
    pub prompt: String,
    pub model: String,
    pub provider: String,
    pub image: ImageBuffer<Rgba<u8>, Vec<u8>>,
    pub original_bytes: Option<Vec<u8>>,
    pub streaming_enabled: bool,
    pub use_json_format: bool,
    pub cancel_token: Option<Arc<AtomicBool>>,
}

pub fn translate_image_streaming<F>(
    request: TranslateImageRequest<'_>,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let TranslateImageRequest {
        groq_api_key,
        gemini_api_key,
        prompt,
        model,
        provider,
        image,
        original_bytes,
        streaming_enabled,
        use_json_format,
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

    let prepared_image = prepare_image_payload(provider.as_str(), image, original_bytes)?;
    let b64_image = prepared_image.b64_image;
    let image_data = prepared_image.image_data;
    let mime_type = prepared_image.mime_type;
    let original_bytes = prepared_image.original_bytes;

    let mut full_content = String::new();

    if provider == "ollama" {
        // Ollama Local API
        let (ollama_base_url, ui_language) = crate::APP
            .lock()
            .ok()
            .map(|app| {
                let config = app.config.clone();
                (config.ollama_base_url.clone(), config.ui_language.clone())
            })
            .unwrap_or_else(|| ("http://localhost:11434".to_string(), "en".to_string()));

        // Reload image from PNG data
        let ollama_image = image::load_from_memory(&image_data)?.to_rgba8();

        return super::ollama::ollama_generate_vision(
            &ollama_base_url,
            &model,
            &prompt,
            ollama_image,
            streaming_enabled,
            &ui_language,
            on_chunk,
        );
    } else if provider == "gemini-live" {
        let ui_language = crate::APP
            .lock()
            .ok()
            .map(|app| app.config.ui_language.clone())
            .unwrap_or_else(|| "en".to_string());
        let live_image_bytes = original_bytes.unwrap_or(image_data);

        return crate::api::gemini_live::gemini_live_generate(
            crate::api::gemini_live::GeminiLiveGenerateRequest {
                model,
                text: prompt,
                instruction: String::new(),
                image_data: Some((live_image_bytes, mime_type)),
                audio_data: None,
                streaming_enabled,
                ui_language: &ui_language,
            },
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
            .header(
                "Content-Type",
                &format!("multipart/form-data; boundary={}", boundary),
            )
            .send(&body)
            .map_err(|e| anyhow::anyhow!("QR Server API Error: {}", e))?;

        let json: serde_json::Value = resp
            .into_body()
            .read_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse QR response: {}", e))?;

        // Response format: [{"type":"qrcode","symbol":[{"seq":0,"data":"content","error":null}]}]
        if let Some(first) = json.as_array().and_then(|a| a.first())
            && let Some(symbols) = first.get("symbol").and_then(|s| s.as_array())
            && let Some(first_symbol) = symbols.first()
        {
            if let Some(data) = first_symbol.get("data").and_then(|d| d.as_str())
                && !data.is_empty()
            {
                full_content = data.to_string();
                on_chunk(&full_content);
                return Ok(full_content);
            }
            // Check for error
            if let Some(error) = first_symbol.get("error").and_then(|e| e.as_str())
                && !error.is_empty()
            {
                return Err(anyhow::anyhow!("QR_NOT_FOUND: {}", error));
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

        // Get UI language from config for thinking indicator
        let ui_language = crate::APP
            .lock()
            .ok()
            .map(|app| app.config.ui_language.clone())
            .unwrap_or_else(|| "en".to_string());

        let parts = serde_json::json!([
            { "text": prompt },
            {
                "inline_data": {
                    "mime_type": mime_type,
                    "data": b64_image
                }
            }
        ]);

        full_content = stream_gemini_generate(
            parts,
            &model,
            gemini_api_key,
            streaming_enabled,
            &ui_language,
            &cancel_token,
            None,
            true,
            &mut on_chunk,
        )?;
    } else if provider == "openrouter" {
        // --- OPENROUTER API ---
        if openrouter_api_key.trim().is_empty() {
            return Err(anyhow::anyhow!("NO_API_KEY:openrouter"));
        }

        // Get UI language from config for thinking indicator
        let ui_language = crate::APP
            .lock()
            .ok()
            .map(|app| app.config.ui_language.clone())
            .unwrap_or_else(|| "en".to_string());

        let messages = serde_json::json!([
            {
                "role": "user",
                "content": [
                    { "type": "text", "text": prompt },
                    { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{}", b64_image) } }
                ]
            }
        ]);

        full_content = stream_openai_compat_chat(
            "https://openrouter.ai/api/v1/chat/completions",
            &openrouter_api_key,
            &model,
            messages,
            streaming_enabled,
            false,
            &ui_language,
            &cancel_token,
            "OpenRouter API Error",
            true,
            |_| {},
            &mut on_chunk,
        )?;
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

        let resp = UREQ_AGENT
            .post("https://api.groq.com/openai/v1/chat/completions")
            .header("Authorization", &format!("Bearer {}", groq_api_key))
            .send_json(payload)
            .map_err(|e| {
                if is_auth_error(&e) {
                    anyhow::anyhow!("INVALID_API_KEY")
                } else if matches!(&e, ureq::Error::StatusCode(400)) {
                    anyhow::anyhow!(
                        "Groq API 400: Bad request. Check model availability or API request format."
                    )
                } else {
                    anyhow::anyhow!(
                        "Error: https://api.groq.com/openai/v1/chat/completions: {}",
                        e
                    )
                }
            })?;

        record_usage_simple(resp.headers(), &model);

        if streaming_enabled {
            let reader = BufReader::new(resp.into_body().into_reader());
            for line in reader.lines() {
                if let Some(ref ct) = cancel_token
                    && ct.load(Ordering::Relaxed)
                {
                    return Err(anyhow::anyhow!("Cancelled"));
                }
                let line = line?;

                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        break;
                    }

                    match serde_json::from_str::<StreamChunk>(data) {
                        Ok(chunk) => {
                            if let Some(content) =
                                chunk.choices.first().and_then(|c| c.delta.content.as_ref())
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
                .into_body()
                .read_json()
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
