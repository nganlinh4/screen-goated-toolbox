use super::client::{UREQ_AGENT, UREQ_RESPONSE_AGENT, record_groq_json_usage, record_usage_simple};
use super::gemini_generate::stream_gemini_generate;
use super::openai_compat::stream_openai_compat_chat;
use super::types::ChatCompletionResponse;
use crate::api::providers::Provider;
use anyhow::Result;
use image::{ImageBuffer, Rgba};
use std::io::BufReader;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

mod image_payload;
use image_payload::{GROQ_SAFE_REQUEST_BYTES, prepare_image_payload};

const QWEN_VISION_MAX_COMPLETION_TOKENS: u64 = 2_048;
const GROQ_MAX_RATE_LIMIT_WAIT_SECS: u64 = 30;

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
    /// When `Some` and the model is Gemma 4, sent as `responseJsonSchema` so Gemma
    /// emits clean structured JSON (it ignores `responseMimeType` alone). Ignored
    /// for other models / providers.
    pub response_schema: Option<serde_json::Value>,
    pub cancel_token: Option<Arc<AtomicBool>>,
}

fn groq_vision_payload(
    model: &str,
    prompt: &str,
    mime_type: &str,
    b64_image: &str,
    streaming: bool,
    response_schema: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": [
                    { "type": "text", "text": prompt },
                    { "type": "image_url", "image_url": { "url": format!("data:{mime_type};base64,{b64_image}") } }
                ]
            }
        ],
        "temperature": 0.1,
        "stream": streaming
    });
    if model.starts_with("qwen/") {
        payload["reasoning_format"] = "hidden".into();
        payload["max_completion_tokens"] = QWEN_VISION_MAX_COMPLETION_TOKENS.into();
    }
    if let Some(schema) = response_schema {
        payload["response_format"] =
            crate::api::groq::structured_response_format(model, "image_result", schema.clone());
    }
    payload
}

fn retry_after_seconds(headers: &ureq::http::HeaderMap) -> Option<u64> {
    headers
        .get("retry-after")?
        .to_str()
        .ok()?
        .parse::<f64>()
        .ok()
        .map(f64::ceil)
        .map(|seconds| seconds as u64)
}

