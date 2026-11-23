//! Holds activation delivery until the owning process has finished the startup
//! that file handling depends on (COM and DPI awareness, TTS, the hotkey
//! listener), so a queued file never reaches a half-initialised application.

use std::sync::{Condvar, Mutex};

pub(crate) struct ReadyGate {
    open: Mutex<bool>,
    opened: Condvar,
}

impl ReadyGate {
    pub(crate) const fn new() -> Self {
        Self {
            open: Mutex::new(false),
            opened: Condvar::new(),
        }
    }

    pub(crate) fn open(&self) {
        *self.open.lock().unwrap_or_else(|error| error.into_inner()) = true;
        self.opened.notify_all();
    }

    pub(crate) fn wait(&self) {
        let mut open = self.open.lock().unwrap_or_else(|error| error.into_inner());
        while !*open {
            open = self
                .opened
                .wait(open)
                .unwrap_or_else(|error| error.into_inner());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ReadyGate;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[test]
    fn queued_activations_wait_until_startup_opens_the_gate() {
        let gate = ReadyGate::new();
        let delivered = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let listener = scope.spawn(|| {
                super::super::deliver_when_ready(&gate, || {
                    delivered.store(true, Ordering::SeqCst);
                    true
                })
            });
            std::thread::sleep(Duration::from_millis(150));
            assert!(!delivered.load(Ordering::SeqCst));
            gate.open();
            assert!(listener.join().unwrap());
        });
        assert!(delivered.load(Ordering::SeqCst));
    }
}
