//! Session-local accepted dialogue, independent of per-capture OCR region IDs.
use super::super::{
    contract::TranslationDocument,
    request::context::{DialogueLine, DialogueTurn},
};
use crate::config::types::ScreenTranslateSettings;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

const MAX_TURNS: usize = 4;
const MAX_BYTES: usize = 4096;
const MAX_AGE: Duration = Duration::from_secs(45);

struct Entry {
    utterance: u64,
    observed_at: Instant,
    turn: DialogueTurn,
}

#[derive(Default)]
pub(super) struct History {
    scope: Option<(u64, String, String, String)>,
    entries: VecDeque<Entry>,
}

impl History {
    pub fn sync(&mut self, epoch: u64, settings: &ScreenTranslateSettings) {
        let scope = (
            epoch,
            settings.target_language.clone(),
            settings.translation_prompt.clone(),
            settings.translation_model.clone(),
        );
        if self.scope.as_ref() != Some(&scope) {
            self.entries.clear();
            self.scope = Some(scope);
        }
    }

    #[cfg(test)]
    pub fn snapshot(&mut self, current_utterance: u64, now: Instant) -> Vec<DialogueTurn> {
        self.snapshot_excluding(&[current_utterance], now)
    }

    pub fn snapshot_excluding(&mut self, current: &[u64], now: Instant) -> Vec<DialogueTurn> {
        self.prune(now);
        self.entries
            .iter()
            .filter(|e| !current.contains(&e.utterance))
            .map(|e| e.turn.clone())
            .collect()
    }

    pub fn commit(
        &mut self,
        utterance: u64,
        observed_at: Instant,
        document: TranslationDocument,
        now: Instant,
    ) {
        let regions = document.regions;
        let turn = DialogueTurn {
            lines: regions
                .into_iter()
                .map(|r| DialogueLine {
                    source: r.source_text,
                    translation: r.translated_segments,
                })
                .collect(),
        };
        self.insert(utterance, observed_at, turn, now);
    }

    fn insert(&mut self, utterance: u64, observed_at: Instant, turn: DialogueTurn, now: Instant) {
        self.entries
            .retain(|e| e.utterance != utterance && e.turn != turn);
        let fits = serde_json::to_vec(&[&turn]).is_ok_and(|bytes| bytes.len() <= MAX_BYTES);
        if fits
            && !turn.lines.is_empty()
            && turn.lines.iter().all(|l| {
                !l.source.is_empty() && l.translation.iter().any(|text| !text.trim().is_empty())
            })
        {
            self.entries.push_back(Entry {
                utterance,
                observed_at,
                turn,
            });
        }
        self.prune(now);
    }

    fn prune(&mut self, now: Instant) {
        self.entries
            .retain(|e| now.saturating_duration_since(e.observed_at) <= MAX_AGE);
        // Bound the actual JSON bytes, including escaping and multilingual UTF-8.
        while self.entries.len() > MAX_TURNS
            || serde_json::to_vec(&self.entries.iter().map(|e| &e.turn).collect::<Vec<_>>())
                .map_or(true, |bytes| bytes.len() > MAX_BYTES)
        {
            if self.entries.pop_front().is_none() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn turn(source: &str) -> DialogueTurn {
        DialogueTurn {
            lines: vec![DialogueLine {
                source: source.into(),
                translation: vec![format!("translation {source}")],
            }],
        }
    }
    #[test]
    fn revisions_replace_and_current_utterance_never_prompts_itself() {
        let now = Instant::now();
        let mut h = History::default();
        h.insert(1, now, turn("first"), now);
        h.insert(2, now, turn("next"), now);
        assert_eq!(h.snapshot(2, now), vec![turn("first")]);
        h.insert(2, now, turn("next line"), now);
        assert_eq!(h.snapshot(3, now), vec![turn("first"), turn("next line")]);
        h.insert(3, now, turn("next line"), now);
        assert_eq!(h.snapshot(4, now).len(), 2);
    }
    #[test]
    fn age_count_and_serialized_bytes_are_bounded_without_truncating_lines() {
        let now = Instant::now();
        let mut h = History::default();
        for n in 0..10 {
            h.insert(n, now, turn(&format!("{n}")), now);
        }
        assert_eq!(h.snapshot(99, now).len(), MAX_TURNS);
        h.insert(11, now, turn(&"界".repeat(MAX_BYTES)), now);
        assert_eq!(h.snapshot(99, now).len(), MAX_TURNS);
        assert!(serde_json::to_vec(&h.snapshot(99, now)).unwrap().len() <= MAX_BYTES);
        h.insert(12, now, turn("recent"), now);
        assert!(
            h.snapshot(99, now + MAX_AGE + Duration::from_millis(1))
                .is_empty()
        );
    }
    #[test]
    fn region_language_prompt_and_model_changes_clear_context() {
        let now = Instant::now();
        let mut h = History::default();
        let mut s = ScreenTranslateSettings::default();
        for step in 0..4 {
            h.sync(1, &s);
            h.insert(1, now, turn("old"), now);
            match step {
                0 => s.target_language.push('x'),
                1 => s.translation_prompt.push('x'),
                2 => s.translation_model.push('x'),
                _ => {}
            }
            h.sync(if step == 3 { 2 } else { 1 }, &s);
            assert!(h.snapshot(2, now).is_empty());
        }
    }
}
