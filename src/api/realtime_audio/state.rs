//! Shared state for realtime transcription and translation

#[path = "state/display.rs"]
mod display;
#[path = "state/impls.rs"]
mod impls;

use std::sync::{Arc, Mutex};
use std::time::Instant;

impl Default for RealtimeState {
    fn default() -> Self {
        Self::new()
    }
}

/// Timeout for User Silence (Wait for user to finish thought)
pub const USER_SILENCE_TIMEOUT_MS: u64 = 800;
/// Timeout for AI Silence (Wait if AI stops generating)
pub const AI_SILENCE_TIMEOUT_MS: u64 = 1000;

/// Minimum characters required to trigger a force-commit on silence
const MIN_FORCE_COMMIT_CHARS: usize = 10;

/// Transcription method being used
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TranscriptionMethod {
    #[default]
    GeminiLive,
    GeminiTranscribe,
    GeminiLiveS2s,
}

pub struct RealtimeState {
    /// Text from previous sessions — shown visually and copyable, but never re-processed.
    pub frozen_prefix: String,

    pub full_transcript: String,
    pub display_transcript: String,
    /// Byte offset of `full_transcript` inside `display_transcript` while the
    /// display holds the frozen prefix followed by the live transcript.
    display_full_start: Option<usize>,
    pub transcript_committed_pos: usize,

    /// Position after the last FULLY FINISHED sentence that was translated
    pub last_committed_pos: usize,
    /// The length of full_transcript when we last triggered a translation
    pub last_processed_len: usize,

    pub committed_translation: String,
    /// Changes whenever committed translation is cleared for a new source segment.
    pub translation_revision: u64,
    pub uncommitted_translation: String,
    pub uncommitted_source_start: usize,
    pub uncommitted_source_end: usize,
    pub display_translation: String,

    pub translation_history: Vec<(String, String)>,

    pub last_transcript_append_time: Instant,
    pub last_translation_update_time: Instant,

    pub is_downloading: bool,
    pub download_title: String,
    pub download_message: String,
    pub download_progress: f32,

    pub transcription_method: TranscriptionMethod,
}

impl RealtimeState {
    pub(super) fn invalidate_advanced_finality(&mut self, previous: usize, current: usize) {
        if self.transcription_method == TranscriptionMethod::GeminiTranscribe
            && current > previous
            && current > self.last_committed_pos
            && self.last_processed_len > self.last_committed_pos
        {
            self.last_processed_len = 0;
        }
    }

    pub(crate) fn request_finality_is_current(&self, request: &TranslationRequest) -> bool {
        self.transcription_method != TranscriptionMethod::GeminiTranscribe
            || self.transcript_committed_pos.min(request.source_end) <= request.finalized_source_end
    }

    pub(super) fn invalidate_processed_correction(
        &mut self,
        shared_prefix: usize,
        previous_len: usize,
    ) {
        if shared_prefix < previous_len && shared_prefix < self.last_processed_len {
            self.last_processed_len = 0;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslationRequest {
    pub source_start: usize,
    pub source_end: usize,
    pub finalized_source_end: usize,
    pub pending_source: String,
    pub finalized_source: String,
    pub draft_source: String,
    pub previous_draft_translation: String,
}

impl TranslationRequest {
    pub fn matches_source(&self, transcript: &str, last_committed_pos: usize) -> bool {
        self.source_start == last_committed_pos
            && transcript.get(self.source_start..self.source_end)
                == Some(self.pending_source.as_str())
    }

    pub fn bytes_to_commit(&self) -> usize {
        self.finalized_source_end.saturating_sub(self.source_start)
    }

    pub fn draft_source_start(&self) -> usize {
        self.finalized_source_end
    }

    pub fn requires_draft_translation(&self) -> bool {
        let trimmed = self.draft_source.trim();
        !trimmed.is_empty() && trimmed.chars().any(|c| c.is_alphanumeric())
    }

    pub fn fallback_draft_translation(&self) -> String {
        if self.requires_draft_translation() {
            String::new()
        } else {
            self.draft_source.trim().to_string()
        }
    }
}

pub type SharedRealtimeState = Arc<Mutex<RealtimeState>>;
