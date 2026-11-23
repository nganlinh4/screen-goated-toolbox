//! Audio transcription APIs - Gemini, Whisper/Groq, and shared processing logic.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};

use super::utils::extract_pcm_from_wav;
use live_read::{LiveRead, classify_live_read};

mod audio_block;
mod groq;
mod live_prompt;
mod live_read;
pub(crate) use audio_block::execute_audio_block_cancellable;

/// Transcribe audio through the Gemini `generateContent` REST path.
///
/// General models go through the shared Gemini helper, so the upload gets the
/// same first-token/idle deadlines, cancellation, transient-HTTP retry and
/// completion validation (finish reason, error events, every part) as text and
/// vision requests. Dedicated transcription keeps its unary endpoint and
/// explicit `audioTranscriptionConfig` while using the same completion parser.
pub fn transcribe_audio_gemini(
    gemini_api_key: &str,
    prompt: String,
    model: String,
    wav_data: &[u8],
    cancel_token: Option<Arc<AtomicBool>>,
) -> Result<String> {
    if gemini_api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:google"));
    }

    let b64_audio = general_purpose::STANDARD.encode(wav_data);
    let audio_part = serde_json::json!({
        "inline_data": {"mime_type": "audio/wav", "data": b64_audio}
    });
    if model == crate::model_config::GEMINI_TRANSCRIBE_BATCH_API_MODEL {
        return transcribe_with_dedicated_model(gemini_api_key, &model, audio_part, cancel_token);
    }

    let ui_language = crate::APP.lock().unwrap().config.ui_language.clone();
    let text = crate::api::gemini_generate::stream_gemini_generate(
        crate::api::gemini_generate::GeminiGenerateRequest {
            parts: serde_json::json!([{"text": prompt}, audio_part]),
            model: &model,
            api_key: gemini_api_key,
            streaming: true,
            ui_language: &ui_language,
            cancel_token: &cancel_token,
            error_label: Some("Gemini Audio"),
            map_auth_errors: true,
            request_timeout: None,
            response_schema: None,
            audio_transcription_config: None,
            media_resolution: None,
            enable_google_search: false,
            retry_observer: None,
        },
        &mut |_| {},
    )?;
    if text.trim().is_empty() {
        return Err(anyhow::anyhow!("No content received from Gemini Audio API"));
    }
    Ok(text)
}

fn transcribe_with_dedicated_model(
    gemini_api_key: &str,
    model: &str,
    audio_part: serde_json::Value,
    cancel_token: Option<Arc<AtomicBool>>,
) -> Result<String> {
    let vocabulary = crate::APP
        .lock()
        .unwrap()
        .config
        .realtime_custom_vocabulary
        .clone();
    let transcription_config = serde_json::json!({
        "languageCodes": [],
        "mode": "SMART",
        "customVocabulary": vocabulary,
    });
    let ui_language = crate::APP.lock().unwrap().config.ui_language.clone();
    crate::api::gemini_generate::stream_gemini_generate(
        crate::api::gemini_generate::GeminiGenerateRequest {
            parts: serde_json::json!([audio_part]),
            model,
            api_key: gemini_api_key,
            streaming: false,
            ui_language: &ui_language,
            cancel_token: &cancel_token,
            error_label: Some("Gemini Audio API Error"),
            map_auth_errors: true,
            request_timeout: Some(crate::api::client::RequestTimeouts::uniform(
                Duration::from_secs(10 * 60),
            )),
            response_schema: None,
            audio_transcription_config: Some(&transcription_config),
            media_resolution: None,
            enable_google_search: false,
            retry_observer: None,
        },
        &mut |_| {},
    )
}

