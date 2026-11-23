use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const STABLE_CONNECTION: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(20);

#[derive(Default)]
pub(in crate::overlay::computer_control::runtime) struct ConnectionHealth {
    connected_at: Option<Instant>,
}

impl ConnectionHealth {
    pub(in crate::overlay::computer_control::runtime) fn connected(&mut self, now: Instant) {
        self.connected_at = Some(now);
    }

    pub(in crate::overlay::computer_control::runtime) fn retire(
        &mut self,
        attempts: &mut u32,
        now: Instant,
    ) {
        if self
            .connected_at
            .take()
            .is_some_and(|start| now.saturating_duration_since(start) >= STABLE_CONNECTION)
        {
            *attempts = 0;
        }
    }
}

fn delay(attempt: u32) -> Duration {
    Duration::from_millis(250 * (1_u64 << attempt.saturating_sub(1).min(4)))
}

pub(in crate::overlay::computer_control::runtime) fn wait(attempt: u32, stop: &AtomicBool) -> bool {
    let deadline = Instant::now() + delay(attempt);
    loop {
        if stop.load(Ordering::SeqCst) {
            return false;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return true;
        }
        std::thread::sleep(remaining.min(POLL));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_connections_cannot_repeatedly_reset_attempts() {
        let start = Instant::now();
        let mut health = ConnectionHealth::default();
        let mut attempts = 0;
        for index in 0..10 {
            let connected = start + Duration::from_secs(index);
            health.connected(connected);
            health.retire(&mut attempts, connected + Duration::from_millis(999));
            attempts += 1;
        }
        assert_eq!(attempts, 10);
    }

    #[test]
    fn stable_connection_resets_once_and_failed_replacements_do_not_recredit_it() {
        let start = Instant::now();
        let mut health = ConnectionHealth::default();
        let mut attempts = 3;
        health.connected(start);
        health.retire(&mut attempts, start + STABLE_CONNECTION);
        assert_eq!(attempts, 0);
        attempts = 1;
        health.retire(&mut attempts, start + STABLE_CONNECTION * 2);
        assert_eq!(attempts, 1);
        health.connected(start);
        health.retire(
            &mut attempts,
            start + STABLE_CONNECTION - Duration::from_nanos(1),
        );
        assert_eq!(attempts, 1);
    }

    #[test]
    fn delays_grow_exponentially_to_a_fixed_cap() {
        assert_eq!(
            (1..=7)
                .map(|attempt| delay(attempt).as_millis())
                .collect::<Vec<_>>(),
            [250, 500, 1000, 2000, 4000, 4000, 4000]
        );
        assert_eq!(delay(u32::MAX), Duration::from_secs(4));
    }

    #[test]
    fn stop_interrupts_even_the_longest_backoff() {
        assert!(!wait(u32::MAX, &AtomicBool::new(true)));
        let stop = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| wait(u32::MAX, &stop));
            stop.store(true, Ordering::SeqCst);
            assert!(!worker.join().unwrap());
        });
    }
}
