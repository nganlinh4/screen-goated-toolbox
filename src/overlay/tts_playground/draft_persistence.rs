//! Coalesces draft edits while keeping close and application exit durable.

use std::io;
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_millis(800);
const RETRY: Duration = Duration::from_secs(2);

static SAVER: LazyLock<DraftSaver> =
    LazyLock::new(|| DraftSaver::new(DEBOUNCE, crate::config::try_save_current_config));

#[derive(Default)]
struct State {
    generation: u64,
    saved: u64,
    deadline: Option<Instant>,
    failure: Option<(u64, String)>,
    frozen: bool,
    stopping: bool,
}

struct DraftSaver {
    shared: Arc<(Mutex<State>, Condvar)>,
    debounce: Duration,
}

impl DraftSaver {
    fn new(debounce: Duration, save: impl Fn() -> io::Result<()> + Send + 'static) -> Self {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = Arc::clone(&shared);
        std::thread::spawn(move || run(worker, save));
        Self { shared, debounce }
    }

    fn update(&self, apply: impl FnOnce() -> Result<bool, String>) -> Result<(), String> {
        let (mutex, changed) = &*self.shared;
        let mut state = mutex.lock().unwrap_or_else(|error| error.into_inner());
        if state.frozen {
            return Err("TTS draft is closing".to_string());
        }
        if apply()? {
            state.generation = state
                .generation
                .checked_add(1)
                .ok_or("draft revision exhausted")?;
            state.deadline = Some(Instant::now() + self.debounce);
            state.failure = None;
            changed.notify_one();
        }
        Ok(())
    }

    fn flush(&self, freeze: bool) -> io::Result<()> {
        let (mutex, changed) = &*self.shared;
        let mut state = mutex.lock().unwrap_or_else(|error| error.into_inner());
        if freeze {
            state.frozen = true;
        }
        let target = state.generation;
        if state.saved < target {
            state.failure = None;
            state.deadline = Some(Instant::now());
            changed.notify_one();
        }
        while state.saved < target {
            if let Some((revision, error)) = &state.failure
                && *revision >= target
            {
                let error = io::Error::other(error.clone());
                if freeze {
                    state.frozen = false;
                }
                return Err(error);
            }
            state = changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
        Ok(())
    }

    fn thaw(&self) {
        let (mutex, _) = &*self.shared;
        mutex
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .frozen = false;
    }
}

impl Drop for DraftSaver {
    fn drop(&mut self) {
        let (mutex, changed) = &*self.shared;
        mutex
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .stopping = true;
        changed.notify_one();
    }
}

fn run(shared: Arc<(Mutex<State>, Condvar)>, save: impl Fn() -> io::Result<()>) {
    let (mutex, changed) = &*shared;
    let mut state = mutex.lock().unwrap_or_else(|error| error.into_inner());
    loop {
        if state.stopping {
            return;
        }
        let Some(deadline) = state.deadline else {
            state = changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
            continue;
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            state = changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner())
                .0;
            continue;
        }
        let revision = state.generation;
        state.deadline = None;
        drop(state);
        let result = save();
        state = mutex.lock().unwrap_or_else(|error| error.into_inner());
        match result {
            Ok(()) => {
                state.saved = state.saved.max(revision);
                state.failure = None;
            }
            Err(error) => {
                crate::log_info!("[TTS Playground] Draft save failed: {error}");
                state.failure = Some((revision, error.to_string()));
                let retry_at = Instant::now() + RETRY;
                state.deadline = Some(state.deadline.map_or(retry_at, |at| at.min(retry_at)));
            }
        }
        changed.notify_all();
    }
}

pub(super) fn set_draft(value: &str) -> Result<(), String> {
    SAVER.update(|| {
        let mut app = crate::APP
            .lock()
            .map_err(|_| "application state unavailable")?;
        if app.config.tts_playground.draft_text == value {
            return Ok(false);
        }
        app.config.tts_playground.draft_text = value.to_owned();
        Ok(true)
    })
}

pub(super) fn flush() -> io::Result<()> {
    SAVER.flush(false)
}

pub(super) fn freeze_and_flush() -> io::Result<()> {
    SAVER.flush(true)
}

pub(super) fn thaw_after_aborted_exit() {
    SAVER.thaw();
}

#[cfg(test)]
mod tests;
