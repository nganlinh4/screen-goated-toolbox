use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StartupAttempt {
    Connected,
    Failed,
    Stopped,
}

/// Bounded startup barrier for the installed integration catalog. Connection
/// workers remain asynchronous; this handle reports when all attempts settle or
/// when the caller's deadline expires.
pub(in crate::overlay::computer_control) struct StartupCatalog {
    installed: usize,
    attempts: Receiver<StartupAttempt>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::overlay::computer_control) struct StartupCatalogReport {
    pub installed: usize,
    pub connected: usize,
    pub failed: usize,
    pub pending: usize,
    pub stopped: bool,
}

impl StartupCatalog {
    pub(super) fn new(installed: usize, attempts: Receiver<StartupAttempt>) -> Self {
        Self {
            installed,
            attempts,
        }
    }

    pub(in crate::overlay::computer_control) fn wait(
        self,
        timeout: Duration,
        stop: &AtomicBool,
    ) -> StartupCatalogReport {
        let deadline = Instant::now() + timeout;
        let mut report = StartupCatalogReport {
            installed: self.installed,
            ..StartupCatalogReport::default()
        };
        while report.connected + report.failed < report.installed {
            if stop.load(Ordering::SeqCst) {
                report.stopped = true;
                break;
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let poll = remaining.min(Duration::from_millis(50));
            match self.attempts.recv_timeout(poll) {
                Ok(StartupAttempt::Connected) => report.connected += 1,
                Ok(StartupAttempt::Failed) => report.failed += 1,
                Ok(StartupAttempt::Stopped) => {
                    report.stopped = true;
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    report.failed += report
                        .installed
                        .saturating_sub(report.connected + report.failed);
                    break;
                }
            }
        }
        report.pending = report
            .installed
            .saturating_sub(report.connected + report.failed);
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn reports_settled_attempts() {
        let (tx, attempts) = mpsc::channel();
        tx.send(StartupAttempt::Connected).unwrap();
        tx.send(StartupAttempt::Failed).unwrap();
        drop(tx);
        let report =
            StartupCatalog::new(2, attempts).wait(Duration::from_secs(1), &AtomicBool::new(false));

        assert_eq!(report.connected, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.pending, 0);
        assert!(!report.stopped);
    }

    #[test]
    fn deadline_is_bounded_and_preserves_pending_count() {
        let (_tx, attempts) = mpsc::channel();
        let report = StartupCatalog::new(1, attempts).wait(Duration::ZERO, &AtomicBool::new(false));

        assert_eq!(report.connected, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(report.pending, 1);
        assert!(!report.stopped);
    }
}
