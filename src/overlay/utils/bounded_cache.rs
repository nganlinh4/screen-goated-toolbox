//! A small insertion-ordered map that evicts its oldest entries past a fixed
//! capacity, for process-lifetime caches keyed by transient identities.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

pub(crate) struct BoundedCache<K, V> {
    capacity: usize,
    entries: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K: Clone + Eq + Hash, V> BoundedCache<K, V> {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.entries.get(key)
    }

    /// Inserts or replaces `key`, making it the newest entry.
    pub(crate) fn insert(&mut self, key: K, value: V) {
        if self.entries.insert(key.clone(), value).is_some() {
            self.order.retain(|existing| existing != &key);
        }
        self.order.push_back(key);
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::BoundedCache;

    #[test]
    fn oldest_entries_are_evicted_at_capacity() {
        let mut cache = BoundedCache::new(3);
        for key in 0..10 {
            cache.insert(key, key * 10);
        }
        assert_eq!(cache.len(), 3);
        assert!(cache.get(&6).is_none());
        assert_eq!(cache.get(&9), Some(&90));

        // Replacing an entry makes it the newest instead of duplicating it.
        cache.insert(7, 71);
        cache.insert(10, 100);
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get(&7), Some(&71));
        assert!(cache.get(&8).is_none());
    }
}
