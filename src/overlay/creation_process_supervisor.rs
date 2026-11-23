use crate::overlay::store_support::now_ms;
use std::io::Read;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::Value;

mod exit;

const POLL_INTERVAL: Duration = Duration::from_millis(250);
pub(crate) const MAX_WALL_TIME_MS: u64 = 2 * 60 * 60 * 1_000;
#[cfg(test)]
const MAX_WALL_TIME: Duration = Duration::from_millis(MAX_WALL_TIME_MS);
const MAX_SILENCE: Duration = Duration::from_secs(10 * 60);

pub struct EventSupervisor {
    receiver: Receiver<Result<Option<Value>, String>>,
    started_at: Instant,
    last_event_at: Instant,
    wall_time: Duration,
}

impl EventSupervisor {
    #[cfg(test)]
    pub fn new(reader: impl Read + Send + 'static) -> Self {
        Self::with_wall_time(reader, MAX_WALL_TIME)
    }

    pub fn with_deadline(
        reader: impl Read + Send + 'static,
        deadline_at_ms: u64,
    ) -> Result<Self, String> {
        let remaining = remaining_wall_time_ms(deadline_at_ms, now_ms())
            .ok_or_else(|| "Creation exceeded its time limit.".to_string())?;
        Ok(Self::with_wall_time(
            reader,
            Duration::from_millis(remaining),
        ))
    }

    fn with_wall_time(reader: impl Read + Send + 'static, wall_time: Duration) -> Self {
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(reader);
            loop {
                let result = crate::overlay::creation_recovery::read_event(&mut reader);
                let finished = !matches!(result, Ok(Some(_)));
                if sender.send(result).is_err() || finished {
                    break;
                }
            }
        });
        let now = Instant::now();
        Self {
            receiver,
            started_at: now,
            last_event_at: now,
            wall_time,
        }
    }

    pub fn next(
        &mut self,
        pid: u32,
        cancelled: impl Fn() -> bool,
    ) -> Result<Option<Value>, String> {
        self.next_with_limits(pid, cancelled, self.wall_time, MAX_SILENCE)
    }

    fn next_with_limits(
        &mut self,
        pid: u32,
        cancelled: impl Fn() -> bool,
        wall_time: Duration,
        silence: Duration,
    ) -> Result<Option<Value>, String> {
        loop {
            self.check_limits(pid, &cancelled, wall_time, silence)?;
            let now = Instant::now();
            let wait = POLL_INTERVAL
                .min(wall_time.saturating_sub(now.duration_since(self.started_at)))
                .min(silence.saturating_sub(now.duration_since(self.last_event_at)));
            match self.receiver.recv_timeout(wait) {
                Ok(result) => {
                    self.check_limits(pid, &cancelled, wall_time, silence)?;
                    self.last_event_at = Instant::now();
                    return result;
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(None),
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }

    fn check_limits(
        &self,
        pid: u32,
        cancelled: &impl Fn() -> bool,
        wall_time: Duration,
        silence: Duration,
    ) -> Result<(), String> {
        let now = Instant::now();
        if cancelled()
            || now.duration_since(self.started_at) >= wall_time
            || now.duration_since(self.last_event_at) >= silence
        {
            if pid != 0 {
                crate::overlay::creation_recovery::terminate_process_tree(pid);
            }
            return Err("Creation process stopped responding.".into());
        }
        Ok(())
    }
}

fn remaining_wall_time_ms(deadline_at_ms: u64, now: u64) -> Option<u64> {
    let remaining = deadline_at_ms.saturating_sub(now);
    (remaining > 0).then_some(remaining.min(MAX_WALL_TIME_MS))
}

pub(crate) fn deadline_expired(deadline_at_ms: u64) -> bool {
    remaining_wall_time_ms(deadline_at_ms, now_ms()).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_event_is_reported_without_blocking_the_supervisor() {
        let reader = std::io::Cursor::new(vec![b'x'; 64 * 1024 + 2]);
        let mut supervisor = EventSupervisor::new(reader);
        assert!(supervisor.next(0, || false).is_err());
    }

    #[test]
    fn a_ready_event_cannot_bypass_cancellation_or_the_wall_deadline() {
        let make = || {
            let (sender, receiver) = mpsc::sync_channel(1);
            sender
                .send(Ok(Some(serde_json::json!({"event":"progress"}))))
                .unwrap();
            let now = Instant::now();
            EventSupervisor {
                receiver,
                started_at: now,
                last_event_at: now,
                wall_time: MAX_WALL_TIME,
            }
        };
        assert!(make().next(0, || true).is_err());
        assert!(
            make()
                .next_with_limits(0, || false, Duration::ZERO, MAX_SILENCE)
                .is_err()
        );
    }

    #[test]
    fn silent_stream_hits_its_watchdog() {
        let (_sender, receiver) = mpsc::sync_channel(1);
        let now = Instant::now();
        let mut supervisor = EventSupervisor {
            receiver,
            started_at: now,
            last_event_at: now,
            wall_time: Duration::from_millis(20),
        };
        assert!(
            supervisor
                .next_with_limits(
                    0,
                    || false,
                    Duration::from_millis(20),
                    Duration::from_millis(20),
                )
                .is_err()
        );
    }

    #[test]
    fn whole_job_watchdog_is_two_hours() {
        assert_eq!(MAX_WALL_TIME, Duration::from_secs(2 * 60 * 60));
    }

    #[test]
    fn recovery_uses_only_the_persisted_remaining_wall_time() {
        let ninety_minutes = 90 * 60 * 1_000;
        let two_hours = MAX_WALL_TIME_MS;
        assert_eq!(
            remaining_wall_time_ms(two_hours, ninety_minutes),
            Some(30 * 60 * 1_000)
        );
        assert_eq!(remaining_wall_time_ms(two_hours, two_hours), None);
        assert_eq!(remaining_wall_time_ms(two_hours, two_hours + 1), None);
    }
}
