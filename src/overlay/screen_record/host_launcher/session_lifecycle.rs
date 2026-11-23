use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use crate::process_job::JobGroup;
use anyhow::{Result, bail};

/// The two worker processes the host may run at once: the interactive recorder
/// window, and one headless job (audio decode, replay, narration test). They
/// are separate processes, so a headless job never has to wait for, or fail
/// against, an open recorder window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum JobSlot {
    Interactive,
    Headless,
}

impl JobSlot {
    const COUNT: usize = 2;

    fn index(self) -> usize {
        match self {
            Self::Interactive => 0,
            Self::Headless => 1,
        }
    }

    /// A command error in the interactive window must not kill the window,
    /// while a failed headless job is disposable.
    pub(super) fn preserves_session(self) -> bool {
        self == Self::Interactive
    }
}

/// One entry per [`JobSlot`].
type PerSlot<T> = Mutex<[Option<T>; JobSlot::COUNT]>;

static LAUNCH_CANCEL: LazyLock<PerSlot<Arc<AtomicBool>>> =
    LazyLock::new(|| Mutex::new([None, None]));
static ACTIVE_JOBS: LazyLock<PerSlot<(u64, JobGroup)>> = LazyLock::new(|| Mutex::new([None, None]));
static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);

pub(super) struct LaunchCancellation {
    slot: JobSlot,
    token: Arc<AtomicBool>,
}

impl LaunchCancellation {
    pub(super) fn begin(slot: JobSlot) -> Result<Self> {
        let mut active = LAUNCH_CANCEL
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        if super::REMOVAL_IN_PROGRESS.load(Ordering::Acquire) {
            bail!("recorder removal is in progress");
        }
        let token = Arc::new(AtomicBool::new(false));
        active[slot.index()] = Some(token.clone());
        Ok(Self { slot, token })
    }

    pub(super) fn token(&self) -> &AtomicBool {
        &self.token
    }
}

impl Drop for LaunchCancellation {
    fn drop(&mut self) {
        let mut active = LAUNCH_CANCEL
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        let entry = &mut active[self.slot.index()];
        if entry
            .as_ref()
            .is_some_and(|token| Arc::ptr_eq(token, &self.token))
        {
            entry.take();
        }
    }
}

pub(super) struct ActiveJobRegistration {
    slot: JobSlot,
    id: u64,
}

impl ActiveJobRegistration {
    pub(super) fn register(group: JobGroup, slot: JobSlot) -> Result<Self> {
        let id = NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed);
        let mut active = ACTIVE_JOBS
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        if super::REMOVAL_IN_PROGRESS.load(Ordering::Acquire) {
            bail!("recorder removal is in progress");
        }
        let entry = &mut active[slot.index()];
        if entry.is_some() {
            bail!("another {slot:?} recorder worker job is already active");
        }
        *entry = Some((id, group));
        Ok(Self { slot, id })
    }
}

impl Drop for ActiveJobRegistration {
    fn drop(&mut self) {
        let mut active = ACTIVE_JOBS
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        let entry = &mut active[self.slot.index()];
        if entry.as_ref().is_some_and(|(id, _)| *id == self.id) {
            entry.take();
        }
    }
}

/// Cancels launches and terminates the worker processes in every slot.
pub(super) fn cancel_active_work() {
    cancel_slots(&[JobSlot::Interactive, JobSlot::Headless]);
}

/// Cancels a pending launch and terminates the running worker of one slot.
pub(super) fn cancel_slot_work(slot: JobSlot) {
    cancel_slots(&[slot]);
}

fn cancel_slots(slots: &[JobSlot]) {
    {
        let launches = LAUNCH_CANCEL
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        for slot in slots {
            if let Some(cancelled) = &launches[slot.index()] {
                cancelled.store(true, Ordering::Release);
            }
        }
    }
    let jobs = ACTIVE_JOBS
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    for slot in slots {
        if let Some((_, job)) = &jobs[slot.index()] {
            let _ = job.terminate();
        }
    }
}

#[cfg(test)]
pub(super) fn active_job_registered() -> bool {
    ACTIVE_JOBS
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .iter()
        .any(Option::is_some)
}
