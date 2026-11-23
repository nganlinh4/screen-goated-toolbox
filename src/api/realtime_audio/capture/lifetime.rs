use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::JoinHandle;
use std::time::Duration;

/// How long a capture worker may take to open its device before the start
/// call fails instead of reporting a capture that never produces audio.
const CAPTURE_READY_TIMEOUT: Duration = Duration::from_secs(5);

/// Lets a capture worker report whether its device opened.
pub struct CaptureReady(mpsc::SyncSender<Result<(), String>>);

impl CaptureReady {
    pub fn opened(&self) {
        let _ = self.0.try_send(Ok(()));
    }

    pub fn failed(&self, reason: impl Into<String>) {
        let _ = self.0.try_send(Err(reason.into()));
    }
}

/// Owns a capture worker. Cancellation is private and never reset across sessions.
#[must_use = "dropping the owner stops and joins the capture worker"]
pub struct CaptureWorker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl CaptureWorker {
    pub fn spawn(
        name: &str,
        run: impl FnOnce(Arc<AtomicBool>) + Send + 'static,
    ) -> anyhow::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let thread = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || run(worker_stop))?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    /// Spawns a worker that must report its device initialization, so an
    /// activation failure is returned to the caller instead of leaving a
    /// silent capture running.
    pub fn spawn_ready(
        name: &str,
        run: impl FnOnce(Arc<AtomicBool>, CaptureReady) + Send + 'static,
    ) -> anyhow::Result<Self> {
        Self::spawn_ready_within(name, CAPTURE_READY_TIMEOUT, run)
    }

    fn spawn_ready_within(
        name: &str,
        timeout: Duration,
        run: impl FnOnce(Arc<AtomicBool>, CaptureReady) + Send + 'static,
    ) -> anyhow::Result<Self> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = Self::spawn(name, move |stop| run(stop, CaptureReady(ready_tx)))?;
        match ready_rx.recv_timeout(timeout) {
            Ok(Ok(())) => Ok(worker),
            Ok(Err(reason)) => Err(anyhow::anyhow!("{name}: {reason}")),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                worker.abandon();
                Err(anyhow::anyhow!("{name}: capture did not start in time"))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(anyhow::anyhow!("{name}: capture stopped before it started"))
            }
        }
    }
}

impl CaptureWorker {
    /// Signals stop without joining: a worker hung inside device activation
    /// cannot be joined without hanging the caller too. It exits on its own
    /// once activation returns and it observes the stop flag.
    fn abandon(mut self) {
        self.stop.store(true, Ordering::Release);
        drop(self.thread.take());
    }
}

impl Drop for CaptureWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Keeps either capture backend alive until its consuming model returns.
pub struct CaptureStream {
    _stream: Option<cpal::Stream>,
    _worker: Option<CaptureWorker>,
}

impl CaptureStream {
    pub(super) fn with_monitor(stream: cpal::Stream, monitor: CaptureWorker) -> Self {
        Self {
            _stream: Some(stream),
            _worker: Some(monitor),
        }
    }
}

impl From<cpal::Stream> for CaptureStream {
    fn from(stream: cpal::Stream) -> Self {
        Self {
            _stream: Some(stream),
            _worker: None,
        }
    }
}

impl From<CaptureWorker> for CaptureStream {
    fn from(worker: CaptureWorker) -> Self {
        Self {
            _stream: None,
            _worker: Some(worker),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn dropping_capture_joins_before_reusable_session_signal_can_restart() {
        let session_stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();
        let old_session_stop = session_stop.clone();
        let worker_stopped = stopped.clone();
        let capture = CaptureWorker::spawn("capture-test", move |stop| {
            ready_tx.send(()).unwrap();
            while !stop.load(Ordering::Acquire) && !old_session_stop.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            worker_stopped.store(true, Ordering::Release);
        })
        .unwrap();
        ready_rx.recv().unwrap();
        drop(capture);
        session_stop.store(false, Ordering::Release);
        assert!(stopped.load(Ordering::Acquire));
    }

    #[test]
    fn capture_activation_failure_is_returned_to_the_caller() {
        let failed = CaptureWorker::spawn_ready("ready-test", |_, ready| {
            ready.failed("loopback activation failed");
        });
        let error = failed
            .err()
            .expect("activation failure must not start a capture");
        assert!(error.to_string().contains("loopback activation failed"));

        let silent = CaptureWorker::spawn_ready("ready-test", |_, _| {});
        assert!(silent.is_err(), "a worker that exits without opening fails");

        let opened = CaptureWorker::spawn_ready("ready-test", |stop, ready| {
            ready.opened();
            while !stop.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        });
        assert!(opened.is_ok());
    }

    #[test]
    fn hung_capture_initialization_times_out() {
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let hung = CaptureWorker::spawn_ready_within(
            "ready-test",
            Duration::from_millis(50),
            move |_, _| {
                let _ = release_rx.recv();
            },
        );
        drop(release_tx);
        assert!(hung.is_err());
    }

    #[test]
    fn dropping_refresh_owner_also_joins_its_active_capture() {
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_stopped = stopped.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let refresh = CaptureWorker::spawn("refresh-test", move |stop| {
            let _capture = CaptureWorker::spawn("nested-capture-test", move |stop| {
                ready_tx.send(()).unwrap();
                while !stop.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                worker_stopped.store(true, Ordering::Release);
            })
            .unwrap();
            while !stop.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        })
        .unwrap();
        ready_rx.recv().unwrap();
        drop(refresh);
        assert!(stopped.load(Ordering::Acquire));
    }
}
