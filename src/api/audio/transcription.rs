//! Audio transcription APIs - Gemini, Whisper/Groq, and shared processing logic.

use std::io::BufRead;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};

use super::utils::extract_pcm_from_wav;
use crate::api::client::UREQ_AGENT;
use crate::config::Preset;
use crate::model_config::{get_model_by_id, model_is_non_llm};
use crate::APP;

/// Transcribe audio using Gemini REST API with streaming
pub fn transcribe_audio_gemini<F>(
    gemini_api_key: &str,
    prompt: String,
    model: String,
    wav_data: Vec<u8>,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    if gemini_api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:google"));
    }

    let b64_audio = general_purpose::STANDARD.encode(&wav_data);
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse",
        model
    );

    let mut payload = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [
                { "text": prompt },
                {
                    "inline_data": {
                        "mime_type": "audio/wav",
                        "data": b64_audio
                    }
                }
            ]
        }]
    });

    // Add grounding tools for all models except gemma-3-27b-it
    if !model.contains("gemma-3-27b-it") {
        payload["tools"] = serde_json::json!([
            { "url_context": {} },
            { "google_search": {} }
        ]);
    }

    let resp = UREQ_AGENT
        .post(&url)
        .header("x-goog-api-key", gemini_api_key)
        .send_json(payload)
        .map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("401") || err_str.contains("403") {
                anyhow::anyhow!("INVALID_API_KEY")
            } else {
                anyhow::anyhow!("Gemini Audio API Error: {}", err_str)
            }
        })?;

    let mut full_content = String::new();
    let reader = std::io::BufReader::new(resp.into_body().into_reader());

    for line in reader.lines() {
        let line = line.map_err(|e| anyhow::anyhow!("Failed to read line: {}", e))?;
        if line.starts_with("data: ") {
            let json_str = &line["data: ".len()..];
            if json_str.trim() == "[DONE]" {
                break;
            }

            if let Ok(chunk_resp) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(candidates) = chunk_resp.get("candidates").and_then(|c| c.as_array()) {
                    if let Some(first_candidate) = candidates.first() {
                        if let Some(parts) = first_candidate
                            .get("content")
                            .and_then(|c| c.get("parts"))
                            .and_then(|p| p.as_array())
                        {
                            if let Some(first_part) = parts.first() {
                                if let Some(text) = first_part.get("text").and_then(|t| t.as_str())
                                {
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

    if full_content.is_empty() {
        return Err(anyhow::anyhow!("No content received from Gemini Audio API"));
    }

    Ok(full_content)
}

/// Transcribe audio using Gemini Live WebSocket with INPUT transcription
/// (transcribes what was recorded, not AI response)
pub fn transcribe_with_gemini_live_input(
    api_key: &str,
    wav_data: Vec<u8>,
) -> anyhow::Result<String> {
    use crate::api::realtime_audio::websocket::{
        connect_websocket, parse_input_transcription, send_audio_chunk, send_setup_message,
        set_socket_nonblocking, set_socket_short_timeout,
    };
    use crate::overlay::recording::AUDIO_INITIALIZING;

    println!(
        "[GeminiLiveInput] Starting transcription, WAV data size: {} bytes",
        wav_data.len()
    );

    // Signal that we're initializing (WebSocket connection)
    AUDIO_INITIALIZING.store(true, Ordering::SeqCst);

    // Connect and setup WebSocket
    println!("[GeminiLiveInput] Connecting to WebSocket...");
    let mut socket = match connect_websocket(api_key) {
        Ok(s) => {
            println!("[GeminiLiveInput] WebSocket connected successfully");
            s
        }
        Err(e) => {
            println!("[GeminiLiveInput] WebSocket connection failed: {}", e);
            AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };

    println!("[GeminiLiveInput] Sending setup message...");
    if let Err(e) = send_setup_message(&mut socket) {
        println!("[GeminiLiveInput] Setup message failed: {}", e);
        AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
        return Err(e);
    }

    // Set short timeout for setup phase
    if let Err(e) = set_socket_short_timeout(&mut socket) {
        AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
        return Err(e);
    }

    // Wait for setup complete
    println!("[GeminiLiveInput] Waiting for setupComplete...");
    let setup_start = Instant::now();
    loop {
        match socket.read() {
            Ok(tungstenite::Message::Text(msg)) => {
                let msg = msg.as_str();
                println!(
                    "[GeminiLiveInput] Received text message: {}",
                    &msg[..msg.len().min(200)]
                );
                if msg.contains("setupComplete") {
                    println!("[GeminiLiveInput] Setup complete received!");
                    break;
                }
                if msg.contains("error") || msg.contains("Error") {
                    println!("[GeminiLiveInput] Server error: {}", msg);
                    AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
                    return Err(anyhow::anyhow!("Server returned error: {}", msg));
                }
            }
            Ok(tungstenite::Message::Binary(data)) => {
                println!(
                    "[GeminiLiveInput] Received binary message: {} bytes",
                    data.len()
                );
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    if text.contains("setupComplete") {
                        println!("[GeminiLiveInput] Setup complete (from binary)!");
                        break;
                    }
                }
            }
            Ok(other) => {
                println!("[GeminiLiveInput] Received other message type: {:?}", other);
            }
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                if setup_start.elapsed() > Duration::from_secs(30) {
                    println!("[GeminiLiveInput] Setup timeout after 30s");
                    AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
                    return Err(anyhow::anyhow!("Setup timeout"));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                println!("[GeminiLiveInput] Socket error during setup: {}", e);
                AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
                return Err(e.into());
            }
        }
    }

    // Setup complete - switch to non-blocking mode and clear initializing state
    if let Err(e) = set_socket_nonblocking(&mut socket) {
        AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
        return Err(e);
    }
    AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
    // Now signal warmup complete so UI shows recording state
    crate::overlay::recording::AUDIO_WARMUP_COMPLETE.store(true, Ordering::SeqCst);

    // Extract PCM samples from WAV data
    println!("[GeminiLiveInput] Extracting PCM samples from WAV...");
    let pcm_samples = extract_pcm_from_wav(&wav_data)?;
    println!(
        "[GeminiLiveInput] Extracted {} PCM samples",
        pcm_samples.len()
    );

    // Send audio in chunks (16kHz, 100ms chunks = 1600 samples)
    let chunk_size = 1600;
    let mut accumulated_text = String::new();
    let mut offset = 0;
    let mut chunks_sent = 0;
    let mut transcripts_received = 0;

    println!("[GeminiLiveInput] Sending audio chunks...");
    while offset < pcm_samples.len() {
        let end = (offset + chunk_size).min(pcm_samples.len());
        let chunk = &pcm_samples[offset..end];

        if send_audio_chunk(&mut socket, chunk).is_err() {
            println!(
                "[GeminiLiveInput] Failed to send audio chunk at offset {}",
                offset
            );
            break;
        }
        chunks_sent += 1;
        offset = end;

        // Read any available transcriptions
        loop {
            match socket.read() {
                Ok(tungstenite::Message::Text(msg)) => {
                    let msg = msg.as_str();
                    println!(
                        "[GeminiLiveInput] Message while sending: {}",
                        &msg[..msg.len().min(300)]
                    );
                    if let Some(transcript) = parse_input_transcription(msg) {
                        if !transcript.is_empty() {
                            println!("[GeminiLiveInput] Got transcript: '{}'", transcript);
                            transcripts_received += 1;
                            accumulated_text.push_str(&transcript);
                        }
                    }
                }
                Ok(tungstenite::Message::Binary(data)) => {
                    if let Ok(text) = String::from_utf8(data.to_vec()) {
                        if let Some(transcript) = parse_input_transcription(&text) {
                            if !transcript.is_empty() {
                                println!(
                                    "[GeminiLiveInput] Got transcript (binary): '{}'",
                                    transcript
                                );
                                transcripts_received += 1;
                                accumulated_text.push_str(&transcript);
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    break; // No more messages available, continue sending
                }
                Err(_) => break,
            }
        }

        // Small delay between chunks to not overwhelm the connection
        std::thread::sleep(Duration::from_millis(10));
    }

    println!(
        "[GeminiLiveInput] Sent {} chunks, waiting 2s for final transcriptions...",
        chunks_sent
    );

    // Wait 2 seconds after sending all audio for final transcriptions
    let conclude_start = Instant::now();
    let conclude_duration = Duration::from_secs(2);

    while conclude_start.elapsed() < conclude_duration {
        match socket.read() {
            Ok(tungstenite::Message::Text(msg)) => {
                let msg = msg.as_str();
                println!(
                    "[GeminiLiveInput] Message in conclude phase: {}",
                    &msg[..msg.len().min(300)]
                );
                if let Some(transcript) = parse_input_transcription(msg) {
                    if !transcript.is_empty() {
                        println!("[GeminiLiveInput] Got final transcript: '{}'", transcript);
                        transcripts_received += 1;
                        accumulated_text.push_str(&transcript);
                    }
                }
            }
            Ok(tungstenite::Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    if let Some(transcript) = parse_input_transcription(&text) {
                        if !transcript.is_empty() {
                            println!(
                                "[GeminiLiveInput] Got final transcript (binary): '{}'",
                                transcript
                            );
                            transcripts_received += 1;
                            accumulated_text.push_str(&transcript);
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }

    let _ = socket.close(None);

    println!(
        "[GeminiLiveInput] Done! Transcripts received: {}, Total text length: {}",
        transcripts_received,
        accumulated_text.len()
    );
    println!("[GeminiLiveInput] Final result: '{}'", accumulated_text);

    if accumulated_text.is_empty() {
        // This is actually okay - could be silence or inaudible
        Ok(String::new())
    } else {
        Ok(accumulated_text)
    }
}

/// Upload audio to Whisper API (Groq)
pub fn upload_audio_to_whisper(
    api_key: &str,
    model: &str,
    audio_data: Vec<u8>,
) -> anyhow::Result<String> {
    // Create multipart form data
    let boundary = format!(
        "----SGTBoundary{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );

    let mut body = Vec::new();

    // Add model field
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"model\"\r\n\r\n");
    body.extend_from_slice(model.as_bytes());
    body.extend_from_slice(b"\r\n");

    // Add file field
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
    body.extend_from_slice(&audio_data);
    body.extend_from_slice(b"\r\n");

    // End boundary
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

    // Make API request
    let response = UREQ_AGENT
        .post("https://api.groq.com/openai/v1/audio/transcriptions")
        .header("Authorization", &format!("Bearer {}", api_key))
        .header(
            "Content-Type",
            &format!("multipart/form-data; boundary={}", boundary),
        )
        .send(&body);

    let response = match response {
        Ok(resp) => resp,
        Err(e) => {
            let err_str = e.to_string();
            return Err(anyhow::anyhow!("API request failed: {}", err_str));
        }
    };

    // Capture rate limits
    if let Some(remaining) = response
        .headers()
        .get("x-ratelimit-remaining-requests")
        .and_then(|v| v.to_str().ok())
    {
        let limit = response
            .headers()
            .get("x-ratelimit-limit-requests")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("?");
        let usage_str = format!("{} / {}", remaining, limit);
        if let Ok(mut app) = APP.lock() {
            app.model_usage_stats.insert(model.to_string(), usage_str);
        }
    }

    // Parse response
    let json: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

    let text = json
        .get("text")
        .and_then(|t| t.as_str())
        .ok_or_else(|| anyhow::anyhow!("No text in response"))?;

    Ok(text.to_string())
}

/// Shared logic to process audio data based on a preset's configuration.
/// Returns the transcription/processing result text.
pub fn execute_audio_processing_logic(preset: &Preset, wav_data: Vec<u8>) -> anyhow::Result<String> {
    // Find the first block that is specifically an "audio" processing block
    // OR allow input_adapter if no audio block exists (for raw audio overlay)
    let (audio_block, is_raw_input_adapter) =
        match preset.blocks.iter().find(|b| b.block_type == "audio") {
            Some(b) => (b.clone(), false),
            None => match preset
                .blocks
                .iter()
                .find(|b| b.block_type == "input_adapter")
            {
                Some(b) => (b.clone(), true),
                None => {
                    let debug_types: Vec<_> = preset.blocks.iter().map(|b| &b.block_type).collect();
                    eprintln!(
                        "DEBUG [Audio]: No 'audio' blocks found in preset. Block types present: {:?}",
                        debug_types
                    );
                    return Err(anyhow::anyhow!(
                        "Audio preset has no 'audio' processing blocks configured"
                    ));
                }
            },
        };

    if is_raw_input_adapter {
        return Ok(String::new());
    }

    let model_config = get_model_by_id(&audio_block.model);
    let model_config = match model_config {
        Some(c) => c,
        None => {
            return Err(anyhow::anyhow!(
                "Model config not found for audio model: {}",
                audio_block.model
            ));
        }
    };
    let model_name = model_config.full_name.clone();
    let provider = model_config.provider.clone();

    let (groq_api_key, gemini_api_key) = {
        let app = crate::APP.lock().unwrap();
        (
            app.config.api_key.clone(),
            app.config.gemini_api_key.clone(),
        )
    };

    // Use block's prompt and language settings
    let mut final_prompt = if model_is_non_llm(&audio_block.model) {
        String::new()
    } else {
        audio_block.prompt.clone()
    };

    for (key, value) in &audio_block.language_vars {
        let pattern = format!("{{{}}}", key);
        final_prompt = final_prompt.replace(&pattern, value);
    }

    if final_prompt.contains("{language1}") && !audio_block.language_vars.contains_key("language1")
    {
        final_prompt = final_prompt.replace("{language1}", &audio_block.selected_language);
    }

    final_prompt = final_prompt.replace("{language}", &audio_block.selected_language);

    if provider == "groq" {
        if groq_api_key.trim().is_empty() {
            Err(anyhow::anyhow!("NO_API_KEY:groq"))
        } else {
            upload_audio_to_whisper(&groq_api_key, &model_name, wav_data)
        }
    } else if provider == "google" {
        if gemini_api_key.trim().is_empty() {
            Err(anyhow::anyhow!("NO_API_KEY:google"))
        } else {
            transcribe_audio_gemini(&gemini_api_key, final_prompt, model_name, wav_data, |_| {})
        }
    } else if provider == "gemini-live" {
        // Gemini Live API (WebSocket-based) - uses INPUT transcription (what user said)
        if gemini_api_key.trim().is_empty() {
            Err(anyhow::anyhow!("NO_API_KEY:gemini"))
        } else {
            transcribe_with_gemini_live_input(&gemini_api_key, wav_data)
        }
    } else {
        Err(anyhow::anyhow!("Unsupported audio provider: {}", provider))
    }
}
