//! Per-segment transcript text for one speech-to-speech session.
//!
//! A long session produces thousands of segments. Rejoining every segment on
//! each text event made per-event work grow with the session, so settled
//! segments are folded once into an archive string and dropped from the map.
//! The map then holds only the recent window, and building the display reads
//! the archive plus that window.

use std::collections::{BTreeMap, BTreeSet};

/// Recent segments kept individually. A segment is folded into the archive
/// only after it is done and this many newer segments exist, so a late text
/// update for a settled segment is not plausible.
const SETTLE_LAG: u64 = 8;
/// A segment this far behind is settled even if its completion never arrived
/// (an error or a skipped segment), so one lost completion cannot pin the
/// window open for the rest of the session.
const FORCE_SETTLE_LAG: u64 = 4 * SETTLE_LAG;

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct S2sVisualText {
    pub committed: String,
    pub draft: String,
    pub full: String,
    pub committed_len: usize,
}

#[derive(Default)]
pub(super) struct SegmentTexts {
    /// Settled segments, trimmed and joined with single spaces.
    archive: String,
    recent: BTreeMap<u64, String>,
    done: BTreeSet<u64>,
}

impl SegmentTexts {
    pub fn get(&self, id: u64) -> &str {
        self.recent.get(&id).map(String::as_str).unwrap_or_default()
    }

    pub fn replace(&mut self, id: u64, text: String) {
        self.recent.insert(id, text);
        self.settle();
    }

    pub fn merge(&mut self, id: u64, incoming: &str) {
        super::merge_segment_text(self.recent.entry(id).or_default(), incoming);
        self.settle();
    }

    pub fn mark_done(&mut self, id: u64) {
        self.done.insert(id);
        self.settle();
    }

    /// The newest non-empty segment is the draft; everything before it,
    /// including the archive, is committed.
    pub fn visuals(&self) -> S2sVisualText {
        let mut parts = self
            .recent
            .values()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        let draft = parts.pop().unwrap_or_default().to_string();
        let mut committed = String::with_capacity(self.archive.len() + 64);
        committed.push_str(&self.archive);
        for part in parts {
            if !committed.is_empty() {
                committed.push(' ');
            }
            committed.push_str(part);
        }
        let full = match (committed.is_empty(), draft.is_empty()) {
            (true, _) => draft.clone(),
            (false, true) => committed.clone(),
            (false, false) => format!("{committed} {draft}"),
        };
        S2sVisualText {
            committed_len: committed.len(),
            committed,
            draft,
            full,
        }
    }

    /// Folds the oldest done segments into the archive while a newer
    /// non-empty segment exists (so the draft never moves into the archive)
    /// and at least `SETTLE_LAG` newer segments are held.
    fn settle(&mut self) {
        while let Some((&oldest, _)) = self.recent.first_key_value() {
            let newest = *self.recent.keys().next_back().unwrap_or(&oldest);
            let newer_text = self
                .recent
                .range(oldest + 1..)
                .any(|(_, value)| !value.trim().is_empty());
            let finished = self.done.contains(&oldest) || newest - oldest >= FORCE_SETTLE_LAG;
            if !finished || newest - oldest < SETTLE_LAG || !newer_text {
                break;
            }
            let text = self.recent.remove(&oldest).unwrap_or_default();
            self.done.remove(&oldest);
            let text = text.trim();
            if !text.is_empty() {
                if !self.archive.is_empty() {
                    self.archive.push(' ');
                }
                self.archive.push_str(text);
            }
        }
        // Completions for ids already folded, or that never carried text,
        // must not accumulate either.
        if let Some(&oldest) = self.recent.keys().next() {
            self.done = self.done.split_off(&oldest);
        }
    }

    #[cfg(test)]
    pub fn held_segments(&self) -> usize {
        self.recent.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The previous whole-history join, kept as the reference behaviour.
    fn rejoined(items: &BTreeMap<u64, String>) -> S2sVisualText {
        let mut segments = items
            .values()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        let draft = segments.pop().unwrap_or_default().to_string();
        let committed = segments.join(" ");
        let full = if committed.is_empty() {
            draft.clone()
        } else if draft.is_empty() {
            committed.clone()
        } else {
            format!("{committed} {draft}")
        };
        S2sVisualText {
            committed_len: committed.len(),
            committed,
            draft,
            full,
        }
    }

    #[test]
    fn a_long_session_holds_a_bounded_window_with_identical_display() {
        let mut texts = SegmentTexts::default();
        let mut history = BTreeMap::new();
        for id in 0..2_000_u64 {
            let text = if id % 7 == 3 {
                String::new()
            } else {
                format!(" segment {id} ")
            };
            texts.replace(id, text.clone());
            history.insert(id, text);
            if id >= 2 {
                texts.mark_done(id - 2);
            }
            assert_eq!(texts.visuals(), rejoined(&history), "after segment {id}");
        }
        assert!(texts.held_segments() <= 12, "{}", texts.held_segments());
    }

    #[test]
    fn segments_that_are_not_done_are_never_archived() {
        let mut texts = SegmentTexts::default();
        for id in 0..20 {
            texts.replace(id, format!("s{id}"));
        }
        assert_eq!(texts.held_segments(), 20);
        texts.merge(0, "s0 more");
        assert!(texts.visuals().committed.starts_with("s0 more s1"));
    }

    #[test]
    fn a_lost_completion_cannot_keep_the_window_growing() {
        let mut texts = SegmentTexts::default();
        let mut history = BTreeMap::new();
        for id in 0..500_u64 {
            texts.replace(id, format!("s{id}"));
            history.insert(id, format!("s{id}"));
            // Segment 5 never completes; every other segment does.
            if id != 5 {
                texts.mark_done(id);
            }
        }
        assert!(texts.held_segments() <= 33, "{}", texts.held_segments());
        assert_eq!(texts.visuals(), rejoined(&history));
    }

    #[test]
    fn the_draft_stays_out_of_the_archive_when_newer_segments_are_empty() {
        let mut texts = SegmentTexts::default();
        texts.replace(0, "spoken".to_string());
        texts.mark_done(0);
        for id in 1..20 {
            texts.replace(id, String::new());
            texts.mark_done(id);
        }
        let visuals = texts.visuals();
        assert_eq!(visuals.draft, "spoken");
        assert!(visuals.committed.is_empty());
    }
}
