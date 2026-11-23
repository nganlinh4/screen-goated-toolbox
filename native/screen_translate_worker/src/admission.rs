//! One-capture-at-a-time admission shared by the stdin reader and the worker.

use anyhow::Result;
use sgt_screen_text_detector_protocol::stream::{self, Event};
use std::io::Write;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct Active {
    /// Capture (or handshake) currently being served; zero when idle.
    id: u64,
    cancelled: Arc<AtomicBool>,
    /// Highest id ever admitted. Ids must increase strictly.
    last_id: u64,
}

#[derive(Clone)]
pub(crate) struct Admission(Arc<Mutex<Active>>);

impl Admission {
    /// The handshake occupies the worker until `finish(hello_id)` reports ready.
    pub(crate) fn starting(hello_id: u64) -> (Self, Arc<AtomicBool>) {
        let cancelled = Arc::new(AtomicBool::new(false));
        let admission = Self(Arc::new(Mutex::new(Active {
            id: hello_id,
            cancelled: Arc::clone(&cancelled),
            last_id: hello_id,
        })));
        (admission, cancelled)
    }

    /// Admit a new capture, or `None` when busy or the id is not newer.
    pub(crate) fn admit(&self, id: u64) -> Option<Arc<AtomicBool>> {
        let mut active = self.lock();
        if active.id != 0 || id <= active.last_id {
            return None;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        active.id = id;
        active.last_id = id;
        active.cancelled = Arc::clone(&cancelled);
        Some(cancelled)
    }

    /// Cancel `id` only while it is the active capture.
    pub(crate) fn cancel(&self, id: u64) {
        let active = self.lock();
        if active.id == id {
            active.cancelled.store(true, Ordering::Release);
        }
    }

    /// The input stream ended: whatever is running can no longer be delivered.
    pub(crate) fn cancel_active(&self) {
        self.lock().cancelled.store(true, Ordering::Release);
    }

    pub(crate) fn finish(&self, id: u64) {
        let mut active = self.lock();
        if active.id == id {
            active.id = 0;
        }
    }

    #[cfg(test)]
    fn is_idle(&self) -> bool {
        self.lock().id == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Active> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Clear admission before the terminal event leaves the worker: the host may
/// send its next capture as soon as it reads the event.
pub(crate) fn send_terminal(
    writer: &mut impl Write,
    admission: &Admission,
    id: u64,
    terminal: &Event,
) -> Result<()> {
    admission.finish(id);
    stream::write_event(writer, id, terminal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ObservingWriter {
        admission: Admission,
        idle_at_first_write: Option<bool>,
    }

    impl Write for ObservingWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.idle_at_first_write
                .get_or_insert_with(|| self.admission.is_idle());
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn terminal_event_is_written_only_after_the_worker_is_idle() {
        let (admission, _) = Admission::starting(1);
        admission.finish(1);
        admission.admit(2).unwrap();
        let mut writer = ObservingWriter {
            admission: admission.clone(),
            idle_at_first_write: None,
        };
        send_terminal(&mut writer, &admission, 2, &Event::Finished { count: 0 }).unwrap();
        assert_eq!(writer.idle_at_first_write, Some(true));
        assert!(admission.admit(3).is_some());
    }

    #[test]
    fn handshake_blocks_captures_until_ready() {
        let (admission, startup) = Admission::starting(5);
        assert!(admission.admit(6).is_none());
        admission.cancel(5);
        assert!(startup.load(Ordering::Acquire));
        admission.finish(5);
        assert!(
            admission.admit(5).is_none(),
            "ids must be newer than the handshake"
        );
        assert!(admission.admit(6).is_some());
    }

    #[test]
    fn stale_or_finished_ids_cannot_cancel_or_reenter() {
        let (admission, _) = Admission::starting(1);
        admission.finish(1);
        let first = admission.admit(2).unwrap();
        admission.finish(2);
        admission.cancel(2);
        assert!(!first.load(Ordering::Acquire));
        let second = admission.admit(3).unwrap();
        admission.cancel(2);
        assert!(!second.load(Ordering::Acquire));
        admission.finish(2);
        assert!(
            admission.admit(4).is_none(),
            "a stale finish must not free the worker"
        );
        admission.cancel(3);
        assert!(second.load(Ordering::Acquire));
    }
}
