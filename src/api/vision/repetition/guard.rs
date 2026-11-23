use super::{repetition_onset, scan_for_onset};

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
    /// The scanned prefix is full and was judged clean; later chunks cannot
    /// change that verdict, so streaming stops rescanning until `finish`.
    prefix_judged: bool,
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
        if self.prefix_judged || self.seen.len() < self.checked_len + CHECK_INTERVAL {
            return GuardAction::Paint;
        }
        self.checked_len = self.seen.len();
        // Judge only whole tokens: a reply in flight ends mid-word, which is
        // indistinguishable from the fragments produced by this defect.
        let complete = self
            .seen
            .rfind(char::is_whitespace)
            .map_or("", |end| &self.seen[..end]);
        let scan = scan_for_onset(complete, STREAMING_MIN_EVIDENCE);
        self.prefix_judged = scan.saturated && scan.onset.is_none();
        match scan.onset {
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
        self.prefix_judged = false;
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

    #[cfg(test)]
    pub(super) fn prefix_judged(&self) -> bool {
        self.prefix_judged
    }
}

#[cfg(test)]
#[path = "guard/tests.rs"]
mod tests;
