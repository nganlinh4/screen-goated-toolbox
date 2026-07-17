//! Gemini Live LLM API
//!
//! This module provides access to Gemini's native audio model as a standard LLM,
//! using the bidirectional WebSocket API for low-latency streaming text responses.
//!
//! Unlike the standard REST API, this uses a connection pool for faster response times.
//! Supports text, image, and audio inputs with text-only output.

pub mod client_message;
pub mod lifecycle;
pub mod manager;
pub mod ready_session;
pub mod server_frame;
pub mod setup;
pub mod transport;
pub mod types;
pub mod websocket;
pub mod worker;

#[cfg(test)]
mod lifecycle_tests;

use std::sync::{
    Arc, LazyLock,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

pub use manager::GeminiLiveManager;
pub use types::{LiveEvent, LiveInputContent};

/// Global Gemini Live manager instance
pub static GEMINI_LIVE_MANAGER: LazyLock<Arc<GeminiLiveManager>> =
    LazyLock::new(|| Arc::new(GeminiLiveManager::new()));

/// Number of worker threads for the connection pool
const WORKER_COUNT: usize = 2;

/// Initialize the Gemini Live LLM system - call this at app startup
pub fn init_gemini_live() {
    for _ in 0..WORKER_COUNT {
        let manager = GEMINI_LIVE_MANAGER.clone();
        std::thread::spawn(move || {
            worker::run_live_worker(manager);
        });
    }
}

pub struct GeminiLiveGenerateRequest<'a> {
    pub model: String,
    pub text: String,
    pub instruction: String,
    pub image_data: Option<(Vec<u8>, String)>,
    pub audio_data: Option<Vec<u8>>,
    pub streaming_enabled: bool,
    pub ui_language: &'a str,
    pub cancel_token: Option<Arc<AtomicBool>>,
    pub request_timeout: Option<Duration>,
}

