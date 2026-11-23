use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use anyhow::{Result, bail};

struct State<T> {
    generation: Arc<()>,
    idle: Option<T>,
    active: Option<Arc<AtomicBool>>,
}

pub(super) struct ClientSlot<T> {
    state: Mutex<State<T>>,
    ready: Condvar,
}

pub(super) struct ClientLease<'a, T> {
    slot: &'a ClientSlot<T>,
    generation: Arc<()>,
    pub(super) value: Option<T>,
    reusable: bool,
}

impl<T: Send + 'static> ClientSlot<T> {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(State {
                generation: Arc::new(()),
                idle: None,
                active: None,
            }),
            ready: Condvar::new(),
        }
    }

    pub(super) fn begin_prepare(&self, cancelled: Arc<AtomicBool>) -> Option<ClientLease<'_, T>> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if cancelled.load(Ordering::Acquire) || state.idle.is_some() || state.active.is_some() {
            return None;
        }
        state.active = Some(cancelled);
        Some(ClientLease {
            slot: self,
            generation: Arc::clone(&state.generation),
            value: None,
            reusable: false,
        })
    }

    pub(super) fn acquire(&self, cancelled: Arc<AtomicBool>) -> Result<ClientLease<'_, T>> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let generation = Arc::clone(&state.generation);
        loop {
            if cancelled.load(Ordering::Acquire) || !Arc::ptr_eq(&generation, &state.generation) {
                bail!("detector operation cancelled before admission");
            }
            if state.active.is_none() {
                state.active = Some(cancelled);
                let value = state.idle.take();
                return Ok(ClientLease {
                    slot: self,
                    generation,
                    reusable: false,
                    value,
                });
            }
            state = self
                .ready
                .wait_timeout(state, Duration::from_millis(20))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    pub(super) fn stop(&self) {
        let idle = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.generation = Arc::new(());
            if let Some(cancelled) = &state.active {
                cancelled.store(true, Ordering::Release);
            }
            state.idle.take()
        };
        self.ready.notify_all();
        if let Some(client) = idle {
            // Resource leases remain held until process teardown completes.
            std::thread::spawn(move || drop(client));
        }
    }
}

impl<T> ClientLease<'_, T> {
    pub(super) fn retain(&mut self) {
        self.reusable = true;
    }
}

struct ReleaseActive<'a, T>(&'a ClientSlot<T>);

impl<T> Drop for ReleaseActive<'_, T> {
    fn drop(&mut self) {
        self.0
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active = None;
        self.0.ready.notify_all();
    }
}

impl<T> Drop for ClientLease<'_, T> {
    fn drop(&mut self) {
        let mut state = self.slot.state.lock().unwrap_or_else(|e| e.into_inner());
        if self.reusable && Arc::ptr_eq(&state.generation, &self.generation) {
            state.idle = self.value.take();
            state.active = None;
            drop(state);
            self.slot.ready.notify_all();
        } else {
            drop(state);
            let _release = ReleaseActive(self.slot);
            // Never wait for a worker or release component leases under the slot mutex.
            drop(self.value.take());
        }
    }
}

#[cfg(test)]
#[path = "client_slot_tests.rs"]
mod tests;
