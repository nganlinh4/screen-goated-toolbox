//! Bound telemetry for connections that end the same way over and over, such as
//! a stale extension retrying a rejected pairing every few seconds.
use std::time::{Duration, Instant};

const REPEAT_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Default)]
pub(super) struct RepeatThrottle {
    last: Option<(String, Instant)>,
    suppressed: u64,
}

impl RepeatThrottle {
    /// `Some(n)` when `message` should be logged now, where `n` identical
    /// endings were suppressed since it was last logged; `None` to suppress.
    pub(super) fn admit(&mut self, message: &str, now: Instant) -> Option<u64> {
        if let Some((last, at)) = &self.last
            && last == message
            && now.saturating_duration_since(*at) < REPEAT_INTERVAL
        {
            self.suppressed += 1;
            return None;
        }
        let repeated = self.last.as_ref().is_some_and(|(last, _)| last == message);
        let suppressed = if repeated { self.suppressed } else { 0 };
        self.last = Some((message.to_string(), now));
        self.suppressed = 0;
        Some(suppressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_endings_log_once_per_interval_with_a_suppressed_count() {
        let start = Instant::now();
        let mut throttle = RepeatThrottle::default();
        assert_eq!(throttle.admit("rejected", start), Some(0));
        for second in 1..=30 {
            let at = start + Duration::from_secs(second * 2 - 1);
            assert_eq!(throttle.admit("rejected", at), None);
        }
        assert_eq!(
            throttle.admit("rejected", start + REPEAT_INTERVAL),
            Some(30)
        );
        assert_eq!(
            throttle.admit("closed", start + REPEAT_INTERVAL),
            Some(0),
            "a different ending is always logged"
        );
        assert_eq!(
            throttle.admit("rejected", start + REPEAT_INTERVAL),
            Some(0),
            "suppression restarts when the ending changes"
        );
    }
}
