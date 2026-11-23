use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use super::{LiveEvent, LiveResponse};

pub(super) const MAX_CANCELLATION_POLL: Duration = Duration::from_millis(50);

pub(super) struct WaitOptions {
    pub(super) cancel_token: Option<Arc<AtomicBool>>,
    pub(super) deadline: Option<Instant>,
    pub(super) request_timeout: Option<Duration>,
    pub(super) first_output_received: Arc<AtomicBool>,
}

impl WaitOptions {
    fn remaining(&self, now: Instant) -> Option<Duration> {
        if self.first_output_received.load(Ordering::SeqCst) {
            return None;
        }
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(now))
    }

    pub(super) fn check(&self, retired: bool, now: Instant) -> anyhow::Result<()> {
        if self
            .cancel_token
            .as_ref()
            .is_some_and(|token| token.load(Ordering::SeqCst))
        {
            anyhow::bail!("Gemini Live request cancelled");
        }
        if self
            .remaining(now)
            .is_some_and(|remaining| remaining.is_zero())
        {
            anyhow::bail!(
                "Gemini Live request timed out after {} ms",
                self.request_timeout.unwrap_or_default().as_millis()
            );
        }
        anyhow::ensure!(!retired, "Gemini Live request retired");
        Ok(())
    }
}

pub(super) fn receive_live_event(
    response: &LiveResponse,
    options: &WaitOptions,
) -> anyhow::Result<Option<LiveEvent>> {
    receive_live_event_with(
        |wait| response.receiver.recv_timeout(wait),
        options,
        || response.is_retired(),
        Instant::now,
    )
}

fn receive_live_event_with(
    mut receive: impl FnMut(Duration) -> Result<LiveEvent, mpsc::RecvTimeoutError>,
    options: &WaitOptions,
    retired: impl Fn() -> bool,
    now: impl Fn() -> Instant,
) -> anyhow::Result<Option<LiveEvent>> {
    loop {
        options.check(retired(), now())?;
        let wait = options
            .remaining(now())
            .unwrap_or(MAX_CANCELLATION_POLL)
            .min(MAX_CANCELLATION_POLL);
        let event = receive(wait);
        options.check(retired(), now())?;
        match event {
            Ok(event) => return Ok(Some(event)),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(
        cancel: Option<Arc<AtomicBool>>,
        deadline: Option<Instant>,
        timeout: Option<Duration>,
    ) -> WaitOptions {
        WaitOptions {
            cancel_token: cancel,
            deadline,
            request_timeout: timeout,
            first_output_received: Arc::new(AtomicBool::new(false)),
        }
    }

    fn contract() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../parity-fixtures/preset-system/live-response-wait.json"
        ))
        .unwrap()
    }

    #[test]
    fn live_wait_shared_rechecks_cancellation_after_receive() {
        let fixture = contract();
        let cancel = Arc::new(AtomicBool::new(false));
        let result = receive_live_event_with(
            |wait| {
                assert!(
                    wait.as_millis()
                        <= fixture["maximumCancellationPollMillis"].as_u64().unwrap() as u128
                );
                cancel.store(true, Ordering::SeqCst);
                Ok(LiveEvent::TextChunk("late".to_string()))
            },
            &options(Some(cancel.clone()), None, None),
            || false,
            Instant::now,
        );
        assert!(
            result.is_err(),
            "retired output escaped the blocking receive: {result:?}"
        );
    }

    #[test]
    fn live_wait_shared_rechecks_deadline_after_receive() {
        let fixture = contract();
        let timeout = Duration::from_millis(fixture["firstOutputTimeoutMillis"].as_u64().unwrap());
        let start = Instant::now();
        let clock = std::cell::Cell::new(start);
        let result = receive_live_event_with(
            |_| {
                clock.set(start + timeout);
                Ok(LiveEvent::TextChunk("late".to_string()))
            },
            &options(None, Some(start + timeout), Some(timeout)),
            || false,
            || clock.get(),
        );
        assert!(
            result.is_err(),
            "expired output escaped the blocking receive: {result:?}"
        );
    }

    #[test]
    fn first_output_during_receive_releases_the_initial_deadline() {
        let timeout = Duration::from_millis(1);
        let start = Instant::now();
        let clock = std::cell::Cell::new(start);
        let options = options(None, Some(start + timeout), Some(timeout));
        let result = receive_live_event_with(
            |_| {
                clock.set(start + timeout);
                options.first_output_received.store(true, Ordering::SeqCst);
                Ok(LiveEvent::TextChunk("accepted".to_string()))
            },
            &options,
            || false,
            || clock.get(),
        )
        .unwrap();
        assert!(matches!(result,Some(LiveEvent::TextChunk(text)) if text=="accepted"));
    }

    #[test]
    fn receive_boundary_honors_deadline_without_an_event() {
        let (_tx, rx) = mpsc::channel();
        let timeout = Duration::from_millis(25);
        let started = Instant::now();
        let result = receive_live_event_with(
            |wait| rx.recv_timeout(wait),
            &options(None, Some(started + timeout), Some(timeout)),
            || false,
            Instant::now,
        );
        assert!(result.unwrap_err().to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn receive_boundary_honors_per_request_cancellation() {
        let cancel = Arc::new(AtomicBool::new(true));
        let result = receive_live_event_with(
            |_| panic!("cancelled wait must not receive"),
            &options(Some(cancel), None, None),
            || false,
            Instant::now,
        );
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
}
