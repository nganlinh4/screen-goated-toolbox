//! Audio recording, transcription, and streaming module.
//!
//! Split into submodules:
//! - `utils` - WAV encoding, PCM extraction, resampling, streaming window helpers
//! - `transcription` - API-based transcription (Gemini, Whisper/Groq)
//! - `gemini_live` - Real-time Gemini Live WebSocket streaming
//! - `recording` - Main recording functions and Parakeet streaming

mod gemini_live;
mod recording;
mod transcription;
mod utils;

// Re-export public API
pub use gemini_live::record_and_stream_gemini_live;
pub use recording::{
    process_audio_file_request, record_and_stream_parakeet, record_audio_and_transcribe,
};
