use std::process::Child;
use std::time::{Duration, Instant};

const GRACEFUL_EXIT_TIMEOUT: Duration = Duration::from_millis(750);
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Wait briefly after a compositor has received its shutdown command, then
/// retain forceful termination as a bounded failure fallback.
pub(crate) fn wait_for_exit_or_kill(child: &mut Child) {
    let deadline = Instant::now() + GRACEFUL_EXIT_TIMEOUT;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => std::thread::sleep(EXIT_POLL_INTERVAL),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub(crate) fn watchdog_should_restart(
    desired: bool,
    transitioning: bool,
    live_generation: u64,
) -> bool {
    desired && !transitioning && live_generation == 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn watchdog_never_restarts_an_intentional_lifecycle_transition() {
        assert!(!super::watchdog_should_restart(true, true, 0));
        assert!(!super::watchdog_should_restart(false, false, 0));
        assert!(!super::watchdog_should_restart(true, false, 7));
        assert!(super::watchdog_should_restart(true, false, 0));
    }
}
