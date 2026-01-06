//! Shared state for realtime transcription and translation

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Timeout for User Silence (Wait for user to finish thought)
pub const USER_SILENCE_TIMEOUT_MS: u64 = 2000;
/// Timeout for AI Silence (Wait if AI stops generating)
pub const AI_SILENCE_TIMEOUT_MS: u64 = 2000;

// ============================================
// PARAKEET-SPECIFIC TIMEOUT CONSTANTS
// ============================================
// Parakeet doesn't produce punctuation, so we use time-based segmentation

/// Base timeout for Parakeet segments (800ms)
pub const PARAKEET_BASE_TIMEOUT_MS: u64 = 800;
/// Minimum word count before allowing timeout-based commit for Parakeet
pub const PARAKEET_MIN_WORDS: usize = 3;
/// Maximum segment length (chars) beyond which we aggressively decrease timeout
pub const PARAKEET_MAX_SEGMENT_CHARS: usize = 200;
/// Minimum timeout (ms) - floor for the decreasing formula
pub const PARAKEET_MIN_TIMEOUT_MS: u64 = 300;
/// How fast timeout decreases per character beyond threshold (1ms per 5 chars)
pub const PARAKEET_TIMEOUT_DECAY_RATE: f64 = 0.2;

/// Transcription method being used
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TranscriptionMethod {
    /// Gemini Live API - produces punctuation
    GeminiLive,
    /// Local Parakeet model - no punctuation
    Parakeet,
}

impl Default for TranscriptionMethod {
    fn default() -> Self {
        TranscriptionMethod::GeminiLive
    }
}

/// Shared state for realtime transcription
pub struct RealtimeState {
    /// Full transcript (used for translation and display)
    pub full_transcript: String,
    /// Display transcript (same as full - WebView handles scrolling)
    pub display_transcript: String,

    /// Position after the last FULLY FINISHED sentence that was translated
    pub last_committed_pos: usize,
    /// The length of full_transcript when we last triggered a translation
    pub last_processed_len: usize,

    /// Committed translation (finished sentences, never replaced)
    pub committed_translation: String,
    /// Current uncommitted translation (may be replaced when sentence grows)
    pub uncommitted_translation: String,
    /// Display translation (WebView handles scrolling)
    pub display_translation: String,

    /// Translation history for conversation context: (source_text, translation)
    /// Keeps last 3 entries to maintain consistent style/atmosphere
    pub translation_history: Vec<(String, String)>,

    /// When the user last spoke (Audio input)
    pub last_transcript_append_time: Instant,
    /// When the AI last sent a translation chunk
    pub last_translation_update_time: Instant,

    /// Download status for models
    pub is_downloading: bool,
    pub download_title: String,
    pub download_message: String,
    pub download_progress: f32,

    // ============================================
    // PARAKEET-SPECIFIC FIELDS
    // ============================================
    /// Which transcription method is being used (Gemini Live or Parakeet)
    /// This determines which commit logic to apply
    pub transcription_method: TranscriptionMethod,
    /// When the current uncommitted segment started (for Parakeet timeout)
    pub parakeet_segment_start_time: Instant,
}

impl RealtimeState {
    pub fn new() -> Self {
        Self {
            full_transcript: String::new(),
            display_transcript: String::new(),
            last_committed_pos: 0,
            last_processed_len: 0,
            committed_translation: String::new(),
            uncommitted_translation: String::new(),
            display_translation: String::new(),
            translation_history: Vec::new(),
            last_transcript_append_time: Instant::now(),
            last_translation_update_time: Instant::now(),
            is_downloading: false,
            download_title: String::new(),
            download_message: String::new(),
            download_progress: 0.0,
            // Parakeet-specific: default to GeminiLive (existing behavior)
            transcription_method: TranscriptionMethod::GeminiLive,
            parakeet_segment_start_time: Instant::now(),
        }
    }

    /// Update display transcript from full transcript
    fn update_display_transcript(&mut self) {
        // No truncation - WebView handles smooth scrolling
        self.display_transcript = self.full_transcript.clone();
    }

