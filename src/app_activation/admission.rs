//! Serializes activation admission with automatic restart reservation.
//!
//! Every activation reaches the singleton owner through its pipe server, so
//! both sides of this lock live in the owner process and no named, shareable
//! kernel object is needed.

use std::io;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

const ADMISSION_TIMEOUT: Duration = Duration::from_secs(5);

static ADMISSION: Admission = Admission::new();

pub(crate) struct AdmissionGuard(&'static Admission);

pub(super) struct Admission {
    held: Mutex<bool>,
    released: Condvar,
}

impl Admission {
    pub(super) const fn new() -> Self {
        Self {
            held: Mutex::new(false),
            released: Condvar::new(),
        }
    }

    pub(super) fn acquire_with_timeout(
        &'static self,
        timeout: Duration,
    ) -> io::Result<AdmissionGuard> {
        let held = self.held.lock().unwrap_or_else(|error| error.into_inner());
        let (mut held, wait) = self
            .released
            .wait_timeout_while(held, timeout, |held| *held)
            .unwrap_or_else(|error| error.into_inner());
        if wait.timed_out() && *held {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "activation admission timed out",
            ));
        }
        *held = true;
        Ok(AdmissionGuard(self))
    }
}

pub(super) fn acquire() -> io::Result<AdmissionGuard> {
    ADMISSION.acquire_with_timeout(ADMISSION_TIMEOUT)
}

impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        *self
            .0
            .held
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = false;
        self.0.released.notify_one();
    }
}
