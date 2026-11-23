//! Audio recording, transcription, and streaming module.
//!
//! Split into submodules:
//! - `utils` - WAV encoding, PCM extraction, resampling, streaming window helpers
//! - `transcription` - API-based transcription (Gemini, Whisper/Groq)
//! - `gemini_live` - Real-time Gemini Live WebSocket streaming
//! - `recording` - Main recording functions

pub(crate) mod activity;
pub(crate) mod capture_diagnostics;
#[cfg(target_os = "windows")]
pub(crate) mod endpoint_id;
mod failure_notice;
mod gemini_live;
pub(crate) mod pcm;
pub(crate) mod pcm_capture;
mod recording;
pub(crate) mod retention;
mod transcription;
mod utils;

// Re-export public API
pub(crate) use gemini_live::record_and_stream_gemini_live;
pub use recording::{process_audio_file_request, record_audio_and_transcribe};
pub(crate) use transcription::execute_audio_block_cancellable;
pub use utils::{encode_wav, resample_linear_i16};