    /// Update display translation from committed + uncommitted
    fn update_display_translation(&mut self) {
        let full = if self.committed_translation.is_empty() {
            self.uncommitted_translation.clone()
        } else if self.uncommitted_translation.is_empty() {
            self.committed_translation.clone()
        } else {
            format!(
                "{} {}",
                self.committed_translation, self.uncommitted_translation
            )
        };
        // No truncation - WebView handles smooth scrolling
        self.display_translation = full;
    }

    /// Append new transcript text and update display
    pub fn append_transcript(&mut self, new_text: &str) {
        // If this is the first text after a commit, reset segment start time
        if self.transcription_method == TranscriptionMethod::Parakeet {
            if self.last_committed_pos >= self.full_transcript.len() {
                // Starting a new segment
                self.parakeet_segment_start_time = Instant::now();
            }
        }

        // Capitalization Logic for Parakeet:
        // If transcript is empty OR ends with a sentence delimiter (like the period we injected),
        // we should capitalize the first letter of this new chunk.
        let mut text_to_append = new_text.to_string();

        if self.transcription_method == TranscriptionMethod::Parakeet {
            let needs_cap =
                self.full_transcript.trim().is_empty() || self.source_ends_with_sentence();

            if needs_cap {
                // Handle " hello" -> " Hello" (preserve leading space)
                if let Some(first_char_idx) = text_to_append.find(|c: char| !c.is_whitespace()) {
                    // Get the char to be capitalized
                    let c = text_to_append.chars().nth(first_char_idx).unwrap();

                    // Reconstruct string: pre-space + uppercase char + rest
                    let pre_space = &text_to_append[..first_char_idx];
                    let rest = &text_to_append[first_char_idx + 1..];
                    text_to_append = format!("{}{}{}", pre_space, c.to_uppercase(), rest);
                }
            }
        }

        self.full_transcript.push_str(&text_to_append);
        self.last_transcript_append_time = Instant::now();
        self.update_display_transcript();
    }

    // ============================================
    // PARAKEET-SPECIFIC METHODS
    // ============================================

    /// Set the transcription method (called when starting transcription)
    /// IMPORTANT: This determines which commit logic is used
    pub fn set_transcription_method(&mut self, method: TranscriptionMethod) {
        self.transcription_method = method;
        if method == TranscriptionMethod::Parakeet {
            self.parakeet_segment_start_time = Instant::now();
        }
    }

    /// Count words in the uncommitted source text (for Parakeet minimum word check)
    fn count_uncommitted_words(&self) -> usize {
        if self.last_committed_pos >= self.full_transcript.len() {
            return 0;
        }
        if !self
            .full_transcript
            .is_char_boundary(self.last_committed_pos)
        {
            return 0;
        }
        let uncommitted = &self.full_transcript[self.last_committed_pos..];
        uncommitted.split_whitespace().count()
    }

    /// Calculate dynamic timeout for Parakeet based on segment length
    /// Formula: base_timeout - (segment_length * decay_rate)
    /// Clamped to PARAKEET_MIN_TIMEOUT_MS floor
    fn calculate_parakeet_timeout_ms(&self) -> u64 {
        let segment_len = if self.last_committed_pos >= self.full_transcript.len() {
            0
        } else if self
            .full_transcript
            .is_char_boundary(self.last_committed_pos)
        {
            self.full_transcript[self.last_committed_pos..].len()
        } else {
            0
        };

        // Start decreasing after some reasonable length
        let threshold = 30usize; // Start decay after 30 chars
        if segment_len <= threshold {
            return PARAKEET_BASE_TIMEOUT_MS;
        }

        // Calculate decay: longer segments = shorter timeout
        let excess_chars = segment_len - threshold;
        let decay = (excess_chars as f64 * PARAKEET_TIMEOUT_DECAY_RATE) as u64;

        let timeout = PARAKEET_BASE_TIMEOUT_MS.saturating_sub(decay);
        timeout.max(PARAKEET_MIN_TIMEOUT_MS)
    }

