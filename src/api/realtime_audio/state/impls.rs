use super::{
    AI_SILENCE_TIMEOUT_MS, MIN_FORCE_COMMIT_CHARS, RealtimeState, TranscriptionMethod,
    TranslationRequest, USER_SILENCE_TIMEOUT_MS,
};
use std::time::{Duration, Instant};

impl RealtimeState {
    pub fn new() -> Self {
        Self {
            frozen_prefix: String::new(),
            full_transcript: String::new(),
            display_transcript: String::new(),
            display_full_start: Some(0),
            transcript_committed_pos: 0,
            last_committed_pos: 0,
            last_processed_len: 0,
            committed_translation: String::new(),
            translation_revision: 0,
            uncommitted_translation: String::new(),
            uncommitted_source_start: 0,
            uncommitted_source_end: 0,
            display_translation: String::new(),
            translation_history: Vec::new(),
            last_transcript_append_time: Instant::now(),
            last_translation_update_time: Instant::now(),
            is_downloading: false,
            download_title: String::new(),
            download_message: String::new(),
            download_progress: 0.0,
            transcription_method: TranscriptionMethod::GeminiLive,
        }
    }

    fn sync_transcript_commit_pos_with_translation(&mut self) {
        match self.transcription_method {
            TranscriptionMethod::GeminiTranscribe => {
                self.transcript_committed_pos = self
                    .transcript_committed_pos
                    .max(self.last_committed_pos)
                    .min(self.full_transcript.len());
            }
            _ => {
                self.transcript_committed_pos =
                    self.last_committed_pos.min(self.full_transcript.len());
            }
        }
    }

    fn clamp_to_char_boundary(text: &str, mut index: usize) -> usize {
        index = index.min(text.len());
        while index > 0 && !text.is_char_boundary(index) {
            index -= 1;
        }
        index
    }

    /// Byte length of the longest common char prefix, in one pass. Every
    /// recognizer update runs this over the whole session transcript while the
    /// shared state lock is held, so it must stay linear in the transcript.
    fn shared_prefix_len(left: &str, right: &str) -> usize {
        left.char_indices()
            .zip(right.chars())
            .find(|((_, a), b)| a != b)
            .map_or(left.len().min(right.len()), |((index, _), _)| index)
    }

    /// Freeze the current transcript so it remains visible and copyable across
    /// session restarts (model/source switches). The new session starts fresh.
    pub fn freeze_current_transcript(&mut self) {
        self.translation_revision = self.translation_revision.wrapping_add(1);
        if !self.full_transcript.is_empty() {
            if self.frozen_prefix.is_empty() {
                self.frozen_prefix = std::mem::take(&mut self.full_transcript);
            } else {
                self.frozen_prefix.push(' ');
                self.frozen_prefix.push_str(&self.full_transcript);
            }
        }
        self.full_transcript.clear();
        self.transcript_committed_pos = 0;
        self.last_committed_pos = 0;
        self.last_processed_len = 0;
        self.committed_translation.clear();
        self.uncommitted_translation.clear();
        self.uncommitted_source_start = 0;
        self.uncommitted_source_end = 0;
        self.display_translation.clear();
        self.translation_history.clear();
        self.update_display_transcript();
    }

    fn reset_translation_state(&mut self) {
        self.translation_revision = self.translation_revision.wrapping_add(1);
        self.last_committed_pos = 0;
        self.last_processed_len = 0;
        self.committed_translation.clear();
        self.uncommitted_translation.clear();
        self.uncommitted_source_start = 0;
        self.uncommitted_source_end = 0;
        self.display_translation.clear();
        self.translation_history.clear();
    }

