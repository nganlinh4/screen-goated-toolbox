use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::utils::VideoFormatLists;

const MAX_SCANS: usize = 2;
static ACTIVE_SCANS: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
pub(super) struct AnalysisState {
    generation: u64,
    pub analyzing: bool,
    pub last_url: String,
    pub formats: Vec<String>,
    pub subtitles: Vec<String>,
    pub error: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
}

impl AnalysisState {
    pub fn invalidate(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.generation = self
            .generation
            .checked_add(1)
            .expect("analysis generation exhausted");
        self.analyzing = false;
        self.last_url.clear();
        self.formats.clear();
        self.subtitles.clear();
        self.error = None;
    }

    pub fn begin(&mut self, url: &str) -> (u64, Arc<AtomicBool>) {
        self.invalidate();
        let cancel = Arc::new(AtomicBool::new(false));
        self.analyzing = true;
        self.cancel = Some(cancel.clone());
        self.last_url = url.to_owned();
        (self.generation, cancel)
    }

    pub fn stop_for_download(&mut self, url: &str) {
        if self.analyzing {
            self.invalidate();
        }
        self.last_url = url.to_owned();
    }

    pub fn finish(&mut self, generation: u64, result: Result<VideoFormatLists, String>) {
        if self.generation != generation || !self.analyzing {
            return;
        }
        self.analyzing = false;
        self.cancel = None;
        match result {
            Ok((formats, subtitles, _automatic)) => {
                self.formats = formats;
                self.subtitles = subtitles;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
}

pub(super) type SharedAnalysis = Arc<Mutex<AnalysisState>>;

pub(super) fn choose_quality(
    preference: &mut Option<String>,
    selected: &mut Option<String>,
    choice: Option<String>,
) {
    *selected = choice.clone();
    *preference = choice;
}

pub(super) struct ScanPermit;

impl ScanPermit {
    pub fn acquire() -> Option<Self> {
        reserve(&ACTIVE_SCANS).then(|| Self)
    }
}

fn reserve(counter: &AtomicUsize) -> bool {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
            (count < MAX_SCANS).then(|| count + 1)
        })
        .is_ok()
}

impl Drop for ScanPermit {
    fn drop(&mut self) {
        ACTIVE_SCANS.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloading_retires_a_running_scan_and_consumes_the_pending_url() {
        let mut state = AnalysisState::default();
        state.stop_for_download("pending");
        assert_eq!(state.last_url, "pending");
        let (generation, cancel) = state.begin("active");
        state.stop_for_download("active");
        state.finish(generation, Err("late failure".into()));
        assert!(cancel.load(Ordering::Acquire));
        assert!(!state.analyzing);
        assert!(state.error.is_none());
        assert_eq!(state.last_url, "active");
    }

    #[test]
    fn shared_lifecycle_rejects_old_success_and_error() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/video-downloader/analysis-lifecycle.json"
        )))
        .unwrap();
        assert_eq!(fixture["maxConcurrentScans"], MAX_SCANS);
        let mut state = AnalysisState::default();
        let (first, cancel) = state.begin("a");
        state.invalidate();
        assert!(cancel.load(Ordering::SeqCst));
        let (second, _) = state.begin("a");
        state.finish(first, Ok((vec!["old".into()], vec!["old".into()], vec![])));
        state.finish(first, Err("old".into()));
        assert!(state.analyzing);
        assert!(state.formats.is_empty());
        state.finish(second, Ok((vec!["new".into()], vec![], vec![])));
        assert_eq!(state.formats, ["new"]);
        assert!(state.error.is_none());
    }

    #[test]
    fn retired_tab_cannot_publish_into_replacement() {
        let mut old_tab = AnalysisState::default();
        let (generation, cancel) = old_tab.begin("a");
        old_tab.invalidate();
        let mut new_tab = AnalysisState::default();
        let (new_generation, _) = new_tab.begin("b");
        old_tab.finish(generation, Ok((vec!["old".into()], vec![], vec![])));
        new_tab.finish(new_generation, Ok((vec!["new".into()], vec![], vec![])));
        assert!(cancel.load(Ordering::SeqCst));
        assert!(old_tab.formats.is_empty());
        assert_eq!(new_tab.formats, ["new"]);
        assert!(new_tab.subtitles.is_empty());
    }

    #[test]
    fn scan_admission_is_bounded_until_a_permit_retires() {
        let active = AtomicUsize::new(0);
        assert!(reserve(&active));
        assert!(reserve(&active));
        assert!(!reserve(&active));
        active.fetch_sub(1, Ordering::SeqCst);
        assert!(reserve(&active));
        assert_eq!(active.load(Ordering::SeqCst), MAX_SCANS);
    }

    #[test]
    fn rejected_permit_does_not_release_another_owner() {
        let first = ScanPermit::acquire().unwrap();
        let second = ScanPermit::acquire().unwrap();
        assert!(ScanPermit::acquire().is_none());
        assert_eq!(ACTIVE_SCANS.load(Ordering::SeqCst), MAX_SCANS);
        drop(first);
        drop(second);
        assert_eq!(ACTIVE_SCANS.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn explicit_quality_choice_updates_global_even_when_tab_already_matches() {
        let mut preference = Some("720p".into());
        let mut tab_best = None;
        choose_quality(&mut preference, &mut tab_best, None);
        assert_eq!(preference, None);
        let mut other_tab = Some("720p".into());
        choose_quality(&mut preference, &mut other_tab, Some("720p".into()));
        assert_eq!(preference.as_deref(), Some("720p"));
    }
}
