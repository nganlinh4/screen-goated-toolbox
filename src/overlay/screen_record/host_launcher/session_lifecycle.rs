use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::JobObjects::TerminateJobObject;

static LAUNCH_CANCEL: LazyLock<Mutex<Option<Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(None));
static ACTIVE_JOB: LazyLock<Mutex<Option<(u64, OwnedHandle)>>> =
    LazyLock::new(|| Mutex::new(None));
static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);

pub(super) struct LaunchCancellation {
    token: Arc<AtomicBool>,
}

impl LaunchCancellation {
    pub(super) fn begin() -> Result<Self> {
        let mut active = LAUNCH_CANCEL
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        if super::REMOVAL_IN_PROGRESS.load(Ordering::Acquire) {
            bail!("recorder removal is in progress");
        }
        let token = Arc::new(AtomicBool::new(false));
        *active = Some(token.clone());
        Ok(Self { token })
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
        if active
            .as_ref()
            .is_some_and(|token| Arc::ptr_eq(token, &self.token))
        {
            active.take();
        }
    }
}

pub(super) struct ActiveJobRegistration {
    id: u64,
}

impl ActiveJobRegistration {
    pub(super) fn register(job: &OwnedHandle) -> Result<Self> {
        let id = NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed);
        let handle = job
            .try_clone()
            .map_err(|error| anyhow!("duplicate recorder job handle: {error}"))?;
        let mut active = ACTIVE_JOB
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        if active.is_some() {
            bail!("another recorder worker job is already active");
        }
        *active = Some((id, handle));
        Ok(Self { id })
    }
}

impl Drop for ActiveJobRegistration {
    fn drop(&mut self) {
        let mut active = ACTIVE_JOB
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        if active.as_ref().is_some_and(|(id, _)| *id == self.id) {
            active.take();
        }
    }
}

pub(super) fn cancel_active_work() {
    if let Some(cancelled) = LAUNCH_CANCEL
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .as_ref()
    {
        cancelled.store(true, Ordering::Release);
    }
    if let Some((_, job)) = ACTIVE_JOB
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .as_ref()
    {
        let _ = unsafe { TerminateJobObject(HANDLE(job.as_raw_handle()), 1) };
    }
}

#[cfg(test)]
pub(super) fn active_job_registered() -> bool {
    ACTIVE_JOB
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .is_some()
}
