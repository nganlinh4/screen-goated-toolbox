//! Types for Gemini Live LLM API

use std::sync::mpsc;

/// Model for Gemini Live LLM (same as TTS/STT for consistency)
pub const GEMINI_LIVE_MODEL: &str = "gemini-2.5-flash-native-audio-preview-12-2025";

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
    /// Unique request ID
    pub id: u64,
    /// The input content
    pub content: LiveInputContent,
    /// System instruction (prompt)
    pub instruction: String,
    /// Whether to enable thinking display
    pub show_thinking: bool,
}

/// Queued request with generation tracking for interrupts
pub struct QueuedLiveRequest {
    pub req: LiveRequest,
    pub generation: u64,
    pub response_tx: mpsc::Sender<LiveEvent>,
}
