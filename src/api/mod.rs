pub mod types;
pub mod client;
pub mod vision;
pub mod audio;
pub mod text;
pub mod realtime_audio;
pub mod ollama;

pub use vision::translate_image_streaming;
pub use text::{translate_text_streaming, refine_text_streaming};
pub use audio::record_audio_and_transcribe;
pub use realtime_audio::{start_realtime_transcription, RealtimeState, SharedRealtimeState, get_realtime_display_text, get_translation_display_text};

/// Special prefix signal that tells callbacks to clear their accumulator before processing
/// When a chunk starts with this, the callback should: 1) Clear acc 2) Add the content after this prefix
pub const WIPE_SIGNAL: &str = "\x00WIPE\x00";
