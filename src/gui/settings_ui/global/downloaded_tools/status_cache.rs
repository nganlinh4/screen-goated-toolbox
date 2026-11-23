use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RETRY_DELAY: Duration = Duration::from_secs(2);

struct State<T> {
    generation: u64,
    running: bool,
    value: Option<T>,
    expires: Instant,
}

/// One in-flight probe per cache, including after invalidation. A stalled probe
/// therefore cannot create an unbounded number of replacement threads.
pub(super) struct StatusCache<T> {
    state: Mutex<State<T>>,
}

impl<T: Clone + Send + 'static> StatusCache<T> {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                generation: 0,
                running: false,
                value: None,
                expires: Instant::now(),
            }),
        })
    }

    pub(super) fn invalidate(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.generation = state.generation.wrapping_add(1);
        state.value = None;
        state.expires = Instant::now();
    }

    pub(super) fn get(
        self: &Arc<Self>,
        ttl: Duration,
        probe: impl FnOnce() -> T + Send + 'static,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Option<T> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if Instant::now() < state.expires {
            return state.value.clone();
        }
        state.value = None;
        if state.running {
            return None;
        }
        state.running = true;
        let generation = state.generation;
        drop(state);
        let guard = ProbeGuard {
            cache: self.clone(),
            generation,
            wake: Box::new(wake),
        };
        let _ = std::thread::Builder::new()
            .name("sgt-tool-status".into())
            .spawn(move || {
                let result = probe();
                let mut state = guard.cache.state.lock().unwrap_or_else(|e| e.into_inner());
                if state.generation == guard.generation {
                    state.value = Some(result);
                    state.expires = Instant::now() + ttl;
                }
            });
        None
    }
}

struct ProbeGuard<T> {
    cache: Arc<StatusCache<T>>,
    generation: u64,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl<T> Drop for ProbeGuard<T> {
    fn drop(&mut self) {
        let mut state = self.cache.state.lock().unwrap_or_else(|e| e.into_inner());
        state.running = false;
        if state.generation == self.generation && state.value.is_none() {
            state.expires = Instant::now() + RETRY_DELAY;
        }
        drop(state);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.wake)()));
    }
}

#[cfg(test)]
#[path = "status_cache_tests.rs"]
mod tests;
