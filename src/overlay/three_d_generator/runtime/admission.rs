use std::sync::MutexGuard;

use super::{RuntimeState, STATE};

static GATE: crate::overlay::creation_lifecycle::AdmissionGate =
    crate::overlay::creation_lifecycle::AdmissionGate::new();

/// Serializes admission/recovery without holding the worker state during IO.
pub(super) struct Admission {
    _guard: MutexGuard<'static, ()>,
    continuation: Option<String>,
}

pub(super) fn acquire() -> Admission {
    Admission {
        _guard: GATE.acquire(),
        continuation: None,
    }
}

pub(super) fn try_acquire() -> Option<Admission> {
    let guard = GATE.try_acquire()?;
    Some(Admission {
        _guard: guard,
        continuation: None,
    })
}

impl Admission {
    pub(super) fn reserve_continuation(&mut self, state: &mut RuntimeState, id: &str) {
        state.reserved_continuations.insert(id.to_string());
        self.continuation = Some(id.to_string());
    }

    pub(super) fn release_continuation(&mut self, state: &mut RuntimeState) {
        if let Some(id) = self.continuation.take() {
            state.reserved_continuations.remove(&id);
        }
    }
}

impl Drop for Admission {
    fn drop(&mut self) {
        if self.continuation.is_some() {
            self.release_continuation(&mut STATE.lock().unwrap_or_else(|error| error.into_inner()));
        }
    }
}

#[cfg(test)]
mod tests;
