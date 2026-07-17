//! Types for Gemini Live LLM API

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

/// Events sent from worker to caller
#[derive(Debug)]
pub enum LiveEvent {
    /// Text chunk received from the model
    TextChunk(String),
    /// Model is thinking (for models with thinking support)
    Thinking,
    /// Turn is complete
    TurnComplete,
    /// Error occurred
    Error(String),
}

/// Input content types for Gemini Live
#[derive(Clone, Debug)]
pub enum LiveInputContent {
    /// Text-only input
    Text(String),
    /// Text with image (base64 encoded)
    TextWithImage {
        text: String,
        image_data: Vec<u8>,
        mime_type: String,
    },
    /// Text with audio (PCM 16-bit mono 16kHz)
    TextWithAudio { text: String, audio_data: Vec<u8> },
    /// Audio-only input (for audio presets)
    AudioOnly(Vec<u8>),
}

/// A request to the Gemini Live LLM
#[derive(Clone)]
pub struct LiveRequest {
    /// Gemini Live API model name
    pub model: String,
    /// The input content
    pub content: LiveInputContent,
    /// System instruction (prompt)
    pub instruction: String,
    /// Whether to enable thinking display
    pub show_thinking: bool,
    /// Per-request cancellation; does not interrupt unrelated pooled requests.
    pub cancel_token: Option<Arc<AtomicBool>>,
    /// Optional wall-clock boundary inherited from the caller.
    pub deadline: Option<Instant>,
}

impl LiveRequest {
    pub fn is_cancelled_or_expired(&self) -> bool {
        self.cancel_token
            .as_ref()
            .is_some_and(|token| token.load(Ordering::SeqCst))
            || self
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
    }

    pub fn remaining(&self) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }
}

/// Queued request with generation tracking for interrupts
pub struct QueuedLiveRequest {
    pub req: LiveRequest,
    pub generation: u64,
    pub response_tx: mpsc::Sender<LiveEvent>,
}
