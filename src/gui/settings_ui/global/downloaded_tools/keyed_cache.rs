//! Per-key values computed on a background thread for downloaded-tool cards.
//!
//! A card reads the last known value immediately and a stale entry is
//! recomputed off the UI thread. Invalidating one key marks only that entry
//! stale and keeps its value on screen until the new one arrives, so removing
//! one tool never flashes every other card to "Missing" or "0 MB".

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Shared across caches so a removed-then-recreated entry never matches the
/// generation of a computation started for its predecessor.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

fn next_generation() -> u64 {
    NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
}

struct Entry<V> {
    value: V,
    updated_at: Option<Instant>,
    calculating: bool,
    generation: u64,
}

pub(super) struct KeyedCache<K, V> {
    entries: LazyLock<Mutex<HashMap<K, Entry<V>>>>,
    ttl: Duration,
    thread_name: &'static str,
}

/// Runs its closure when dropped unless disarmed; releases a reservation if
/// the computation panics or its thread cannot start.
pub(super) struct OnDrop<F: FnOnce()>(pub(super) Option<F>);

impl<F: FnOnce()> Drop for OnDrop<F> {
    fn drop(&mut self) {
        if let Some(callback) = self.0.take() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback));
        }
    }
}

impl<K, V> KeyedCache<K, V>
where
    K: Eq + Hash + Clone + Send + 'static,
    V: Copy + Default + Send + 'static,
{
    pub(super) const fn new(ttl: Duration, thread_name: &'static str) -> Self {
        Self {
            entries: LazyLock::new(|| Mutex::new(HashMap::new())),
            ttl,
            thread_name,
        }
    }

    /// The last known value; starts a background refresh when it is stale.
    pub(super) fn get(&'static self, key: K, compute: impl FnOnce() -> V + Send + 'static) -> V {
        let now = Instant::now();
        let (generation, last_value) = {
            let Ok(mut entries) = self.entries.lock() else {
                return V::default();
            };
            let entry = entries.entry(key.clone()).or_insert_with(|| Entry {
                value: V::default(),
                updated_at: None,
                calculating: false,
                generation: next_generation(),
            });
            let fresh = entry
                .updated_at
                .is_some_and(|updated_at| now.duration_since(updated_at) < self.ttl);
            if fresh || entry.calculating {
                return entry.value;
            }
            entry.calculating = true;
            (entry.generation, entry.value)
        };

        let guard_key = key.clone();
        let guard = OnDrop(Some(move || {
            self.finish(&guard_key, generation, None);
        }));
        let spawned = std::thread::Builder::new()
            .name(self.thread_name.into())
            .spawn(move || {
                let mut guard = guard;
                let value = compute();
                guard.0.take();
                self.finish(&key, generation, Some(value));
            });
        if let Err(error) = spawned {
            crate::log_info!(
                "[DownloadedTools] Could not start {}: {error}",
                self.thread_name
            );
        }
        last_value
    }

    /// Records a computed value (or releases a failed reservation) only for
    /// the entry generation that started it.
    fn finish(&self, key: &K, generation: u64, value: Option<V>) {
        if let Ok(mut entries) = self.entries.lock()
            && let Some(entry) = entries.get_mut(key)
            && entry.generation == generation
            && entry.calculating
        {
            entry.calculating = false;
            entry.updated_at = Some(Instant::now());
            if let Some(value) = value {
                entry.value = value;
            }
        }
        super::utils::request_gui_repaint();
    }

    /// Marks one entry stale, keeping its value until the refresh lands.
    pub(super) fn invalidate(&self, key: &K) {
        if let Ok(mut entries) = self.entries.lock()
            && let Some(entry) = entries.get_mut(key)
        {
            entry.updated_at = None;
            entry.calculating = false;
            entry.generation = next_generation();
        }
        super::utils::request_gui_repaint();
    }

    /// Forgets every entry (after Clean All, nothing cached is still true).
    pub(super) fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }

    #[cfg(test)]
    pub(super) fn forget(&self, key: &K) {
        self.entries.lock().unwrap().remove(key);
    }

    #[cfg(test)]
    pub(super) fn is_calculating(&self, key: &K) -> bool {
        self.entries
            .lock()
            .unwrap()
            .get(key)
            .is_some_and(|entry| entry.calculating)
    }
}