/// Streaming text generation using Gemini Live API
/// This is the main entry point for using Gemini Live as an LLM
///
/// Arguments:
/// - `text`: The user prompt text
/// - `instruction`: System instruction / prompt template
/// - `image_data`: Optional image data (bytes, mime_type)
/// - `audio_data`: Optional audio data (PCM 16-bit mono 16kHz)
/// - `streaming_enabled`: Whether to stream chunks or wait for complete response
/// - `ui_language`: UI language for thinking message
/// - `on_chunk`: Callback for each text chunk
///
/// Returns: Complete response text or error
pub fn gemini_live_generate<F>(
    request: GeminiLiveGenerateRequest<'_>,
    mut on_chunk: F,
) -> anyhow::Result<String>
where
    F: FnMut(&str),
{
    let GeminiLiveGenerateRequest {
        model,
        text,
        instruction,
        image_data,
        audio_data,
        streaming_enabled,
        ui_language,
        cancel_token,
        request_timeout,
    } = request;
    let deadline = request_timeout.and_then(|timeout| Instant::now().checked_add(timeout));

    // Log what we're sending
    let content_type = match (&image_data, &audio_data) {
        (Some((img, mime)), _) => format!("TextWithImage ({}bytes, {})", img.len(), mime),
        (None, Some(audio)) => format!("TextWithAudio ({}bytes)", audio.len()),
        (None, None) => format!("Text ({}chars)", text.len()),
    };
    println!("[GeminiLive] gemini_live_generate called: {}", content_type);
    println!(
        "[GeminiLive] instruction len: {}, streaming: {}",
        instruction.len(),
        streaming_enabled
    );

    // Build input content based on what's provided
    let content = match (image_data, audio_data) {
        (Some((img, mime)), _) => LiveInputContent::TextWithImage {
            text,
            image_data: img,
            mime_type: mime,
        },
        (None, Some(audio)) => {
            if text.trim().is_empty() {
                LiveInputContent::AudioOnly(audio)
            } else {
                LiveInputContent::TextWithAudio {
                    text,
                    audio_data: audio,
                }
            }
        }
        (None, None) => LiveInputContent::Text(text),
    };

    // Native-audio live turns should stay on the low-latency path.
    let show_thinking = false;

    // Send request to the manager
    let (id, rx) = GEMINI_LIVE_MANAGER.request(
        model,
        content,
        instruction,
        show_thinking,
        cancel_token.clone(),
        deadline,
    );
    println!("[GeminiLive] Request queued with ID: {}", id);

    let mut full_content = String::new();
    let mut thinking_shown = false;
    let mut content_started = false;
    let mut event_count = 0;

    let locale = crate::gui::locale::LocaleText::get(ui_language);

    // Process events from the worker
    loop {
        let event = match receive_live_event(&rx, &cancel_token, deadline, request_timeout) {
            Ok(Some(event)) => event,
            Ok(None) => {
                println!("[GeminiLive] Channel closed");
                if full_content.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Gemini Live channel closed before producing output"
                    ));
                }
                break;
            }
            Err(error) => return Err(error),
        };
        match event {
            LiveEvent::Thinking => {
                event_count += 1;
                println!("[GeminiLive] Event {}: Thinking", event_count);
                if !thinking_shown && !content_started {
                    if streaming_enabled {
                        on_chunk(locale.global_settings.model_thinking);
                    }
                    thinking_shown = true;
                }
            }
            LiveEvent::TextChunk(chunk) => {
                event_count += 1;
                println!(
                    "[GeminiLive] Event {}: TextChunk ({}bytes)",
                    event_count,
                    chunk.len()
                );
                if streaming_enabled {
                    // If we showed thinking, wipe it on first content
                    if !content_started && thinking_shown {
                        content_started = true;
                        full_content.push_str(&chunk);
                        let wipe_content = format!("{}{}", crate::api::WIPE_SIGNAL, full_content);
                        on_chunk(&wipe_content);
                    } else {
                        content_started = true;
                        full_content.push_str(&chunk);
                        on_chunk(&chunk);
                    }
                } else {
                    content_started = true;
                    full_content.push_str(&chunk);
                }
            }
            LiveEvent::TurnComplete => {
                event_count += 1;
                println!(
                    "[GeminiLive] Event {}: TurnComplete (total content: {}bytes)",
                    event_count,
                    full_content.len()
                );
                if !streaming_enabled && !full_content.is_empty() {
                    on_chunk(&full_content);
                }
                break;
            }
            LiveEvent::Error(e) => {
                event_count += 1;
                println!("[GeminiLive] Event {}: Error - {}", event_count, e);
                if e.contains("NO_API_KEY") {
                    crate::overlay::utils::show_api_key_error_notification(&e, ui_language);
                    return Err(anyhow::anyhow!("{}", e));
                }
                if e.contains("INVALID_API_KEY") {
                    crate::overlay::utils::show_api_key_error_notification(&e, ui_language);
                    return Err(anyhow::anyhow!("{}", e));
                }
                return Err(anyhow::anyhow!("Gemini Live error: {}", e));
            }
        }
    }

    println!(
        "[GeminiLive] gemini_live_generate complete: {}bytes result",
        full_content.len()
    );
    Ok(full_content)
}

fn receive_live_event(
    receiver: &mpsc::Receiver<LiveEvent>,
    cancel_token: &Option<Arc<AtomicBool>>,
    deadline: Option<Instant>,
    request_timeout: Option<Duration>,
) -> anyhow::Result<Option<LiveEvent>> {
    loop {
        if cancel_token
            .as_ref()
            .is_some_and(|token| token.load(Ordering::SeqCst))
        {
            anyhow::bail!("Gemini Live request cancelled");
        }
        let remaining = deadline.map(|value| value.saturating_duration_since(Instant::now()));
        if remaining.is_some_and(|duration| duration.is_zero()) {
            anyhow::bail!(
                "Gemini Live request timed out after {} ms",
                request_timeout.unwrap_or_default().as_millis()
            );
        }
        let wait = remaining
            .unwrap_or(Duration::from_millis(50))
            .min(Duration::from_millis(50));
        match receiver.recv_timeout(wait) {
            Ok(event) => return Ok(Some(event)),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(None),
        }
    }
}

#[cfg(test)]
mod request_boundary_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn receive_boundary_honors_deadline_without_an_event() {
        let (_tx, rx) = mpsc::channel();
        let timeout = Duration::from_millis(25);
        let started = Instant::now();
        let error = receive_live_event(&rx, &None, Some(Instant::now() + timeout), Some(timeout))
            .expect_err("an idle live request must time out");
        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn receive_boundary_honors_per_request_cancellation() {
        let (_tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(true));
        let error = receive_live_event(&rx, &Some(cancel), None, None)
            .expect_err("a cancelled live request must stop");
        assert!(error.to_string().contains("cancelled"));
    }
}
