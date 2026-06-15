//! Pure offline-ASR (sherpa) transcript-commit state machine.
//!
//! This is the canonical commit/segmentation logic for streaming offline ASR,
//! extracted from the Windows sherpa loop so it is free of FFI / HWND / wall-clock
//! and can be unit-tested. It is the single source of truth that the Android port
//! mirrors (Kotlin `OfflineAsrStreamParity` + the shared `offline-asr-stream` golden
//! fixtures), replacing Android's previously-divergent inline commit glue.
//!
//! The machine derives the still-uncommitted `draft` from the recognizer text minus
//! the already-committed prefix, then decides when to promote the draft into the
//! finished history:
//! - with native punctuation: at the last interior sentence boundary, or when the
//!   draft ends in `.?!` and has been stable for >= 600 ms;
//! - without native punctuation: when the draft has been silent past the
//!   word-count-scaled threshold (`check_draft_commit`), appending a trailing `.`.

use super::state::check_draft_commit;
use super::utils::{append_history_segment, split_at_sentence_boundary};

/// How long a punctuation-terminated draft must be stable before it commits (ms).
const PUNCT_STALE_COMMIT_MS: u64 = 600;

/// Accumulating state for the offline-ASR commit machine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OfflineAsrCommitState {
    /// Finished, committed transcript text.
    pub committed_history: String,
    /// Portion of the current recognizer stream already committed. Advances but is
    /// never reset mid-speech; used to derive the still-uncommitted `draft`.
    pub stream_committed_prefix: String,
    /// The last draft text seen, for change detection.
    pub last_draft_text: String,
    /// Monotonic-clock timestamp (ms) when `last_draft_text` last changed.
    pub last_draft_change_ms: u64,
}

/// Advance the commit machine with the latest (already JSON-parsed, trimmed)
/// recognizer text. `now_ms` is a monotonic millisecond clock. Mutates the
/// committed history / prefix in place and returns the active (uncommitted) draft
/// to render after the committed history.
pub fn offline_asr_commit_step(
    state: &mut OfflineAsrCommitState,
    recognizer_text: &str,
    has_native_punctuation: bool,
    now_ms: u64,
) -> String {
    let text = recognizer_text.trim();
    let draft = if text.starts_with(state.stream_committed_prefix.as_str()) {
        text[state.stream_committed_prefix.len()..]
            .trim_start()
            .to_string()
    } else {
        text.to_string()
    };

    if draft != state.last_draft_text {
        state.last_draft_text = draft.clone();
        state.last_draft_change_ms = now_ms;
    }
    let elapsed_ms = now_ms.saturating_sub(state.last_draft_change_ms);

    if has_native_punctuation {
        if let Some((before, after)) = split_at_sentence_boundary(&draft) {
            append_history_segment(&mut state.committed_history, &before);
            state.stream_committed_prefix =
                text[..text.len() - after.len()].trim_end().to_string();
            state.last_draft_text.clear();
            state.last_draft_change_ms = now_ms;
            after.trim_start().to_string()
        } else if draft.trim_end().ends_with(['.', '?', '!']) && elapsed_ms >= PUNCT_STALE_COMMIT_MS
        {
            append_history_segment(&mut state.committed_history, &draft);
            state.stream_committed_prefix = text.trim_end().to_string();
            state.last_draft_text.clear();
            state.last_draft_change_ms = now_ms;
            String::new()
        } else {
            draft
        }
    } else if let Some(committed) = check_draft_commit(&draft, elapsed_ms) {
        let committed = format!("{committed}.");
        append_history_segment(&mut state.committed_history, &committed);
        state.stream_committed_prefix = text.trim_end().to_string();
        state.last_draft_text.clear();
        state.last_draft_change_ms = now_ms;
        String::new()
    } else {
        draft
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_draft_before_any_commit() {
        let mut s = OfflineAsrCommitState::default();
        // Non-punctuated draft, no silence elapsed -> just streamed as the active draft.
        let active = offline_asr_commit_step(&mut s, "hello world", false, 0);
        assert_eq!(active, "hello world");
        assert_eq!(s.committed_history, "");
    }

    #[test]
    fn commits_on_interior_sentence_boundary_with_punctuation() {
        let mut s = OfflineAsrCommitState::default();
        // "First sentence. second" -> commits "First sentence." leaving "second".
        let active = offline_asr_commit_step(&mut s, "First sentence. second", true, 100);
        assert_eq!(s.committed_history, "First sentence.");
        assert_eq!(active, "second");
        assert_eq!(s.stream_committed_prefix, "First sentence.");
    }

    #[test]
    fn commits_punctuation_terminated_draft_after_stale_period() {
        let mut s = OfflineAsrCommitState::default();
        // Draft appears at t=0; stays identical and ends in '.', so at t>=600 it commits.
        assert_eq!(offline_asr_commit_step(&mut s, "All done.", true, 0), "All done.");
        let active = offline_asr_commit_step(&mut s, "All done.", true, 600);
        assert_eq!(s.committed_history, "All done.");
        assert_eq!(active, "");
    }

    #[test]
    fn punctuation_draft_not_committed_before_stale_period() {
        let mut s = OfflineAsrCommitState::default();
        assert_eq!(offline_asr_commit_step(&mut s, "Wait.", true, 0), "Wait.");
        // Only 300ms stable -> still a draft, not committed.
        let active = offline_asr_commit_step(&mut s, "Wait.", true, 300);
        assert_eq!(active, "Wait.");
        assert_eq!(s.committed_history, "");
    }

    #[test]
    fn commits_unpunctuated_draft_after_silence_threshold() {
        let mut s = OfflineAsrCommitState::default();
        // 2 words -> threshold 1200/(1+2*0.5)=600ms. Stable from t=0; commits at t>=600 with a '.'.
        assert_eq!(offline_asr_commit_step(&mut s, "hello world", false, 0), "hello world");
        let active = offline_asr_commit_step(&mut s, "hello world", false, 600);
        assert_eq!(s.committed_history, "hello world.");
        assert_eq!(active, "");
    }

    #[test]
    fn strips_committed_prefix_to_form_the_draft() {
        let mut s = OfflineAsrCommitState {
            stream_committed_prefix: "Already committed".to_string(),
            ..Default::default()
        };
        // Recognizer keeps emitting the full text; only the suffix is the live draft.
        let active = offline_asr_commit_step(&mut s, "Already committed and more", false, 0);
        assert_eq!(active, "and more");
        assert_eq!(s.committed_history, "");
    }
}
