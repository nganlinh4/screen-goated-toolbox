//! Gemini Live LLM API
//!
//! This module provides access to Gemini's native audio model as a standard LLM,
//! using the bidirectional WebSocket API for low-latency streaming text responses.
//!
//! Unlike the standard REST API, this uses a connection pool for faster response times.
//! Supports text, image, and audio inputs with text-only output.

pub mod client_message;
#[cfg(not(feature = "recorder-worker"))]
pub mod lifecycle;
pub mod manager;
mod request;
mod response;
mod response_deadlines;
pub mod setup_failure;
mod wait;
pub use request::LiveResponse;

pub mod ready_session;
pub mod server_frame;
pub mod setup;
#[cfg(test)]
mod test_support;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) mod transcription_recovery;
pub mod transport;
pub mod types;
pub mod websocket;
pub mod worker;

#[cfg(all(test, not(feature = "recorder-worker")))]
mod lifecycle_tests;

use std::sync::{Arc, LazyLock, atomic::AtomicBool};
use std::time::{Duration, Instant};

pub use manager::GeminiLiveManager;
pub use types::{LiveEvent, LiveInputContent, LiveRequest};

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
    pub api_key: String,
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
    on_chunk: F,
) -> anyhow::Result<String>
where
    F: FnMut(&str),
{
    let GeminiLiveGenerateRequest {
        api_key,
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
    if api_key.trim().is_empty() {
        crate::overlay::utils::show_api_key_error_notification("NO_API_KEY:gemini", ui_language);
        anyhow::bail!("NO_API_KEY:gemini");
    }
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
    let first_output_received = Arc::new(AtomicBool::new(false));
    let options = wait::WaitOptions {
        cancel_token: cancel_token.clone(),
        deadline,
        request_timeout,
        first_output_received: first_output_received.clone(),
    };
    let response = GEMINI_LIVE_MANAGER.request(LiveRequest {
        api_key,
        model,
        content,
        instruction,
        show_thinking,
        cancel_token: cancel_token.clone(),
        deadline,
        first_output_received: first_output_received.clone(),
    });
    println!("[GeminiLive] Request queued with ID: {}", response.id);

    response::collect_response(response, &options, streaming_enabled, ui_language, on_chunk)
}
