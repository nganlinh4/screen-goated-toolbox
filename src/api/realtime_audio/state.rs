//! Shared state for realtime transcription and translation

mod impls;

use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Timeout for User Silence (Wait for user to finish thought)
/// Reduced from 2000ms to 800ms for snappier response with Parakeet
pub const USER_SILENCE_TIMEOUT_MS: u64 = 800;
/// Timeout for AI Silence (Wait if AI stops generating)
pub const AI_SILENCE_TIMEOUT_MS: u64 = 1000;

/// Minimum characters required to trigger a force-commit on silence
const MIN_FORCE_COMMIT_CHARS: usize = 10;

// ============================================
// PARAKEET-SPECIFIC TIMEOUT CONSTANTS
// ============================================

pub const PARAKEET_BASE_TIMEOUT_MS: u64 = 800;
pub const PARAKEET_SHORT_TIMEOUT_MS: u64 = 1200;
pub const PARAKEET_MIN_WORDS: usize = 2;
pub const PARAKEET_MIN_TIMEOUT_MS: u64 = 350;
pub const PARAKEET_TIMEOUT_DECAY_RATE: f64 = 2.5;

// ============================================
// CENTRALIZED DRAFT COMMIT (smooth formula)
// ============================================

/// Smooth commit threshold: longer drafts need shorter silence to commit.
/// Returns the silence duration (ms) needed before appending a period.
///
/// Formula: threshold = BASE / (1 + words * DECAY), clamped to [MIN, BASE].
///   - 0 words:  never commit (returns u64::MAX)
///   - 1 word:   ~800ms
///   - 3 words:  ~650ms
///   - 5 words:  ~550ms
///   - 8 words:  ~470ms
///   - Threshold decays toward 0 as word count grows — no floor.
///   - CJK characters each count as one word (no spaces in Chinese/Japanese).
///   - Long drafts commit on any brief pause.
const DRAFT_COMMIT_BASE_MS: f64 = 1200.0;
const DRAFT_COMMIT_DECAY: f64 = 0.5;

pub fn draft_commit_threshold_ms(draft: &str) -> u64 {
    // Count CJK characters individually as words (each char is a semantic unit)
    let cjk_count = draft.chars().filter(|&c| c as u32 > 0x2E80).count();
    let word_count = draft.split_whitespace().count() + cjk_count;
    if word_count == 0 {
        return u64::MAX;
    }
    let threshold = DRAFT_COMMIT_BASE_MS / (1.0 + word_count as f64 * DRAFT_COMMIT_DECAY);
    threshold as u64
}

/// Check if draft should be committed (stale silence exceeded smooth threshold).
/// Returns `Some(text)` if should commit, `None` otherwise.
pub fn check_draft_commit(draft: &str, silence_ms: u64) -> Option<String> {
    if draft.is_empty() {
        return None;
    }
    let threshold = draft_commit_threshold_ms(draft);
    if silence_ms >= threshold {
        let trimmed = draft.trim_end();
        Some(trimmed.to_string())
    } else {
        None
    }
}

/// Transcription method being used
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TranscriptionMethod {
    #[default]
    GeminiLive,
    Parakeet,
    Qwen3Local,
    SherpaZipformer,
    GeminiLiveS2s,
}

pub struct RealtimeState {
    /// Text from previous sessions — shown visually and copyable, but never re-processed.
    pub frozen_prefix: String,

    pub full_transcript: String,
    pub display_transcript: String,
    pub transcript_committed_pos: usize,

    /// Position after the last FULLY FINISHED sentence that was translated
    pub last_committed_pos: usize,
    /// The length of full_transcript when we last triggered a translation
    pub last_processed_len: usize,

    pub committed_translation: String,
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
    pub parakeet_segment_start_time: Instant,
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
