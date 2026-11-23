//! Transcript assembly and structural input-epoch ownership.

use crate::api::realtime_audio::s2s::utils::largest_suffix_prefix_overlap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::overlay::computer_control::runtime) struct InputTranscriptUpdate {
    pub(in crate::overlay::computer_control::runtime) text: String,
    pub(in crate::overlay::computer_control::runtime) starts_turn: bool,
    pub(in crate::overlay::computer_control::runtime) changed: bool,
    pub(in crate::overlay::computer_control::runtime) fragment_index: usize,
}

/// Joins Live input-transcription deltas within one user turn. The endpoint may
/// send a growing cumulative string, overlapping chunks, or exact duplicates.
/// Lifecycle code resets this only at a structural user/model turn boundary.
#[derive(Default)]
pub(in crate::overlay::computer_control::runtime) struct InputTranscriptAssembler {
    text: String,
    open: bool,
    /// Set by eligible speech after a response boundary, provider interruption,
    /// or explicit text submission. A transcript without this signal while the
    /// previous epoch is still open is a late revision, not a new request.
    fresh_epoch: bool,
    fragment_count: usize,
    epoch: u64,
}

impl InputTranscriptAssembler {
    pub(in crate::overlay::computer_control::runtime) fn merge(
        &mut self,
        fragment: &str,
    ) -> Option<InputTranscriptUpdate> {
        let fragment = fragment.trim();
        if fragment.is_empty() {
            return None;
        }
        let starts_turn = self.fresh_epoch || !self.open;
        if starts_turn {
            self.epoch = self.epoch.saturating_add(1);
            self.text.clear();
            self.text.push_str(fragment);
            self.open = true;
            self.fresh_epoch = false;
            self.fragment_count = 1;
            return Some(InputTranscriptUpdate {
                text: self.text.clone(),
                starts_turn,
                changed: true,
                fragment_index: self.fragment_count,
            });
        }
        self.fragment_count = self.fragment_count.saturating_add(1);
        let before = self.text.clone();
        merge_transcript_text(&mut self.text, fragment);
        Some(InputTranscriptUpdate {
            text: self.text.clone(),
            starts_turn,
            changed: self.text != before,
            fragment_index: self.fragment_count,
        })
    }

    pub(in crate::overlay::computer_control::runtime) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(in crate::overlay::computer_control::runtime) fn begin_epoch(&mut self) {
        self.fresh_epoch = true;
    }

    pub(in crate::overlay::computer_control::runtime) fn has_fresh_epoch(&self) -> bool {
        self.fresh_epoch
    }

    pub(in crate::overlay::computer_control::runtime) fn is_open(&self) -> bool {
        self.open
    }
}

fn merge_transcript_text(existing: &mut String, incoming: &str) {
    let current = existing.trim();
    if current.is_empty() || incoming.starts_with(current) {
        existing.clear();
        existing.push_str(incoming);
        return;
    }
    if current.starts_with(incoming) || current.ends_with(incoming) {
        return;
    }
    let overlap = largest_suffix_prefix_overlap(current, incoming);
    if overlap > 0 {
        existing.push_str(&incoming[overlap..]);
    } else {
        if !existing.ends_with(char::is_whitespace) {
            existing.push(' ');
        }
        existing.push_str(incoming);
    }
}
