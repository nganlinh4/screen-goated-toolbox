use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

struct State {
    next_id: u64,
    quiescing: bool,
    active: HashMap<u64, Arc<AtomicBool>>,
}

static ACTIVITY: LazyLock<(Mutex<State>, Condvar)> = LazyLock::new(|| {
    (
        Mutex::new(State {
            next_id: 1,
            quiescing: false,
            active: HashMap::new(),
        }),
        Condvar::new(),
    )
});

/// Registers a cancellable install before its worker is spawned.
///
/// Registration fails while Clean All owns the quiescence guard, preventing a
/// new installer from racing deletion after cancellation has been broadcast.
pub(crate) fn register(cancel: Arc<AtomicBool>) -> Result<InstallActivityGuard> {
    let (state, _) = &*ACTIVITY;
    let mut state = state.lock().unwrap_or_else(|value| value.into_inner());
    if state.quiescing {
        cancel.store(true, Ordering::Release);
        bail!("downloaded-tool installation is stopping for cleanup");
    }
    let id = state.next_id;
    state.next_id = state.next_id.wrapping_add(1).max(1);
    state.active.insert(id, cancel);
    Ok(InstallActivityGuard { id })
}

pub(crate) fn begin_quiescence(timeout: Duration) -> Result<InstallQuiescenceGuard> {
    let (state, settled) = &*ACTIVITY;
    let mut state = state.lock().unwrap_or_else(|value| value.into_inner());
    if state.quiescing {
        bail!("downloaded-tool cleanup is already stopping installers");
    }
    state.quiescing = true;
    for cancel in state.active.values() {
        cancel.store(true, Ordering::Release);
    }

    let deadline = Instant::now() + timeout;
    while !state.active.is_empty() {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            state.quiescing = false;
            settled.notify_all();
            bail!(
                "{} downloaded-tool installer(s) did not stop before cleanup timed out",
                state.active.len()
            );
        };
        let waited = settled
            .wait_timeout(state, remaining)
            .unwrap_or_else(|value| value.into_inner());
        state = waited.0;
    }
    Ok(InstallQuiescenceGuard)
}

pub(crate) struct InstallActivityGuard {
    id: u64,
}

impl Drop for InstallActivityGuard {
    fn drop(&mut self) {
        let (state, settled) = &*ACTIVITY;
        let mut state = state.lock().unwrap_or_else(|value| value.into_inner());
        state.active.remove(&self.id);
        settled.notify_all();
    }
}

pub(crate) struct InstallQuiescenceGuard;

impl Drop for InstallQuiescenceGuard {
    fn drop(&mut self) {
        let (state, settled) = &*ACTIVITY;
        let mut state = state.lock().unwrap_or_else(|value| value.into_inner());
        state.quiescing = false;
        settled.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiescence_cancels_active_work_and_rejects_racing_registration() {
        let cancel = Arc::new(AtomicBool::new(false));
        let activity = register(cancel.clone()).unwrap();
        let worker = std::thread::spawn(move || {
            while !cancel.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            drop(activity);
        });

        let quiescence = begin_quiescence(Duration::from_secs(1)).unwrap();
        assert!(register(Arc::new(AtomicBool::new(false))).is_err());
        drop(quiescence);
        worker.join().unwrap();
        assert!(register(Arc::new(AtomicBool::new(false))).is_ok());
    }
}
