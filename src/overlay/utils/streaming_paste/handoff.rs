//! Verify the captured input destination, not just its outer foreground window.
use super::{GENERATION, Shared, Target};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(super) fn ready(target: &Target, shared: &Shared, generation: u64, abort: &AtomicBool) -> bool {
    let start = Instant::now();
    settle(
        || {
            let state = shared.state.lock().unwrap();
            let active = !state.expired
                && !state.overflow
                && !abort.load(Ordering::SeqCst)
                && GENERATION.load(Ordering::SeqCst) == generation;
            drop(state);
            active && target.check().is_ok()
        },
        || start.elapsed(),
        || std::thread::sleep(Duration::from_millis(10)),
    )
}

fn settle(
    mut valid: impl FnMut() -> bool,
    mut elapsed: impl FnMut() -> Duration,
    mut wait: impl FnMut(),
) -> bool {
    loop {
        if !valid() {
            return false;
        }
        if elapsed() >= Duration::from_millis(250) {
            return true;
        }
        wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn transient_destination_never_receives_pending_speech() {
        let ms = Cell::new(0);
        assert!(!settle(
            || ms.get() < 130,
            || Duration::from_millis(ms.get()),
            || ms.set(ms.get() + 10)
        ));
        assert_eq!(ms.get(), 130);
    }

    #[test]
    fn stable_destination_can_resume_without_a_provider_event() {
        let ms = Cell::new(0);
        assert!(settle(
            || true,
            || Duration::from_millis(ms.get()),
            || ms.set(ms.get() + 10)
        ));
        assert_eq!(ms.get(), 250);
    }
}
