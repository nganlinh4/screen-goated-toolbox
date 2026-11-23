//! Process-local progress for independently owned installs.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, LazyLock, Mutex, mpsc};

type Repaint = Arc<dyn Fn() + Send + Sync>;
static REGISTRY: LazyLock<Arc<Registry>> = LazyLock::new(|| Arc::new(Registry::default()));
static REPAINT: Mutex<Option<Repaint>> = Mutex::new(None);
static REPAINT_WAKE: LazyLock<Option<mpsc::SyncSender<()>>> = LazyLock::new(|| {
    let (sender, receiver) = mpsc::sync_channel::<()>(1);
    std::thread::Builder::new()
        .name("install-progress-repaint".into())
        .spawn(move || {
            while receiver.recv().is_ok() {
                let callback = REPAINT
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .clone();
                if let Some(callback) = callback {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback()));
                }
            }
        })
        .ok()?;
    Some(sender)
});

#[derive(Default)]
struct Registry {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    next: u64,
    changed: u64,
    entries: HashMap<Arc<str>, BTreeMap<u64, Entry>>,
}

#[derive(Default)]
struct Entry {
    measured: Option<(u64, f32)>,
}

pub(crate) struct Guard {
    registry: Arc<Registry>,
    key: Arc<str>,
    generation: u64,
}

pub(crate) fn begin(key: &'static str) -> Guard {
    REGISTRY
        .start(key.into(), false)
        .expect("unconditional start")
}

pub(crate) fn try_begin(key: &'static str) -> Option<Guard> {
    REGISTRY.start(key.into(), true)
}

pub(crate) fn snapshot(key: &str) -> Option<f32> {
    REGISTRY.snapshot(key)
}

pub(crate) fn begin_external(key: &str) -> Guard {
    REGISTRY
        .start(key.into(), false)
        .expect("unconditional start")
}

pub(crate) fn set_repaint_callback(callback: Option<Repaint>) {
    *REPAINT.lock().unwrap_or_else(|error| error.into_inner()) = callback;
}

fn repaint() {
    if let Some(sender) = REPAINT_WAKE.as_ref() {
        let _ = sender.try_send(());
    }
}

impl Registry {
    fn start(self: &Arc<Self>, key: Arc<str>, exclusive: bool) -> Option<Guard> {
        let generation = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if exclusive && state.entries.contains_key(key.as_ref()) {
                return None;
            }
            state.next = state
                .next
                .checked_add(1)
                .expect("install generation exhausted");
            let generation = state.next;
            state
                .entries
                .entry(key.clone())
                .or_default()
                .insert(generation, Entry::default());
            generation
        };
        repaint();
        Some(Guard {
            registry: self.clone(),
            key,
            generation,
        })
    }

    fn snapshot(&self, key: &str) -> Option<f32> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.entries.get(key).map(|entries| {
            entries
                .values()
                .filter_map(|entry| entry.measured)
                .max_by_key(|(changed, _)| *changed)
                .map_or(-1.0, |(_, percent)| percent)
        })
    }

    fn report(&self, key: &str, generation: u64, percent: f32) {
        if !percent.is_finite() {
            return;
        }
        let changed = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if !state
                .entries
                .get(key)
                .is_some_and(|entries| entries.contains_key(&generation))
            {
                return;
            }
            state.changed = state
                .changed
                .checked_add(1)
                .expect("install progress sequence exhausted");
            let changed = state.changed;
            state
                .entries
                .get_mut(key)
                .and_then(|entries| entries.get_mut(&generation))
                .expect("active generation")
                .measured = Some((changed, percent.clamp(0.0, 100.0)));
            true
        };
        if changed {
            repaint();
        }
    }

    fn finish(&self, key: &str, generation: u64) {
        let removed = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(entries) = state.entries.get_mut(key) else {
                return;
            };
            let removed = entries.remove(&generation).is_some();
            if entries.is_empty() {
                state.entries.remove(key);
            }
            removed
        };
        if removed {
            repaint();
        }
    }
}

impl Guard {
    pub(crate) fn report(&self, percent: f32) {
        self.registry.report(&self.key, self.generation, percent);
    }

    pub(crate) fn clear_measurement(&self) {
        let changed = {
            let mut state = self
                .registry
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state
                .entries
                .get_mut(self.key.as_ref())
                .and_then(|entries| entries.get_mut(&self.generation))
                .is_some_and(|entry| entry.measured.take().is_some())
        };
        if changed {
            repaint();
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.registry.finish(&self.key, self.generation);
    }
}

#[cfg(test)]
mod tests;
