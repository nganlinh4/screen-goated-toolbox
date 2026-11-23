use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PAGE_MAX_COUNT: usize = 128;
const PAGE_MAX_BYTES: usize = 32 * 1024 * 1024;
const PAGE_MAX_IDLE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

struct PageEntry {
    html: Arc<str>,
    last_access: Instant,
}

#[derive(Default)]
pub(super) struct PageStore {
    pages: HashMap<String, PageEntry>,
    total_bytes: usize,
}

impl PageStore {
    pub(super) fn insert(&mut self, id: String, html: String, now: Instant) -> bool {
        self.prune_expired(now);
        let bytes = html.len();
        if bytes > PAGE_MAX_BYTES {
            return false;
        }
        if let Some(existing) = self.pages.get_mut(&id) {
            existing.last_access = now;
            return true;
        }
        while self.pages.len() >= PAGE_MAX_COUNT
            || self.total_bytes.saturating_add(bytes) > PAGE_MAX_BYTES
        {
            if !self.remove_oldest() {
                return false;
            }
        }
        self.total_bytes += bytes;
        self.pages.insert(
            id,
            PageEntry {
                html: Arc::from(html),
                last_access: now,
            },
        );
        true
    }

    pub(super) fn get(&mut self, id: &str, now: Instant) -> Option<Arc<str>> {
        self.prune_expired(now);
        let entry = self.pages.get_mut(id)?;
        entry.last_access = now;
        Some(Arc::clone(&entry.html))
    }

    fn prune_expired(&mut self, now: Instant) {
        let expired = self
            .pages
            .iter()
            .filter_map(|(id, entry)| {
                (now.saturating_duration_since(entry.last_access) > PAGE_MAX_IDLE)
                    .then_some(id.clone())
            })
            .collect::<Vec<_>>();
        for id in expired {
            self.remove(&id);
        }
    }

    fn remove_oldest(&mut self) -> bool {
        let Some(id) = self
            .pages
            .iter()
            .min_by_key(|(_, entry)| entry.last_access)
            .map(|(id, _)| id.clone())
        else {
            return false;
        };
        self.remove(&id);
        true
    }

    fn remove(&mut self, id: &str) {
        if let Some(entry) = self.pages.remove(id) {
            self.total_bytes = self.total_bytes.saturating_sub(entry.html.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_store_has_a_hard_entry_bound() {
        let mut store = PageStore::default();
        let now = Instant::now();
        for index in 0..PAGE_MAX_COUNT * 2 {
            assert!(store.insert(index.to_string(), "page".to_string(), now));
        }
        assert_eq!(store.pages.len(), PAGE_MAX_COUNT);
        assert!(store.total_bytes <= PAGE_MAX_BYTES);
    }
}
