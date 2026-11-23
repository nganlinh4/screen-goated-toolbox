//! Single-flight ownership for a window-event tracker and its startup worker.
use anyhow::{Result, anyhow};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

pub(super) trait ThreadLease: Send + Sync + 'static {
    fn state(&self) -> ThreadState;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ThreadState {
    Alive,
    Exited,
    Unknown,
}

pub(super) struct Prepared<T> {
    pub(super) thread: T,
    pub(super) thread_id: u32,
    pub(super) hook: usize,
    pub(super) activate: mpsc::Sender<()>,
}

pub(super) struct Owner<T> {
    pub(super) thread: T,
    pub(super) thread_id: u32,
    pub(super) hook: usize,
    pub(super) epoch: u64,
}

pub(super) struct Lifecycle<T> {
    pub(super) current: Option<Arc<Owner<T>>>,
    pending: Option<mpsc::Receiver<Option<Prepared<T>>>>,
    retiring: Option<Arc<Owner<T>>>,
    next_epoch: u64,
    retry_at: Instant,
    exhausted: bool,
}

impl<T: ThreadLease> Lifecycle<T> {
    pub(super) fn new() -> Self {
        Self {
            current: None,
            pending: None,
            retiring: None,
            next_epoch: 0,
            retry_at: Instant::now(),
            exhausted: false,
        }
    }

    pub(super) fn acquire(
        &mut self,
        timeout: Duration,
        start: impl FnOnce() -> Result<mpsc::Receiver<Option<Prepared<T>>>>,
        mut retire: impl FnMut(),
        mut activate: impl FnMut(usize),
    ) -> Result<Arc<Owner<T>>> {
        if let Some(owner) = &self.current {
            match owner.thread.state() {
                ThreadState::Alive => return Ok(owner.clone()),
                ThreadState::Unknown => {
                    return Err(anyhow!("native window tracker liveness is unknown"));
                }
                ThreadState::Exited => {}
            }
            retire();
            self.current = None;
        }
        if let Some(owner) = &self.retiring {
            if owner.thread.state() != ThreadState::Exited {
                return Err(anyhow!("native window tracker is retiring"));
            }
            self.retiring = None;
        }
        if self.exhausted {
            return Err(anyhow!("native window tracker generation exhausted"));
        }
        if self.pending.is_none() {
            if Instant::now() < self.retry_at {
                return Err(anyhow!("native window tracker retry is delayed"));
            }
            match start() {
                Ok(pending) => self.pending = Some(pending),
                Err(error) => {
                    self.retry_at = Instant::now() + Duration::from_secs(2);
                    return Err(error);
                }
            }
        }
        let ready = match self.pending.as_ref().unwrap().recv_timeout(timeout) {
            Ok(Some(ready)) => ready,
            Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.pending = None;
                self.retry_at = Instant::now() + Duration::from_secs(2);
                return Err(anyhow!("native window lifecycle tracker could not start"));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(anyhow!("native window lifecycle tracker is starting"));
            }
        };
        self.pending = None;
        let Some(epoch) = self.next_epoch.checked_add(1) else {
            self.exhausted = true;
            let owner = Arc::new(Owner {
                thread: ready.thread,
                thread_id: ready.thread_id,
                hook: ready.hook,
                epoch: 0,
            });
            self.retiring = Some(owner);
            return Err(anyhow!("native window tracker generation exhausted"));
        };
        self.next_epoch = epoch;
        let owner = Arc::new(Owner {
            thread: ready.thread,
            thread_id: ready.thread_id,
            hook: ready.hook,
            epoch,
        });
        match owner.thread.state() {
            ThreadState::Alive => {}
            ThreadState::Exited => {
                self.retry_at = Instant::now() + Duration::from_secs(2);
                return Err(anyhow!(
                    "native window lifecycle tracker exited during startup"
                ));
            }
            ThreadState::Unknown => {
                self.retiring = Some(owner);
                return Err(anyhow!("native window tracker startup liveness is unknown"));
            }
        }
        activate(owner.hook);
        if ready.activate.send(()).is_err() {
            retire();
            self.retiring = Some(owner);
            return Err(anyhow!("native window lifecycle tracker activation failed"));
        }
        self.current = Some(owner.clone());
        Ok(owner)
    }

    pub(super) fn owns(&self, owner: &Arc<Owner<T>>) -> bool {
        self.current.as_ref().is_some_and(|current| {
            Arc::ptr_eq(current, owner)
                && current.epoch == owner.epoch
                && current.thread.state() == ThreadState::Alive
        })
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
