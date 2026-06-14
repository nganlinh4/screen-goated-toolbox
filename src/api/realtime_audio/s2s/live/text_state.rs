use super::super::*;

#[derive(Default)]
pub(super) struct LiveTranslateTextState {
    source_committed: String,
    source_draft: String,
    target_committed: String,
    target_draft: String,
}

impl LiveTranslateTextState {
    pub(super) fn update_source(&mut self, incoming: &str) -> bool {
        let before = (self.source_committed.clone(), self.source_draft.clone());
        update_live_text_pair(&mut self.source_committed, &mut self.source_draft, incoming);
        before.0 != self.source_committed || before.1 != self.source_draft
    }

    pub(super) fn update_target(&mut self, incoming: &str) -> bool {
        let before = (self.target_committed.clone(), self.target_draft.clone());
        update_live_text_pair(&mut self.target_committed, &mut self.target_draft, incoming);
        before.0 != self.target_committed || before.1 != self.target_draft
    }

    pub(super) fn snapshot_event(&self) -> S2sEvent {
        let source_full = join_live_text(&self.source_committed, &self.source_draft);
        S2sEvent::LiveText {
            source_committed_len: self.source_committed.len(),
            source_full,
            target_committed: self.target_committed.clone(),
            target_draft: self.target_draft.clone(),
        }
    }
}

fn update_live_text_pair(committed: &mut String, draft: &mut String, incoming: &str) {
    let incoming = incoming.trim();
    if incoming.is_empty() {
        return;
    }
    if draft.is_empty() {
        draft.push_str(incoming);
        maybe_commit_live_draft(committed, draft);
        return;
    }
    if incoming == draft.trim() || draft.trim_start().starts_with(incoming) {
        return;
    }
    if incoming.starts_with(draft.trim()) {
        draft.clear();
        draft.push_str(incoming);
        maybe_commit_live_draft(committed, draft);
        return;
    }

    let overlap = largest_suffix_prefix_overlap(draft.trim_end(), incoming);
    if overlap > 0 {
        merge_segment_text(draft, incoming);
        maybe_commit_live_draft(committed, draft);
        return;
    }

    commit_live_draft(committed, draft);
    draft.push_str(incoming);
    maybe_commit_live_draft(committed, draft);
}

fn maybe_commit_live_draft(committed: &mut String, draft: &mut String) {
    let trimmed = draft.trim();
    let word_count = trimmed.split_whitespace().count();
    let ends_sentence = trimmed
        .chars()
        .last()
        .is_some_and(|ch| matches!(ch, '.' | '?' | '!' | '。' | '？' | '！'));
    if ends_sentence || word_count >= 18 {
        commit_live_draft(committed, draft);
    }
}

fn commit_live_draft(committed: &mut String, draft: &mut String) {
    let trimmed = draft.trim();
    if trimmed.is_empty() {
        draft.clear();
        return;
    }
    if !committed.is_empty() {
        committed.push(' ');
    }
    committed.push_str(trimmed);
    draft.clear();
}

fn join_live_text(committed: &str, draft: &str) -> String {
    if committed.is_empty() {
        draft.trim().to_string()
    } else if draft.trim().is_empty() {
        committed.to_string()
    } else {
        format!("{} {}", committed, draft.trim())
    }
}