    /// Check if uncommitted source text ends with a sentence delimiter
    pub fn source_ends_with_sentence(&self) -> bool {
        let sentence_delimiters = ['.', '!', '?', '。', '！', '？'];
        if self.last_committed_pos >= self.full_transcript.len() {
            return false;
        }
        let uncommitted_source = &self.full_transcript[self.last_committed_pos..];
        uncommitted_source
            .trim()
            .chars()
            .last()
            .map(|c| sentence_delimiters.contains(&c))
            .unwrap_or(false)
    }

    /// Check if we should force-commit due to timeout.
    /// For Gemini Live: waits 2 seconds of silence (BOTH user AND AI)
    /// For Parakeet: ONLY checks user silence (800ms) - translation runs independently!
    pub fn should_force_commit_on_timeout(&self) -> bool {
        // For Parakeet: completely independent path
        // Only check user silence - let translation work in parallel
        if self.transcription_method == TranscriptionMethod::Parakeet {
            // Must have uncommitted source text
            if self.last_committed_pos >= self.full_transcript.len() {
                return false;
            }

            // Must have minimum words
            let word_count = self.count_uncommitted_words();
            if word_count < PARAKEET_MIN_WORDS {
                return false;
            }

            // Check ONLY user silence - NOT AI silence!
            // This allows transcription to segment and move forward
            // Translation will catch up independently
            let now = Instant::now();
            let user_timeout = self.calculate_parakeet_timeout_ms();
            let user_silent = now.duration_since(self.last_transcript_append_time)
                > Duration::from_millis(user_timeout);

            return user_silent;
        }

        // For Gemini Live: original behavior - wait for BOTH user AND AI silence
        if self.uncommitted_translation.is_empty() {
            return false;
        }

        let now = Instant::now();
        let user_silent = now.duration_since(self.last_transcript_append_time)
            > Duration::from_millis(USER_SILENCE_TIMEOUT_MS);
        let ai_silent = now.duration_since(self.last_translation_update_time)
            > Duration::from_millis(AI_SILENCE_TIMEOUT_MS);

        // Conditions to force commit:
        // 1. We have pending source text (translation is lagging) OR source ends with sentence.
        // 2. User has stopped talking (sentence likely done).
        // 3. AI has stopped streaming (it's stuck or finished).

        let source_ready = self.source_ends_with_sentence()
            || self.last_committed_pos < self.full_transcript.len();

        source_ready && user_silent && ai_silent
    }

    /// Force commit all uncommitted content (used for timeout-based commit)
    /// For Gemini: requires non-empty translation before committing
    /// For Parakeet: Injects punctuation to create logical sentences (does not force commit)
    pub fn force_commit_all(&mut self) {
        // For Parakeet: special handling - inject punctuation
        // Instead of forcing a commit, we inject a period to mark the segment end.
        // This converts the time-based segment into a "sentence" which the
        // translation loop (and commit_finished_sentences) can handle naturally.
        if self.transcription_method == TranscriptionMethod::Parakeet {
            // Only inject if we have uncommitted text and it doesn't already have punctuation
            if self.last_committed_pos < self.full_transcript.len()
                && !self.source_ends_with_sentence()
            {
                self.full_transcript.push_str(". ");
                self.update_display_transcript();
            }
            // We do NOT advance last_committed_pos or clear translation here.
            // We rely on `commit_finished_sentences()` to see the new period
            // and match it with the translation's period.
            return;
        }

        // For Gemini Live: original behavior - require non-empty translation
        if self.uncommitted_translation.is_empty() {
            return;
        }

        let trans_segment = self.uncommitted_translation.trim().to_string();

        if !trans_segment.is_empty() {
            // Get source segment for history (may be empty if transcription already committed)
            let source_segment = if self.last_committed_pos < self.full_transcript.len() {
                self.full_transcript[self.last_committed_pos..]
                    .trim()
                    .to_string()
            } else {
                // Transcription already committed - use a placeholder for history
                "[continued]".to_string()
            };

            // Add to history (for translation context continuity)
            self.add_to_history(source_segment, trans_segment.clone());

            // Append to committed translation
            if self.committed_translation.is_empty() {
                self.committed_translation = trans_segment;
            } else {
                self.committed_translation.push(' ');
                self.committed_translation.push_str(&trans_segment);
            }

            // Update commit pointer to end of transcript (in case it wasn't already)
            self.last_committed_pos = self.full_transcript.len();

            // Clear uncommitted
            self.uncommitted_translation.clear();
        }

        self.update_display_translation();
    }