fn transcribe_with_gemini_live(
    api_key: &str,
    model: &str,
    target_language: Option<&str>,
    prompt: Option<&str>,
    wav_data: &[u8],
    cancelled: Option<&AtomicBool>,
) -> anyhow::Result<String> {
    use crate::api::gemini_live::transport::{
        connect_websocket_cancellable, set_socket_nonblocking, set_socket_short_timeout,
    };
    use crate::api::realtime_audio::websocket::{
        parse_input_transcription, parse_output_transcription, send_audio_chunk,
        send_audio_stream_end, send_live_translate_setup_message, send_setup_message,
    };

    let is_translate = target_language.is_some();
    live_prompt::check_cancelled(cancelled)?;
    let output_transcription = is_translate || prompt.is_some();
    let require_interaction_idle = crate::model_config::live_endpoint_profile(model)
        .is_some_and(|profile| profile.require_interaction_idle);
    let mut prompted_response =
        prompt.map(|_| live_prompt::ResponseState::new(require_interaction_idle));
    let manual_activity = !is_translate
        && crate::model_config::live_endpoint_profile(model)
            .is_some_and(|profile| profile.protocol == Some("native-audio"));
    println!(
        "[GeminiLiveInput] Starting {}, WAV data size: {} bytes",
        if is_translate {
            "translation"
        } else {
            "transcription"
        },
        wav_data.len()
    );

    // Connect and setup WebSocket
    println!("[GeminiLiveInput] Connecting to WebSocket...");
    let is_cancelled = || cancelled.is_some_and(|flag| flag.load(Ordering::SeqCst));
    let mut socket = match connect_websocket_cancellable(api_key, &is_cancelled) {
        Ok(s) => {
            println!("[GeminiLiveInput] WebSocket connected successfully");
            s
        }
        Err(e) => {
            println!("[GeminiLiveInput] WebSocket connection failed: {}", e);
            return Err(e);
        }
    };
    live_prompt::check_cancelled(cancelled)?;

    let setup_result = if let Some(prompt) = prompt {
        live_prompt::send_setup(&mut socket, model, prompt, manual_activity)
    } else if let Some(target_language) = target_language {
        send_live_translate_setup_message(&mut socket, model, target_language)
    } else {
        send_setup_message(&mut socket, model, manual_activity)
    };
    if let Err(e) = setup_result {
        println!("[GeminiLiveInput] Setup message failed: {}", e);
        return Err(e);
    }

    // Set short timeout for setup phase
    set_socket_short_timeout(&mut socket)?;
    println!("[GeminiLiveInput] Waiting for setupComplete...");
    live_read::wait_for_setup(
        || socket.read(),
        |message| match &prompted_response {
            Some(state) => state.observe_setup(message),
            None => Ok(crate::api::gemini_live::websocket::is_setup_complete(
                message,
            )),
        },
        || live_prompt::check_cancelled(cancelled),
        Duration::from_secs(30),
    )?;

    // Setup complete - switch to non-blocking mode.
    set_socket_nonblocking(&mut socket)?;

    // Extract PCM samples from WAV data
    let pcm_samples = extract_pcm_from_wav(wav_data)?;

    // Send audio in chunks (16kHz, 100ms chunks = 1600 samples)
    let chunk_size = 1600;
    let mut accumulated_text = String::new();
    let mut offset = 0;
    let mut chunks_sent = 0;
    let mut transcripts_received = 0;
    let parse_text = |msg: &str| {
        if output_transcription {
            parse_output_transcription(msg)
        } else {
            parse_input_transcription(msg)
        }
    };

    if manual_activity {
        socket.write(tungstenite::Message::Text(
            crate::api::gemini_live::client_message::activity_boundary(true)
                .to_string()
                .into(),
        ))?;
        socket.flush()?;
    }
    while offset < pcm_samples.len() {
        live_prompt::check_cancelled(cancelled)?;
        let end = (offset + chunk_size).min(pcm_samples.len());
        let chunk = &pcm_samples[offset..end];

        send_audio_chunk(&mut socket, chunk)?;
        chunks_sent += 1;
        offset = end;

        // Read any available transcriptions
        loop {
            live_prompt::check_cancelled(cancelled)?;
            match classify_live_read(socket.read(), "while sending audio")? {
                LiveRead::Text(msg) => {
                    if let Some(state) = &prompted_response {
                        state.observe(&msg)?;
                    }
                    if let Some(transcript) = parse_text(&msg)
                        && !transcript.is_empty()
                    {
                        transcripts_received += 1;
                        accumulated_text.push_str(&transcript);
                    }
                }
                LiveRead::Other => {}
                // No more messages available, continue sending.
                LiveRead::Idle => break,
            }
        }

        // Small delay between chunks to not overwhelm the connection
        std::thread::sleep(Duration::from_millis(10));
    }

    println!(
        "[GeminiLiveInput] Sent {} chunks, waiting for final transcriptions...",
        chunks_sent
    );

    live_prompt::check_cancelled(cancelled)?;
    if manual_activity {
        socket.write(tungstenite::Message::Text(
            crate::api::gemini_live::client_message::activity_boundary(false)
                .to_string()
                .into(),
        ))?;
        socket.flush()?;
    } else {
        send_audio_stream_end(&mut socket)?;
    }
    if let Some(state) = &mut prompted_response {
        state.finish_input();
    }

    // Allow speech-boundary transcripts to arrive before applying the tail idle limit.
    let conclude_start = Instant::now();
    let conclude_duration = Duration::from_secs(if prompt.is_some() { 60 } else { 15 });
    let mut last_transcript_at = (!accumulated_text.is_empty()).then(Instant::now);
    let mut response_complete = false;

    while conclude_start.elapsed() < conclude_duration {
        live_prompt::check_cancelled(cancelled)?;
        if prompt.is_none()
            && last_transcript_at.is_some_and(|last| last.elapsed() >= Duration::from_secs(2))
        {
            break;
        }
        match classify_live_read(socket.read(), "before response completion")? {
            LiveRead::Text(msg) => {
                let complete = if let Some(state) = &prompted_response {
                    state.observe(&msg)?
                } else {
                    false
                };
                if let Some(transcript) = parse_text(&msg)
                    && !transcript.is_empty()
                {
                    last_transcript_at = Some(Instant::now());
                    transcripts_received += 1;
                    accumulated_text.push_str(&transcript);
                }
                if complete {
                    response_complete = true;
                    break;
                }
            }
            LiveRead::Other => {}
            LiveRead::Idle => std::thread::sleep(Duration::from_millis(50)),
        }
    }

    let _ = socket.close(None);

    if prompt.is_some() && !response_complete {
        anyhow::bail!("Live audio response ended before completion");
    }

    if prompt.is_some() && accumulated_text.trim().is_empty() {
        anyhow::bail!("Live audio response contained no output transcription");
    }

    println!(
        "[GeminiLiveInput] Done! Transcripts received: {}, Total text length: {}",
        transcripts_received,
        accumulated_text.len()
    );

    if accumulated_text.is_empty() {
        // This is actually okay - could be silence or inaudible
        Ok(String::new())
    } else {
        Ok(accumulated_text)
    }
}
