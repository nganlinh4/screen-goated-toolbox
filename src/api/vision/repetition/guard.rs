use super::{repetition_onset, repetition_onset_with_evidence};

/// Watches a streamed reply and replaces it once it starts repeating.
///
/// Streaming paints as it goes, so by the time the fault is visible the window
/// already shows part of it. The salvaged text replaces what was already drawn,
/// and everything after the onset is suppressed.
#[derive(Default)]
pub(crate) struct RepetitionGuard {
    seen: String,
    salvaged: Option<String>,
    checked_len: usize,
}

pub(crate) enum GuardAction {
    Paint,
    Replace(String),
    Suppress,
}

const CHECK_INTERVAL: usize = 48;
const STREAMING_MIN_EVIDENCE: usize = 64;

impl RepetitionGuard {
    pub(crate) fn observe(&mut self, chunk: &str) -> GuardAction {
        if self.salvaged.is_some() {
            return GuardAction::Suppress;
        }
        self.seen.push_str(chunk);
        if self.seen.len() < self.checked_len + CHECK_INTERVAL {
            return GuardAction::Paint;
        }
        self.checked_len = self.seen.len();
        // Judge only whole tokens: a reply in flight ends mid-word, which is
        // indistinguishable from the fragments produced by this defect.
        let complete = self
            .seen
            .rfind(char::is_whitespace)
            .map_or("", |end| &self.seen[..end]);
        match repetition_onset_with_evidence(complete, STREAMING_MIN_EVIDENCE) {
            Some(onset) => {
                let salvaged = complete[..onset].trim_end().to_string();
                self.salvaged = Some(salvaged.clone());
                GuardAction::Replace(salvaged)
            }
            None => GuardAction::Paint,
        }
    }

    pub(crate) fn restart(&mut self, text: &str) {
        self.seen.clear();
        self.seen.push_str(text);
        self.salvaged = None;
        self.checked_len = 0;
    }

    pub(crate) fn finish(mut self, streamed: String) -> String {
        if let Some(salvaged) = self.salvaged.take() {
            return salvaged;
        }
        match repetition_onset(&streamed) {
            Some(onset) => streamed[..onset].trim_end().to_string(),
            None => streamed,
        }
    }
}