    /// Get text to translate: from last_committed_pos to end
    /// Returns (text_to_translate, contains_finished_sentence)
    pub fn get_translation_chunk(&self) -> Option<(String, bool)> {
        if self.last_committed_pos >= self.full_transcript.len() {
            return None;
        }
        if !self
            .full_transcript
            .is_char_boundary(self.last_committed_pos)
        {
            return None;
        }
        let text = &self.full_transcript[self.last_committed_pos..];
        if text.trim().is_empty() {
            return None;
        }

        // Check if chunk contains any sentence delimiter
        let sentence_delimiters = ['.', '!', '?', '。', '！', '？'];
        let has_finished_sentence = text.chars().any(|c| sentence_delimiters.contains(&c));

        Some((text.trim().to_string(), has_finished_sentence))
    }

    /// Check if the transcript has grown since the last translation request
    pub fn is_transcript_unchanged(&self) -> bool {
        self.full_transcript.len() == self.last_processed_len
    }

    /// Mark the current transcript length as processed
    pub fn update_last_processed_len(&mut self) {
        self.last_processed_len = self.full_transcript.len();
    }

    /// Commit finished sentences (or clauses) to keep TTS flowing smoothly.
    ///
    /// LOGIC:
    /// 1. Finds matching delimiters in Source and Translation.
    /// 2. If the text is long, it accepts Commas (Clauses) as cut points.
    /// 3. Slices the buffers instead of clearing them, allowing partial commits.
    pub fn commit_finished_sentences(&mut self) -> bool {
        let sentence_delimiters = ['.', '!', '?', '。', '！', '？'];
        let clause_delimiters = [',', ';', ':', '，', '；', '：'];

        // Thresholds
        const LONG_SENTENCE_THRESHOLD: usize = 30; // Start looking for commas if buffer > 60 chars
        const MIN_CLAUSE_LENGTH: usize = 20; // Don't commit tiny fragments like "Oh,"

        let uncommitted_len = self.uncommitted_translation.len();

        // 1. Identify which delimiters we accept right now
        // Always accept full sentences. Accept commas only if text is getting long.
        let enable_clause_commit = uncommitted_len > LONG_SENTENCE_THRESHOLD;

        // Store valid matches: (source_absolute_end, translation_relative_end, is_clause)
        let mut matches: Vec<(usize, usize, bool)> = Vec::new();

        let mut temp_src_pos = self.last_committed_pos;
        let mut temp_trans_pos = 0;

        // 2. "Zipper" Scan: Try to match delimiters in order
        // We loop to find ALL available commits (e.g. if we received 2 full sentences at once)
        loop {
            if temp_src_pos >= self.full_transcript.len() {
                break;
            }
            if temp_trans_pos >= self.uncommitted_translation.len() {
                break;
            }

            let source_text = &self.full_transcript[temp_src_pos..];
            let trans_text = &self.uncommitted_translation[temp_trans_pos..];

            // Find next Sentence Delimiter in Source
            let src_sentence_end = source_text
                .char_indices()
                .find(|(_, c)| sentence_delimiters.contains(c))
                .map(|(i, c)| i + c.len_utf8());

            // Find next Clause Delimiter in Source (only if enabled)
            let src_clause_end = if enable_clause_commit {
                source_text
                    .char_indices()
                    .find(|(i, c)| *i >= MIN_CLAUSE_LENGTH && clause_delimiters.contains(c))
                    .map(|(i, c)| i + c.len_utf8())
            } else {
                None
            };

            // Pick the earliest one
            let (src_rel_end, is_clause) = match (src_sentence_end, src_clause_end) {
                (Some(s), Some(c)) => {
                    if s < c {
                        (s, false)
                    } else {
                        (c, true)
                    }
                }
                (Some(s), None) => (s, false),
                (None, Some(c)) => (c, true),
                (None, None) => break, // No more delimiters in source
            };

            // Now try to find a corresponding delimiter in Translation
            // We search roughly in the same ratio, but simply finding the *next* matching type is usually robust enough for streaming
            let trans_rel_end = if is_clause {
                trans_text
                    .char_indices()
                    .find(|(i, c)| *i >= MIN_CLAUSE_LENGTH && clause_delimiters.contains(c))
                    .map(|(i, c)| i + c.len_utf8())
            } else {
                trans_text
                    .char_indices()
                    .find(|(_, c)| sentence_delimiters.contains(c))
                    .map(|(i, c)| i + c.len_utf8())
            };

            if let Some(t_end) = trans_rel_end {
                // Found a match! Record absolute positions
                let s_abs = temp_src_pos + src_rel_end;
                let t_abs = temp_trans_pos + t_end;

                matches.push((s_abs, t_abs, is_clause));

                // Advance temp pointers to look for more
                temp_src_pos = s_abs;
                temp_trans_pos = t_abs;
            } else {
                // Found delimiter in Source but NOT in Translation yet.
                // Stop matching, we need to wait for AI to generate more.
                break;
            }
        }

        // 3. Execute the Commits
        // We take the LAST successful match found in the loop (greedy commit)
        if let Some(&(final_src_pos, final_trans_pos, _)) = matches.last() {
            // Extract the chunks
            let source_segment = self.full_transcript[self.last_committed_pos..final_src_pos]
                .trim()
                .to_string();
            let trans_segment = self.uncommitted_translation[..final_trans_pos]
                .trim()
                .to_string();

            if !source_segment.is_empty() && !trans_segment.is_empty() {
                // Add to History
                self.add_to_history(source_segment, trans_segment.clone());

                // Add to Committed String
                if self.committed_translation.is_empty() {
                    self.committed_translation = trans_segment;
                } else {
                    self.committed_translation.push(' ');
                    self.committed_translation.push_str(&trans_segment);
                }

                // Update Pointers
                self.last_committed_pos = final_src_pos;

                // SLICE the uncommitted buffer (Remove what we just committed, keep the rest)
                self.uncommitted_translation = self.uncommitted_translation[final_trans_pos..]
                    .trim_start()
                    .to_string();

                self.update_display_translation();
                return true;
            }
        }

        self.update_display_translation();
        false
    }

