use std::time::{Duration, Instant};

pub(crate) const PAGE_READY_TIMEOUT: Duration = Duration::from_secs(10);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct WheelLiveness {
    last_seen: Option<(u64, Instant)>,
}

impl WheelLiveness {
    pub(crate) const fn new() -> Self {
        Self { last_seen: None }
    }

    pub(crate) fn begin(&mut self, id: u64, now: Instant) {
        self.last_seen = Some((id, now));
    }

    pub(crate) fn refresh(&mut self, id: u64, now: Instant) {
        if let Some((owner, seen)) = self.last_seen.as_mut()
            && *owner == id
        {
            *seen = now;
        }
    }

    pub(crate) fn clear(&mut self) {
        self.last_seen = None;
    }

    pub(crate) fn is_alive(&self, id: u64, now: Instant) -> bool {
        self.last_seen.is_some_and(|(owner, seen)| {
            owner == id && now.saturating_duration_since(seen) < HEARTBEAT_TIMEOUT
        })
    }
}

pub(crate) struct WheelWaitState {
    pub result: i32,
    pub window_alive: bool,
    pub page_ready: bool,
    pub elapsed: Duration,
    pub heartbeat_alive: bool,
}

/// Waiting for a healthy user's choice has no deadline. Missing page progress does.
pub(crate) fn wait_for_selection(
    mut observe: impl FnMut() -> WheelWaitState,
    mut abort: impl FnMut(),
    mut pump: impl FnMut() -> bool,
    mut pause: impl FnMut(),
) -> Option<usize> {
    loop {
        let state = observe();
        if state.result != -1 {
            return (state.result >= 0).then_some(state.result as usize);
        }
        if !state.window_alive
            || (!state.page_ready && state.elapsed >= PAGE_READY_TIMEOUT)
            || (state.page_ready && !state.heartbeat_alive)
            || !pump()
        {
            abort();
            return None;
        }
        pause();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn stale_or_retired_heartbeat_cannot_keep_another_request_alive() {
        let start = Instant::now();
        let mut live = WheelLiveness::new();
        live.begin(7, start);
        live.refresh(6, start + HEARTBEAT_TIMEOUT);
        assert!(!live.is_alive(7, start + HEARTBEAT_TIMEOUT));
        live.begin(8, start + HEARTBEAT_TIMEOUT);
        live.refresh(7, start + HEARTBEAT_TIMEOUT * 2);
        assert!(!live.is_alive(8, start + HEARTBEAT_TIMEOUT * 2));
        live.clear();
        live.refresh(8, start + HEARTBEAT_TIMEOUT * 2);
        assert!(!live.is_alive(8, start + HEARTBEAT_TIMEOUT * 2));
    }

    #[test]
    fn shown_renderer_without_heartbeat_releases_the_actual_wait_driver() {
        let start = Instant::now();
        let mut live = WheelLiveness::new();
        live.begin(7, start);
        let polls = Cell::new(0);
        let aborts = Cell::new(0);
        let result = wait_for_selection(
            || {
                let elapsed = HEARTBEAT_TIMEOUT * polls.get();
                polls.set(polls.get() + 1);
                WheelWaitState {
                    result: -1,
                    window_alive: true,
                    page_ready: true,
                    elapsed,
                    heartbeat_alive: live.is_alive(7, start + elapsed),
                }
            },
            || aborts.set(aborts.get() + 1),
            || true,
            || {},
        );
        assert_eq!(result, None);
        assert_eq!(polls.get(), 2);
        assert_eq!(aborts.get(), 1);
    }

    #[test]
    fn healthy_idle_choice_remains_unlimited_and_then_returns_the_selection() {
        let start = Instant::now();
        let mut live = WheelLiveness::new();
        live.begin(7, start);
        let mut polls = 0;
        let result = wait_for_selection(
            || {
                let elapsed = Duration::from_secs(24 * 60 * 60) * polls;
                polls += 1;
                live.refresh(7, start + elapsed);
                WheelWaitState {
                    result: if polls == 3 { 4 } else { -1 },
                    window_alive: true,
                    page_ready: true,
                    elapsed,
                    heartbeat_alive: live.is_alive(7, start + elapsed),
                }
            },
            || panic!("a healthy choice must not be aborted"),
            || true,
            || {},
        );
        assert_eq!(result, Some(4));
        assert_eq!(polls, 3);
    }

    #[test]
    fn missing_ready_dead_window_and_caller_quit_each_abort_once() {
        for (window_alive, page_ready, elapsed, pump) in [
            (true, false, PAGE_READY_TIMEOUT, true),
            (false, true, Duration::ZERO, true),
            (true, true, Duration::ZERO, false),
        ] {
            let aborts = Cell::new(0);
            assert_eq!(
                wait_for_selection(
                    || WheelWaitState {
                        result: -1,
                        window_alive,
                        page_ready,
                        elapsed,
                        heartbeat_alive: true,
                    },
                    || aborts.set(aborts.get() + 1),
                    || pump,
                    || panic!("aborted waits cannot sleep"),
                ),
                None
            );
            assert_eq!(aborts.get(), 1);
        }
    }
}
