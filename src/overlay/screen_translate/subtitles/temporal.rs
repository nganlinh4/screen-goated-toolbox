//! Time-based subtitle ownership. Pixels and provider completions cannot create revisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Revision {
    pub id: u64,
    pub utterance: u64,
    pub text: String,
}

#[derive(Default)]
pub(super) struct Tracker {
    next: u64,
    current: Option<Revision>,
    candidate: String,
    candidate_since: u64,
    changing_since: Option<u64>,
    observations: usize,
    growing: bool,
}

pub(super) struct Update {
    pub changed: bool,
    pub ready: Option<Revision>,
}

impl Tracker {
    #[cfg(test)]
    pub(super) fn current_id(&self) -> Option<u64> {
        self.current.as_ref().map(|r| r.id)
    }

    pub(super) fn reset(&mut self) {
        self.next += 1;
        self.current = None;
        self.candidate.clear();
        self.changing_since = None;
        self.observations = 0;
        self.growing = false;
    }

    pub(super) fn observe(&mut self, text: &str, now: u64) -> Update {
        // Only whitespace is normalized: punctuation, numbers, names and script remain evidence.
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text == self.candidate {
            self.observations += 1;
        } else {
            self.growing = !self.candidate.is_empty() && text.starts_with(&self.candidate);
            self.candidate = text.clone();
            self.candidate_since = now;
            self.observations = 1;
        }
        let differs = self.current.as_ref().map(|r| r.text.as_str()).unwrap_or("") != text;
        if !differs {
            self.changing_since = None;
            self.growing = false;
            return Update {
                changed: false,
                ready: None,
            };
        }
        let began = *self.changing_since.get_or_insert(now);
        // Repeated observations reject transient OCR misses; a bounded hold permits revisions
        // of text that is continuously typed rather than waiting forever for completion.
        let stable = self.observations >= 2
            && now.saturating_sub(self.candidate_since) >= if self.growing { 350 } else { 90 };
        let bounded = !text.is_empty() && now.saturating_sub(began) >= 800;
        if !stable && !bounded {
            return Update {
                changed: false,
                ready: None,
            };
        }
        self.next += 1;
        let utterance = self
            .current
            .as_ref()
            .filter(|old| text.starts_with(&old.text) || old.text.starts_with(&text))
            .map_or(self.next, |old| old.utterance);
        self.current = (!text.is_empty()).then_some(Revision {
            id: self.next,
            utterance,
            text,
        });
        self.changing_since = None;
        let ready = self.current.clone();
        Update {
            changed: true,
            ready,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typewriter_revisions_share_history_identity_until_blank_or_reset() {
        let mut tracker = Tracker::default();
        tracker.observe("Wait", 0);
        let first = tracker.observe("Wait", 100).ready.unwrap();
        tracker.observe("Wait here", 200);
        let grown = tracker.observe("Wait here", 600).ready.unwrap();
        assert_ne!(first.id, grown.id);
        assert_eq!(first.utterance, grown.utterance);
        tracker.observe("Go now", 700);
        let next = tracker.observe("Go now", 800).ready.unwrap();
        assert_ne!(next.utterance, grown.utterance);
        tracker.observe("", 900);
        assert!(tracker.observe("", 1000).changed);
        tracker.observe("Go now", 1100);
        let repeated = tracker.observe("Go now", 1200).ready.unwrap();
        assert_ne!(repeated.utterance, next.utterance);
        tracker.reset();
        tracker.observe("Go now", 1300);
        let reset = tracker.observe("Go now", 1400).ready.unwrap();
        assert_ne!(reset.utterance, repeated.utterance);
    }

    #[test]
    fn unchanged_text_does_not_retranslate_and_one_miss_does_not_clear() {
        let mut tracker = Tracker::default();
        assert!(tracker.observe("Stay here.", 0).ready.is_none());
        let first = tracker.observe("Stay here.", 100).ready.unwrap();
        assert!(!tracker.observe("", 200).changed);
        assert!(!tracker.observe("Stay here.", 300).changed);
        assert_eq!(tracker.current_id(), Some(first.id));
        assert!(!tracker.observe("", 400).changed);
        assert!(tracker.observe("", 510).changed);
        assert_eq!(tracker.current_id(), None);
    }

    #[test]
    fn small_meaningful_changes_receive_new_ownership() {
        let mut tracker = Tracker::default();
        tracker.observe("Take 10", 0);
        let first = tracker.observe("Take 10", 100).ready.unwrap();
        tracker.observe("Take 11", 200);
        let second = tracker.observe("Take 11", 300).ready.unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(second.text, "Take 11");
        tracker.reset();
        assert_ne!(tracker.current_id(), Some(second.id));
    }

    #[test]
    fn continuously_growing_text_has_a_bounded_wait_then_can_be_revised() {
        let mut tracker = Tracker::default();
        for index in 0..8 {
            assert!(
                tracker
                    .observe(&"a".repeat(index + 1), index as u64 * 100)
                    .ready
                    .is_none()
            );
        }
        assert!(tracker.observe("aaaaaaaaa", 800).ready.is_some());
        assert!(tracker.observe("Completed sentence", 900).ready.is_none());
        assert_eq!(
            tracker
                .observe("Completed sentence", 1000)
                .ready
                .unwrap()
                .text,
            "Completed sentence"
        );
    }
}