    /// Start new translation (clears uncommitted, keeps committed)
    /// NOTE: Caller must update UI immediately after calling this to clear old partial
    pub fn start_new_translation(&mut self) {
        self.uncommitted_translation.clear();
    }

    /// Append to uncommitted translation and update display
    pub fn append_translation(&mut self, new_text: &str) {
        self.uncommitted_translation.push_str(new_text);
        self.last_translation_update_time = Instant::now(); // Track AI activity!
        self.update_display_translation();
    }

    /// Add a completed translation to history for conversation context
    /// Keeps only the last 3 entries
    pub fn add_to_history(&mut self, source: String, translation: String) {
        self.translation_history.push((source, translation));
        // Keep only last 3 entries
        while self.translation_history.len() > 3 {
            self.translation_history.remove(0);
        }
    }

    /// Get translation history as messages for API request
    pub fn get_history_messages(&self, target_language: &str) -> Vec<serde_json::Value> {
        let mut messages = Vec::new();

        for (source, translation) in &self.translation_history {
            // User message: request to translate
            messages.push(serde_json::json!({
                "role": "user",
                "content": format!("Translate to {}:\n{}", target_language, source)
            }));
            // Assistant message: the translation
            messages.push(serde_json::json!({
                "role": "assistant",
                "content": translation
            }));
        }

        messages
    }
}

pub type SharedRealtimeState = Arc<Mutex<RealtimeState>>;
