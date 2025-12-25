//! Shared state for realtime transcription and translation

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Timeout for auto-committing sentences when no new words arrive (milliseconds)
pub const SENTENCE_COMMIT_TIMEOUT_MS: u64 = 1000;

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
    
    /// Timestamp of when transcript was last appended (for timeout-based commit)
    pub last_transcript_append_time: Instant,
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
            format!("{} {}", self.committed_translation, self.uncommitted_translation)
        };
        // No truncation - WebView handles smooth scrolling
        self.display_translation = full;
    }

        
    /// Append new transcript text and update display
    pub fn append_transcript(&mut self, new_text: &str) {
        self.full_transcript.push_str(new_text);
        self.last_transcript_append_time = Instant::now();
        self.update_display_transcript();
    }
    
    /// Check if uncommitted source text ends with a sentence delimiter
    pub fn source_ends_with_sentence(&self) -> bool {
        let sentence_delimiters = ['.', '!', '?', '。', '！', '？'];
        if self.last_committed_pos >= self.full_transcript.len() {
            return false;
        }
        let uncommitted_source = &self.full_transcript[self.last_committed_pos..];
        uncommitted_source.trim().chars().last()
            .map(|c| sentence_delimiters.contains(&c))
            .unwrap_or(false)
    }
    
    /// Check if we should force-commit due to timeout
    /// Returns true if: uncommitted translation exists, AND either:
    /// - Source ends with sentence delimiter, OR
    /// - Transcription is already fully committed (translation lagging)
    /// AND no new words for 1+ second
    pub fn should_force_commit_on_timeout(&self) -> bool {
        if self.uncommitted_translation.is_empty() {
            return false;
        }
        
        // Either source ends with sentence OR transcription is fully committed (translation lagging)
        let source_ready = self.source_ends_with_sentence() 
            || self.last_committed_pos >= self.full_transcript.len();
        
        source_ready && self.last_transcript_append_time.elapsed() > Duration::from_millis(SENTENCE_COMMIT_TIMEOUT_MS)
    }
    
    /// Force commit all uncommitted content (used for timeout-based commit)
    /// This bypasses the normal sentence-matching logic and commits everything as-is
    pub fn force_commit_all(&mut self) {
        if self.uncommitted_translation.is_empty() {
            return;
        }
        
        let trans_segment = self.uncommitted_translation.trim().to_string();
        
        if !trans_segment.is_empty() {
            // Get source segment for history (may be empty if transcription already committed)
            let source_segment = if self.last_committed_pos < self.full_transcript.len() {
                self.full_transcript[self.last_committed_pos..].trim().to_string()
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
        if !self.full_transcript.is_char_boundary(self.last_committed_pos) {
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
    
    /// Commit finished sentences after successful translation
    /// Matches sentence delimiters between source and translation, then commits all matched pairs.
    /// For long sentences, PROACTIVELY uses commas as split points before waiting for sentence end.
    /// Uses a low-threshold pressure valve for single sentences to avoid excessive re-translation.
    /// Returns true if any sentences were committed.
    pub fn commit_finished_sentences(&mut self) -> bool {
        let sentence_delimiters = ['.', '!', '?', '。', '！', '？'];
        let clause_delimiters = [',', ';', ':', '，', '；', '：'];
        
        // Thresholds for proactive comma splitting
        const LONG_SENTENCE_THRESHOLD: usize = 60;  // Start looking for comma after this many chars
        const MIN_CLAUSE_LENGTH: usize = 20;        // Minimum chars before a comma to commit
        
        let uncommitted_len = self.uncommitted_translation.len();
        
        // Store all valid matches found: (source_absolute_end, translation_relative_end, is_clause)
        let mut matches: Vec<(usize, usize, bool)> = Vec::new();

        // 1. PROACTIVE COMMA SPLIT: If uncommitted is long, check for comma FIRST
        // This ensures we don't wait for sentence endings in long ongoing sentences
        if uncommitted_len > LONG_SENTENCE_THRESHOLD {
            let source_remaining = &self.full_transcript[self.last_committed_pos..];
            let trans_remaining = &self.uncommitted_translation;
            
            // Find the LAST comma that gives us at least MIN_CLAUSE_LENGTH chars
            // (prefer later split for more context while still being proactive)
            let src_clause_end = source_remaining.char_indices()
                .filter(|(_, c)| clause_delimiters.contains(c))
                .filter(|(i, _)| *i >= MIN_CLAUSE_LENGTH)
                .last()
                .map(|(i, c)| i + c.len_utf8());
            
            let trn_clause_end = trans_remaining.char_indices()
                .filter(|(_, c)| clause_delimiters.contains(c))
                .filter(|(i, _)| *i >= MIN_CLAUSE_LENGTH)
                .last()
                .map(|(i, c)| i + c.len_utf8());
            
            if let (Some(s_rel), Some(t_rel)) = (src_clause_end, trn_clause_end) {
                let s_abs = self.last_committed_pos + s_rel;
                matches.push((s_abs, t_rel, true));
            }
        }
        
        // 2. If no proactive comma split, look for sentence delimiters
        if matches.is_empty() {
            let mut temp_src_pos = self.last_committed_pos;
            let mut temp_trans_pos = 0;
            
            loop {
                if temp_src_pos >= self.full_transcript.len() { break; }
                if temp_trans_pos >= self.uncommitted_translation.len() { break; }
                
                let source_text = &self.full_transcript[temp_src_pos..];
                let trans_text = &self.uncommitted_translation[temp_trans_pos..];

                let src_sentence_end = source_text.char_indices()
                    .find(|(_, c)| sentence_delimiters.contains(c))
                    .map(|(i, c)| i + c.len_utf8());
                
                let trn_sentence_end = trans_text.char_indices()
                    .find(|(_, c)| sentence_delimiters.contains(c))
                    .map(|(i, c)| i + c.len_utf8());

                if let (Some(s_rel), Some(t_rel)) = (src_sentence_end, trn_sentence_end) {
                    let s_abs = temp_src_pos + s_rel;
                    let t_abs = temp_trans_pos + t_rel;
                    
                    matches.push((s_abs, t_abs, false));
                    temp_src_pos = s_abs;
                    temp_trans_pos = t_abs;
                } else {
                    break;
                }
            }
        }

        // 3. Decide how many to commit
        let num_matches = matches.len();
        let mut num_to_commit = num_matches;

        // Pressure Valve: For single sentence, still require minimum length
        // (but clause splits already checked for 30 char minimum)
        if num_matches == 1 && !matches[0].2 && self.uncommitted_translation.len() < 50 {
            num_to_commit = 0; // Wait for more text or another sentence
        }

        let mut did_commit = false;
        if num_to_commit > 0 {
            // Get the boundary of the last item we are committing
            let (final_src_pos, final_trans_pos, _is_clause) = matches[num_to_commit - 1];
            
            // Extract the text chunk we are committing
            let source_segment = self.full_transcript[self.last_committed_pos..final_src_pos].trim().to_string();
            let trans_segment = self.uncommitted_translation[..final_trans_pos].trim().to_string();
            
            if !source_segment.is_empty() && !trans_segment.is_empty() {
                // Add to history (Clean, stabilized context)
                self.add_to_history(source_segment, trans_segment.clone());
                
                // Add to committed string
                if self.committed_translation.is_empty() {
                    self.committed_translation = trans_segment;
                } else {
                    self.committed_translation.push(' ');
                    self.committed_translation.push_str(&trans_segment);
                }
                
                // Update the commit pointer
                self.last_committed_pos = final_src_pos;
                
                // Slice the uncommitted buffer
                self.uncommitted_translation = self.uncommitted_translation[final_trans_pos..].trim().to_string();
                
                did_commit = true;
            }
        }
        
        self.update_display_translation();
        did_commit
    }
    
    /// Start new translation (clears uncommitted, keeps committed)
    /// NOTE: Caller must update UI immediately after calling this to clear old partial
    pub fn start_new_translation(&mut self) {
        self.uncommitted_translation.clear();
    }
    
    /// Append to uncommitted translation and update display
    pub fn append_translation(&mut self, new_text: &str) {
        self.uncommitted_translation.push_str(new_text);
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