    fn rollback_translation_to(&mut self, byte_pos: usize) {
        if byte_pos == 0 {
            self.reset_translation_state();
            return;
        }
        if byte_pos >= self.last_committed_pos {
            // Divergence is after where translation stopped - no rollback needed.
            self.clear_uncommitted_translation();
            return;
        }
        // Keep committed_translation up to approximately the divergence point.
        // We can't perfectly map source bytes to translation bytes, so keep
        // the translation as-is and just move the source pointer back.
        // The next translation request will re-translate from the divergence.
        self.last_committed_pos = byte_pos;
        self.last_processed_len = byte_pos;
        self.uncommitted_translation.clear();
        self.uncommitted_source_start = byte_pos;
        self.uncommitted_source_end = byte_pos;
        self.update_display_translation();
    }

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
        self.display_translation = full;
    }

    pub fn append_transcript(&mut self, new_text: &str) {
        let previous_len = self.full_transcript.len();
        self.full_transcript.push_str(new_text);
        self.last_transcript_append_time = Instant::now();
        self.refresh_display_transcript(previous_len, previous_len);
        self.sync_transcript_commit_pos_with_translation();
    }

    pub fn set_transcription_method(&mut self, method: TranscriptionMethod) {
        self.transcription_method = method;
        self.sync_transcript_commit_pos_with_translation();
    }

    pub fn set_transcript_segments(&mut self, committed: &str, draft: &str) {
        let committed = Self::sanitize_transcript_segment(committed);
        let draft = Self::sanitize_transcript_segment(draft);

        let (full_transcript, server_committed_pos) = if committed.is_empty() {
            (draft.trim_start().to_string(), 0)
        } else if draft.is_empty() {
            (committed.clone(), committed.len())
        } else {
            let combined = Self::join_transcript_segments(&committed, &draft);
            let pos = committed.len();
            (combined, pos)
        };

        let server_committed_pos =
            Self::clamp_to_char_boundary(&full_transcript, server_committed_pos);
        let previous_transcript = std::mem::replace(&mut self.full_transcript, full_transcript);
        let previous_finality = self.transcript_committed_pos;
        let translation_boundary = self.last_committed_pos.min(previous_transcript.len());

        let shared_prefix = Self::shared_prefix_len(&previous_transcript, &self.full_transcript);
        self.refresh_display_transcript(shared_prefix, previous_transcript.len());
        self.transcript_committed_pos = server_committed_pos;
        self.invalidate_advanced_finality(previous_finality, server_committed_pos);
        self.last_transcript_append_time = Instant::now();

        if shared_prefix < translation_boundary {
            if self.transcription_method == TranscriptionMethod::GeminiTranscribe {
                self.reset_translation_state();
            } else if shared_prefix > 0 {
                // Partial rollback: keep translations up to the divergence point
                // instead of nuking everything; minor formatting corrections
                // (", " vs " , ") shouldn't discard all translation.
                self.rollback_translation_to(shared_prefix);
            } else {
                self.reset_translation_state();
            }
        } else {
            self.invalidate_processed_correction(shared_prefix, previous_transcript.len());
            // Transcript growth/corrections after the committed boundary must not blank the
            // currently visible draft. The translation loop will replace it when the next
            // provider response arrives; clearing here causes long draft disappearances.
            if self.uncommitted_source_start > self.full_transcript.len() {
                self.clear_uncommitted_translation();
            } else {
                self.update_display_translation();
            }
        }

        if self.transcription_method == TranscriptionMethod::GeminiTranscribe {
            self.transcript_committed_pos = server_committed_pos
                .max(self.last_committed_pos)
                .min(self.full_transcript.len());
        }
    }

    fn sanitize_transcript_segment(segment: &str) -> String {
        segment.replace(['\n', '\t'], " ")
    }

    fn join_transcript_segments(left: &str, right: &str) -> String {
        match (left.is_empty(), right.is_empty()) {
            (true, true) => String::new(),
            (true, false) => right.trim_start().to_string(),
            (false, true) => left.to_string(),
            (false, false) => {
                let left_has_space = left.chars().last().is_some_and(char::is_whitespace);
                let right_has_space = right.chars().next().is_some_and(char::is_whitespace);
                if left_has_space || right_has_space {
                    format!("{left}{right}")
                } else {
                    format!("{left} {right}")
                }
            }
        }
    }

    fn source_range_ends_with_sentence(&self, end: usize) -> bool {
        let sentence_delimiters = ['.', '!', '?', '。', '！', '？'];
        if self.last_committed_pos >= end || end > self.full_transcript.len() {
            return false;
        }
        if !self.full_transcript.is_char_boundary(end) {
            return false;
        }
        let uncommitted_source = &self.full_transcript[self.last_committed_pos..end];
        uncommitted_source
            .trim()
            .chars()
            .last()
            .map(|c| sentence_delimiters.contains(&c))
            .unwrap_or(false)
    }

    pub fn should_force_commit_on_timeout(&self) -> bool {
        if self.uncommitted_translation.is_empty() {
            return false;
        }

        let pending_end = self.uncommitted_source_end.min(self.full_transcript.len());
        if pending_end <= self.last_committed_pos
            || !self.full_transcript.is_char_boundary(pending_end)
        {
            return false;
        }
        let pending_len = pending_end - self.last_committed_pos;
        if pending_len < MIN_FORCE_COMMIT_CHARS {
            return false;
        }

        let now = Instant::now();
        let user_silent = now.duration_since(self.last_transcript_append_time)
            > Duration::from_millis(USER_SILENCE_TIMEOUT_MS);
        let ai_silent = now.duration_since(self.last_translation_update_time)
            > Duration::from_millis(AI_SILENCE_TIMEOUT_MS);

        let source_ready = self.source_range_ends_with_sentence(pending_end)
            || pending_end > self.last_committed_pos;

        source_ready && user_silent && ai_silent
    }

    pub fn force_commit_all(&mut self) {
        if self.uncommitted_translation.is_empty() {
            return;
        }

        let trans_segment = self.uncommitted_translation.trim().to_string();

        if !trans_segment.is_empty() {
            let pending_end = self.uncommitted_source_end.min(self.full_transcript.len());
            let source_segment = if self.last_committed_pos < pending_end
                && self.full_transcript.is_char_boundary(pending_end)
            {
                self.full_transcript[self.last_committed_pos..pending_end]
                    .trim()
                    .to_string()
            } else {
                "[continued]".to_string()
            };

            self.add_to_history(source_segment, trans_segment.clone());

            if self.committed_translation.is_empty() {
                self.committed_translation = trans_segment;
            } else {
                self.committed_translation.push(' ');
                self.committed_translation.push_str(&trans_segment);
            }

            self.last_committed_pos = pending_end;
            self.clear_uncommitted_translation();
        }

        self.update_display_translation();
        self.sync_transcript_commit_pos_with_translation();
    }

    /// Build a translation request over the full untranslated tail, keeping track of which
    /// prefix is safe to commit if the provider returns a usable result.
    pub fn get_translation_request(&self) -> Option<TranslationRequest> {
        if self.last_committed_pos >= self.full_transcript.len() {
            return None;
        }
        if !self
            .full_transcript
            .is_char_boundary(self.last_committed_pos)
        {
            return None;
        }

        let source_start = self.last_committed_pos;
        let source_end = self.full_transcript.len();
        let text = &self.full_transcript[source_start..source_end];
        if text.trim().is_empty() {
            return None;
        }

        let sentence_delimiters = ['.', '!', '?', '。', '！', '？'];
        let mut punctuated_len = 0;
        for (i, c) in text.char_indices() {
            if sentence_delimiters.contains(&c) {
                punctuated_len = i + c.len_utf8();
            }
        }
        let finalized_len = if self.transcription_method == TranscriptionMethod::GeminiTranscribe {
            let server_finalized_len =
                Self::clamp_to_char_boundary(&self.full_transcript, self.transcript_committed_pos)
                    .saturating_sub(source_start)
                    .min(text.len());
            server_finalized_len.max(punctuated_len)
        } else {
            punctuated_len
        };
        let raw_finalized_source = text[..finalized_len].to_string();
        let raw_draft_source = text[finalized_len..].to_string();
        let has_meaningful_draft = !raw_draft_source.trim().is_empty();
        let finalized_source_end = if has_meaningful_draft {
            source_start + finalized_len
        } else {
            source_end
        };

        Some(TranslationRequest {
            source_start,
            source_end,
            finalized_source_end,
            pending_source: text.to_string(),
            finalized_source: if has_meaningful_draft {
                raw_finalized_source
            } else {
                text.to_string()
            },
            draft_source: if has_meaningful_draft {
                raw_draft_source
            } else {
                String::new()
            },
            previous_draft_translation: if self.uncommitted_source_start == source_start
                && self.uncommitted_source_end == source_end
            {
                self.uncommitted_translation.clone()
            } else {
                String::new()
            },
        })
    }

    pub fn is_transcript_unchanged(&self) -> bool {
        self.full_transcript.len() == self.last_processed_len
    }

    pub fn mark_processed_up_to(&mut self, processed_len: usize) {
        if processed_len <= self.full_transcript.len()
            && self.full_transcript.is_char_boundary(processed_len)
        {
            self.last_processed_len = processed_len;
        }
    }

    pub fn clear_uncommitted_translation(&mut self) {
        self.uncommitted_translation.clear();
        self.uncommitted_source_start = self.last_committed_pos;
        self.uncommitted_source_end = self.last_committed_pos;
        self.update_display_translation();
    }

    pub fn apply_translation_result(
        &mut self,
        request: &TranslationRequest,
        finalized_translation: &str,
        draft_translation: &str,
    ) -> bool {
        if !request.matches_source(&self.full_transcript, self.last_committed_pos)
            || !self.request_finality_is_current(request)
            || !self
                .full_transcript
                .is_char_boundary(request.finalized_source_end)
        {
            return false;
        }

        if request.bytes_to_commit() > 0 && finalized_translation.trim().is_empty() {
            return false;
        }

        let normalized_draft_translation = if draft_translation.trim().is_empty() {
            request.fallback_draft_translation()
        } else {
            draft_translation.trim().to_string()
        };

        if request.requires_draft_translation() && normalized_draft_translation.is_empty() {
            return false;
        }

        if request.bytes_to_commit() > 0 {
            let final_text = finalized_translation.trim();
            if self.committed_translation.is_empty() {
                self.committed_translation = final_text.to_string();
            } else {
                self.committed_translation.push(' ');
                self.committed_translation.push_str(final_text);
            }
            let source_segment = request.finalized_source.trim().to_string();
            if !source_segment.is_empty() {
                self.add_to_history(source_segment, final_text.to_string());
            }
            self.last_committed_pos = request.finalized_source_end;
        }

        if request.draft_source.is_empty() {
            self.clear_uncommitted_translation();
        } else {
            self.uncommitted_translation = normalized_draft_translation;
            self.uncommitted_source_start = request.draft_source_start();
            self.uncommitted_source_end = request.source_end;
            self.last_translation_update_time = Instant::now();
            self.update_display_translation();
        }

        self.mark_processed_up_to(request.source_end);
        self.sync_transcript_commit_pos_with_translation();
        true
    }

    pub fn add_to_history(&mut self, source: String, translation: String) {
        self.translation_history.push((source, translation));
        while self.translation_history.len() > 3 {
            self.translation_history.remove(0);
        }
    }
}
