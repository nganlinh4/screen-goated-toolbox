use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;

use super::super::d3d_interop::KeyedMutexGuard;

/// A failed pipeline must not return a device with outstanding submissions to
/// the shared cache, including exits before its normal completion wait begins.
pub(super) struct SubmissionGuard {
    failed: Arc<AtomicBool>,
    outstanding: usize,
}

impl SubmissionGuard {
    pub fn new(failed: Arc<AtomicBool>) -> Self {
        Self {
            failed,
            outstanding: 0,
        }
    }

    pub fn begin(&mut self) {
        self.outstanding = self.outstanding.saturating_add(1);
    }

    pub fn complete_one(&mut self) -> Result<(), String> {
        let Some(remaining) = self.outstanding.checked_sub(1) else {
            self.failed.store(true, Ordering::Release);
            return Err("GPU completion did not match a submitted frame".to_string());
        };
        self.outstanding = remaining;
        Ok(())
    }
}

impl Drop for SubmissionGuard {
    fn drop(&mut self) {
        if self.outstanding != 0 {
            self.failed.store(true, Ordering::Release);
        }
    }
}

/// A ring slot whose submitted GPU work may still be using its shared surface.
/// Dropping an uncompleted slot deliberately does not release its keyed mutex:
/// peer threads stop via cancellation instead of reusing an uncertain surface.
pub(super) struct PendingSurface<'a> {
    index: usize,
    guard: Option<KeyedMutexGuard<'a>>,
}

impl<'a> PendingSurface<'a> {
    pub fn acquire(
        mutexes: &'a [IDXGIKeyedMutex],
        index: usize,
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        let guard = if mutexes.is_empty() {
            None
        } else {
            let mutex = mutexes
                .get(index)
                .ok_or_else(|| "Invalid shared GPU slot".to_string())?;
            Some(KeyedMutexGuard::acquire(mutex, 0, cancel)?)
        };
        Ok(Self { index, guard })
    }

    /// Call before attempting submission: a panic or failure after this point
    /// cannot prove that the GPU did not receive the work.
    pub fn mark_submitted(&mut self) {
        if let Some(guard) = &mut self.guard {
            guard.quarantine();
        }
    }

    pub fn release_after_completion(mut self) -> Result<usize, String> {
        if let Some(guard) = self.guard.take() {
            guard.release_after_completion()?;
        }
        Ok(self.index)
    }
}

pub(super) fn recycle_completed<'a>(
    queue: &mut std::collections::VecDeque<Option<PendingSurface<'a>>>,
    tx: Option<&Sender<usize>>,
) -> Result<(), String> {
    if let Some(Some(surface)) = queue.pop_front() {
        let index = surface.release_after_completion()?;
        if let Some(tx) = tx {
            let _ = tx.send(index);
        }
    }
    Ok(())
}

pub(super) fn receive_or_end<T>(
    rx: &Receiver<T>,
    cancel: &AtomicBool,
) -> Result<Option<T>, String> {
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("GPU pipeline cancelled while waiting for input".to_string());
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(value) => return Ok(Some(value)),
            Err(RecvTimeoutError::Disconnected) => return Ok(None),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_empty_mutex_slot_is_not_recycled_until_completion() {
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = AtomicBool::new(false);
        let mut slot = PendingSurface::acquire(&[], 7, &cancel).unwrap();
        slot.mark_submitted();
        let mut queue = std::collections::VecDeque::from([Some(slot)]);
        assert!(rx.try_recv().is_err());
        recycle_completed(&mut queue, Some(&tx)).unwrap();
        assert_eq!(rx.try_recv().unwrap(), 7);
    }

    #[test]
    fn uncertain_surface_is_not_recycled() {
        let (tx, rx) = std::sync::mpsc::channel::<usize>();
        let cancel = AtomicBool::new(false);
        let mut slot = PendingSurface::acquire(&[], 7, &cancel).unwrap();
        slot.mark_submitted();
        let queue = std::collections::VecDeque::from([Some(slot)]);
        drop(queue);
        assert!(rx.try_recv().is_err());
        drop(tx);
    }

    #[test]
    fn cancellation_interrupts_wait_without_receiving_a_slot() {
        let (_tx, rx) = std::sync::mpsc::channel::<usize>();
        let cancel = AtomicBool::new(true);
        assert!(receive_or_end(&rx, &cancel).is_err());
    }

    #[test]
    fn incomplete_frames_retire_the_context_but_completed_frames_do_not() {
        let failed = Arc::new(AtomicBool::new(false));
        {
            let mut work = SubmissionGuard::new(Arc::clone(&failed));
            work.begin();
            work.begin();
            work.complete_one().unwrap();
        }
        assert!(failed.load(Ordering::Acquire));
        failed.store(false, Ordering::Release);
        {
            let mut work = SubmissionGuard::new(Arc::clone(&failed));
            work.begin();
            work.complete_one().unwrap();
        }
        assert!(!failed.load(Ordering::Acquire));
    }
}