fn wait_for_groq_retry(seconds: u64, cancel_token: &Option<Arc<AtomicBool>>) -> bool {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        if cancel_token
            .as_ref()
            .is_some_and(|token| token.load(Ordering::Relaxed))
        {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

fn groq_error_message(status: u16, body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|root| {
            root.pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| format!("HTTP {status}"))
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
        response_schema,
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
        .map(|app| app.config.cerebras_api_key.clone())
        .unwrap_or_default();

    let prepared_image =
        prepare_image_payload(provider.as_str(), image, original_bytes, prompt.len())?;
    let b64_image = prepared_image.b64_image;
    let image_data = prepared_image.image_data;
    let mime_type = prepared_image.mime_type;
    let original_bytes = prepared_image.original_bytes;

    let mut full_content = String::new();

    if Provider::from_wire(&provider) == Some(Provider::Ollama) {
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
    } else if Provider::from_wire(&provider) == Some(Provider::GeminiLive) {
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
    } else if Provider::from_wire(&provider) == Some(Provider::Qrserver) {
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
    } else if Provider::from_wire(&provider) == Some(Provider::Google) {
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
            response_schema.as_ref(),
            &mut on_chunk,
        )?;
    } else if Provider::from_wire(&provider) == Some(Provider::Cerebras) {
        let ui_language = crate::APP
            .lock()
            .ok()
            .map(|app| app.config.ui_language.clone())
            .unwrap_or_else(|| "en".to_string());
        let messages = serde_json::json!([{
            "role": "user",
            "content": [
                { "type": "text", "text": prompt },
                { "type": "image_url", "image_url": { "url": format!("data:{};base64,{}", mime_type, b64_image) } }
            ]
        }]);
        let response_format = response_schema
            .filter(|_| crate::api::cerebras::supports_structured_outputs(&model))
            .map(|schema| crate::api::cerebras::strict_json_schema("image_result", schema))
            .or_else(|| use_json_format.then(|| serde_json::json!({ "type": "json_object" })));
        full_content = crate::api::cerebras::stream_chat(
            crate::api::cerebras::StreamChatRequest {
                api_key: &cerebras_api_key,
                model: &model,
                messages,
                streaming: streaming_enabled,
                ui_language: &ui_language,
                cancel_token: &cancel_token,
                error_label: "Cerebras Vision API Error",
                response_format,
                prediction: None,
            },
            &mut on_chunk,
        )?;
    } else if Provider::from_wire(&provider) == Some(Provider::OpenRouter) {
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
                    { "type": "image_url", "image_url": { "url": format!("data:{};base64,{}", mime_type, b64_image) } }
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

        let mut payload = groq_vision_payload(
            &model,
            &prompt,
            &mime_type,
            &b64_image,
            streaming_enabled,
            response_schema.as_ref(),
        );

        let mut payload_bytes = serde_json::to_vec(&payload)
            .map_err(|e| anyhow::anyhow!("Failed to encode Groq vision request: {e}"))?;
        println!(
            "[vision] Groq request model={model} mime={mime_type} image_bytes={} request_bytes={} limit={GROQ_SAFE_REQUEST_BYTES}",
            image_data.len(),
            payload_bytes.len()
        );
        if payload_bytes.len() > GROQ_SAFE_REQUEST_BYTES {
            return Err(anyhow::anyhow!(
                "Groq vision request exceeded the local byte limit: {} > {}",
                payload_bytes.len(),
                GROQ_SAFE_REQUEST_BYTES
            ));
        }

        let mut rate_attempt = 0;
        let mut empty_recovery_attempt = 0;
        let root = loop {
            let resp = loop {
                let response = UREQ_RESPONSE_AGENT
                    .post("https://api.groq.com/openai/v1/chat/completions")
                    .header("Authorization", &format!("Bearer {}", groq_api_key))
                    .header("Content-Type", "application/json")
                    .send(payload_bytes.as_slice())
                    .map_err(|error| anyhow::anyhow!("Groq vision transport error: {error}"))?;
                let status = response.status().as_u16();
                if response.status().is_success() {
                    break response;
                }

                let retry_after = retry_after_seconds(response.headers());
                let body = response.into_body().read_to_string().unwrap_or_default();
                let message = groq_error_message(status, &body);
                if status == 429
                    && rate_attempt == 0
                    && retry_after.is_some_and(|seconds| seconds <= GROQ_MAX_RATE_LIMIT_WAIT_SECS)
                {
                    let seconds = retry_after.unwrap_or_default();
                    crate::log_info!(
                        "[vision] Groq token limit reached; retrying once in {seconds}s"
                    );
                    if !wait_for_groq_retry(seconds, &cancel_token) {
                        return Err(anyhow::anyhow!("Groq vision request cancelled"));
                    }
                    rate_attempt += 1;
                    continue;
                }
                if status == 401 || status == 403 {
                    return Err(anyhow::anyhow!("INVALID_API_KEY"));
                }
                return Err(anyhow::anyhow!("Groq vision API HTTP {status}: {message}"));
            };

            record_usage_simple(resp.headers(), &model);

            if streaming_enabled {
                let reader = BufReader::new(resp.into_body().into_reader());
                full_content = crate::api::openai_compat::consume_content_stream(
                    reader,
                    &cancel_token,
                    &mut on_chunk,
                )?;
                if model.starts_with("qwen/")
                    && full_content.trim().is_empty()
                    && empty_recovery_attempt == 0
                {
                    crate::log_info!(
                        "[vision] Qwen stream used its completion without final content; retrying without reasoning"
                    );
                    payload["reasoning_effort"] = "none".into();
                    payload_bytes = serde_json::to_vec(&payload).map_err(|e| {
                        anyhow::anyhow!("Failed to encode Groq vision recovery request: {e}")
                    })?;
                    empty_recovery_attempt += 1;
                    rate_attempt = 0;
                    continue;
                }
                break serde_json::Value::Null;
            }
            let root: serde_json::Value = resp
                .into_body()
                .read_json()
                .map_err(|e| anyhow::anyhow!("Failed to parse non-streaming response: {}", e))?;
            record_groq_json_usage(&model, &root);
            let content = root
                .pointer("/choices/0/message/content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if model.starts_with("qwen/")
                && content.trim().is_empty()
                && empty_recovery_attempt == 0
            {
                crate::log_info!(
                    "[vision] Qwen used its completion without final content; retrying without reasoning"
                );
                payload["reasoning_effort"] = "none".into();
                payload_bytes = serde_json::to_vec(&payload).map_err(|e| {
                    anyhow::anyhow!("Failed to encode Groq vision recovery request: {e}")
                })?;
                empty_recovery_attempt += 1;
                rate_attempt = 0;
                continue;
            }
            break root;
        };

        if !streaming_enabled {
            let chat_resp: ChatCompletionResponse = serde_json::from_value(root)
                .map_err(|e| anyhow::anyhow!("Failed to decode non-streaming response: {}", e))?;

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

#[cfg(test)]
mod live_tests;
