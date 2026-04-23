use super::qwen3::runtime::RuntimeTranscriptionResult;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MonotonicTranscriptSnapshot {
    pub session_epoch: u64,
    pub context_prefix_text: String,
    pub resume_prefix_text: String,
    pub committed_text: String,
    pub draft_text: String,
    pub display_text: String,
}

#[derive(Debug, Clone, Default)]
pub struct MonotonicTranscriptState {
    current_epoch: u64,
    current_context_prefix_text: String,
    epoch_base_text: String,
    epoch_fixed_text: String,
    committed_text: String,
    draft_text: String,
}

impl MonotonicTranscriptState {
    pub fn ingest(&mut self, result: &RuntimeTranscriptionResult) -> MonotonicTranscriptSnapshot {
        let context_prefix_text = sanitize_transcript_segment(&result.context_prefix_text);
        if result.session_epoch != self.current_epoch {
            if !context_prefix_text.is_empty()
                && !tail_matches(&self.committed_text, &context_prefix_text)
            {
                crate::log_info!(
                    "[QwenTranscript] checkpoint-context-mismatch epoch={} committed_tail_chars={} context_chars={}",
                    result.session_epoch,
                    self.committed_text.chars().count().min(64),
                    context_prefix_text.chars().count()
                );
            }
            self.current_epoch = result.session_epoch;
            self.current_context_prefix_text = context_prefix_text.clone();
            self.epoch_base_text = self.committed_text.clone();
            self.epoch_fixed_text.clear();
            self.draft_text.clear();
        }

        let fixed_text = sanitize_transcript_segment(&result.fixed_text);
        let raw_local_text = sanitize_transcript_segment(&result.text);
        let accepted_fixed_text = self.accept_fixed_text(&fixed_text);
        let draft_text = derive_draft_text(
            &accepted_fixed_text,
            &raw_local_text,
            &sanitize_transcript_segment(&result.draft_text),
        );
        self.epoch_fixed_text = accepted_fixed_text.clone();
        self.committed_text = join_transcript_segments(&self.epoch_base_text, &accepted_fixed_text);
        self.draft_text = draft_text;

        let display_text = join_transcript_segments(&self.committed_text, &self.draft_text);
        MonotonicTranscriptSnapshot {
            session_epoch: self.current_epoch,
            context_prefix_text,
            resume_prefix_text: sanitize_transcript_segment(&result.resume_prefix_text),
            committed_text: self.committed_text.clone(),
            draft_text: self.draft_text.clone(),
            display_text,
        }
    }

    fn accept_fixed_text(&self, candidate: &str) -> String {
        if candidate.is_empty() {
            return self.epoch_fixed_text.clone();
        }
        if self.epoch_fixed_text.is_empty() || candidate.starts_with(&self.epoch_fixed_text) {
            return candidate.to_string();
        }
        if self.epoch_fixed_text.starts_with(candidate) {
            return self.epoch_fixed_text.clone();
        }

        crate::log_info!(
            "[QwenTranscript] fixed-text-divergence epoch={} previous_chars={} candidate_chars={}",
            self.current_epoch,
            self.epoch_fixed_text.chars().count(),
            candidate.chars().count()
        );
        self.epoch_fixed_text.clone()
    }
}

fn sanitize_transcript_segment(segment: &str) -> String {
    segment.replace('\n', " ").replace('\t', " ")
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

fn derive_draft_text(
    accepted_fixed_text: &str,
    raw_local_text: &str,
    draft_fallback: &str,
) -> String {
    if accepted_fixed_text.is_empty() {
        if !raw_local_text.is_empty() {
            return raw_local_text.to_string();
        }
        return draft_fallback.to_string();
    }
    if let Some(rest) = raw_local_text.strip_prefix(accepted_fixed_text) {
        return rest.to_string();
    }
    if raw_local_text.is_empty() || accepted_fixed_text.starts_with(raw_local_text) {
        return String::new();
    }
    if let Some(rest) = draft_fallback.strip_prefix(accepted_fixed_text) {
        return rest.to_string();
    }
    String::new()
}

fn tail_matches(committed_text: &str, context_prefix_text: &str) -> bool {
    if committed_text.ends_with(context_prefix_text) {
        return true;
    }

    let committed_suffix = take_tail_chars(committed_text, context_prefix_text.chars().count());
    committed_suffix == context_prefix_text
}

fn take_tail_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars().rev().take(max_chars).collect::<Vec<_>>();
    chars.reverse();
    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::{MonotonicTranscriptState, RuntimeTranscriptionResult};

    #[test]
    fn keeps_transcript_monotonic_across_epoch_rollover() {
        let mut state = MonotonicTranscriptState::default();
        let first = state.ingest(&RuntimeTranscriptionResult {
            session_epoch: 1,
            fixed_text: "Hello there".to_string(),
            draft_text: " general".to_string(),
            resume_prefix_text: "there general".to_string(),
            ..RuntimeTranscriptionResult::default()
        });
        assert_eq!(first.committed_text, "Hello there");
        assert_eq!(first.display_text, "Hello there general");

        let second = state.ingest(&RuntimeTranscriptionResult {
            session_epoch: 2,
            context_prefix_text: "there general".to_string(),
            fixed_text: " Kenobi".to_string(),
            draft_text: String::new(),
            resume_prefix_text: "general Kenobi".to_string(),
            ..RuntimeTranscriptionResult::default()
        });
        assert_eq!(second.committed_text, "Hello there Kenobi");
        assert_eq!(second.display_text, "Hello there Kenobi");
    }

    #[test]
    fn keeps_committed_text_monotonic_when_fixed_text_retracts() {
        let mut state = MonotonicTranscriptState::default();
        let _ = state.ingest(&RuntimeTranscriptionResult {
            session_epoch: 3,
            fixed_text: "Claim he says".to_string(),
            draft_text: " one can".to_string(),
            ..RuntimeTranscriptionResult::default()
        });
        let corrected = state.ingest(&RuntimeTranscriptionResult {
            session_epoch: 3,
            fixed_text: "He says".to_string(),
            draft_text: " one can".to_string(),
            ..RuntimeTranscriptionResult::default()
        });
        assert_eq!(corrected.committed_text, "Claim he says");
        assert_eq!(corrected.display_text, "Claim he says");
    }
}
