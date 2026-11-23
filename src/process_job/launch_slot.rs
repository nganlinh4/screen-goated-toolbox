use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, ensure};

/// A capability's replacement cannot reuse its state while a retired process
/// tree still owns the permit. Retain it inside OwnedProcess resources.
pub(crate) struct LaunchSlot(AtomicBool);

pub(crate) struct Permit(&'static LaunchSlot);

impl LaunchSlot {
    pub(crate) const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    pub(crate) fn acquire(&'static self) -> Result<Permit> {
        ensure!(
            self.0
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok(),
            "previous process tree has not retired"
        );
        Ok(Permit(self))
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.0.store(false, Ordering::Release);
    }
}
