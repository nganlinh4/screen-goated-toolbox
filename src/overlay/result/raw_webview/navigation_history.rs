/// History for the native navigation view. Browser-driven loads are recorded only
/// after they finish; host-requested loads update their existing slot for redirects.
pub(super) struct NavigationHistory {
    urls: Vec<String>,
    depth: usize,
    managed_load: bool,
}

const MAX_HISTORY_ENTRIES: usize = 128;

impl NavigationHistory {
    pub(super) fn new(url: String) -> Self {
        Self {
            urls: vec![url],
            depth: 1,
            managed_load: true,
        }
    }

    pub(super) fn open(&mut self, url: String) {
        self.urls.truncate(self.depth);
        if self.urls.len() == MAX_HISTORY_ENTRIES {
            self.urls.remove(0);
            self.depth -= 1;
        }
        self.urls.push(url);
        self.depth += 1;
        self.managed_load = true;
    }

    pub(super) fn back(&mut self) -> Option<Option<String>> {
        if self.depth == 0 {
            return None;
        }
        self.depth -= 1;
        if self.depth == 0 {
            return Some(None);
        }
        self.managed_load = true;
        Some(Some(self.urls[self.depth - 1].clone()))
    }

    pub(super) fn forward(&mut self) -> Option<String> {
        if self.depth >= self.urls.len() {
            return None;
        }
        let url = self.urls[self.depth].clone();
        self.depth += 1;
        self.managed_load = true;
        Some(url)
    }

    pub(super) fn finished(&mut self, url: Option<String>) {
        if let Some(url) = url
            && self.depth > 0
        {
            if self.managed_load {
                self.urls[self.depth - 1] = url;
            } else if self.urls[self.depth - 1] != url {
                self.open(url);
            }
        }
        self.managed_load = false;
    }

    pub(super) fn depth(&self) -> usize {
        self.depth
    }

    pub(super) fn retire_failed(&mut self) {
        self.depth = 0;
        self.managed_load = false;
    }
    pub(super) fn max_depth(&self) -> usize {
        self.urls.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_HISTORY_ENTRIES, NavigationHistory};

    #[test]
    fn page_navigation_is_recorded_and_back_forward_reuses_slots() {
        let mut history = NavigationHistory::new("https://one.test/".into());
        history.finished(Some("https://one.test/final".into()));
        assert_eq!((history.depth(), history.max_depth()), (1, 1));
        history.finished(Some("https://two.test/".into()));
        assert_eq!((history.depth(), history.max_depth()), (2, 2));
        assert_eq!(history.back(), Some(Some("https://one.test/final".into())));
        history.finished(Some("https://one.test/final".into()));
        assert_eq!(history.forward(), Some("https://two.test/".into()));
        history.finished(Some("https://two.test/redirected".into()));
        assert_eq!((history.depth(), history.max_depth()), (2, 2));
        assert_eq!(history.back(), Some(Some("https://one.test/final".into())));
        history.finished(Some("https://one.test/final".into()));
        history.finished(Some("https://new.test/".into()));
        assert_eq!((history.depth(), history.max_depth()), (2, 2));
        assert_eq!(history.back(), Some(Some("https://one.test/final".into())));
        assert_eq!(history.back(), Some(None));
        assert_eq!(history.back(), None);
    }

    #[test]
    fn page_navigation_cannot_grow_history_without_bound() {
        let mut history = NavigationHistory::new("https://one.test/".into());
        history.finished(Some("https://one.test/".into()));
        for index in 0..1_000 {
            history.finished(Some(format!("https://example.test/{index}")));
        }
        assert_eq!(
            (history.depth(), history.max_depth()),
            (MAX_HISTORY_ENTRIES, MAX_HISTORY_ENTRIES)
        );
    }
}
