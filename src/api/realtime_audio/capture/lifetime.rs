use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;

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
