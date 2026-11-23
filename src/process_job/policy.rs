//! Explicit job inheritance and parent-owned helper groups.
use super::JobControl;
use anyhow::{Context, Result, ensure};
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::sync::{Arc, Mutex};
#[cfg(not(feature = "recorder-worker"))]
use windows::Win32::Foundation::HANDLE;
#[cfg(not(feature = "recorder-worker"))]
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
#[cfg(not(feature = "recorder-worker"))]
use windows::Win32::System::JobObjects::SetInformationJobObject;
use windows::Win32::System::JobObjects::{
    JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, QueryInformationJobObject,
};
use windows::core::PCWSTR;

pub(crate) const PARENT_JOB_ENV: &str = "SGT_PROCESS_PARENT_JOB";
#[cfg(not(feature = "recorder-worker"))]
pub(crate) enum Inheritance {
    Contained,
    #[cfg(not(feature = "recorder-worker"))]
    SilentBreakaway,
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn configure(job: &JobControl, inheritance: Inheritance) -> Result<()> {
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    #[cfg(not(feature = "recorder-worker"))]
    if matches!(inheritance, Inheritance::SilentBreakaway) {
        limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
    }
    #[cfg(feature = "recorder-worker")]
    let _ = inheritance;
    unsafe {
        SetInformationJobObject(
            super::handle(&job.0),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            std::mem::size_of_val(&limits) as u32,
        )?;
    }
    Ok(())
}

fn name(value: &str) -> Result<Vec<u16>> {
    let mut value: Vec<_> = value.encode_utf16().collect();
    ensure!(
        !value.is_empty() && value.len() < 256 && !value.contains(&0),
        "Invalid process job identity"
    );
    value.push(0);
    Ok(value)
}

impl JobControl {
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn contains_process(&self, process: HANDLE) -> Result<bool> {
        let mut member = windows::core::BOOL::default();
        unsafe {
            windows::Win32::System::JobObjects::IsProcessInJob(
                process,
                Some(super::handle(&self.0)),
                &mut member,
            )?;
        }
        Ok(member.as_bool())
    }

    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn assign(&self, process: HANDLE) -> Result<()> {
        unsafe {
            windows::Win32::System::JobObjects::AssignProcessToJobObject(
                super::handle(&self.0),
                process,
            )?;
        }
        ensure!(
            self.contains_process(process)?,
            "Process job assignment could not be verified"
        );
        Ok(())
    }
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn named(value: &str) -> Result<Self> {
        let value = name(value)?;
        let (created, error) = unsafe {
            let created =
                windows::Win32::System::JobObjects::CreateJobObjectW(None, PCWSTR(value.as_ptr()));
            (created, GetLastError())
        };
        let raw = created.context("create parent-owned process job")?;
        let handle = unsafe { OwnedHandle::from_raw_handle(raw.0) };
        ensure!(
            error != ERROR_ALREADY_EXISTS,
            "Process job identity is already in use"
        );
        let job = Self(
            Arc::new(handle),
            Arc::new(Mutex::new(super::completion::Members::default())),
        );
        configure(&job, Inheritance::Contained)?;
        job.verify_contained()?;
        Ok(job)
    }

    pub(crate) fn limits(&self) -> Result<JOBOBJECT_EXTENDED_LIMIT_INFORMATION> {
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        unsafe {
            QueryInformationJobObject(
                Some(super::handle(&self.0)),
                JobObjectExtendedLimitInformation,
                (&raw mut limits).cast(),
                std::mem::size_of_val(&limits) as u32,
                None,
            )?;
        }
        Ok(limits)
    }

    fn verify_contained(&self) -> Result<()> {
        let flags = self.limits()?.BasicLimitInformation.LimitFlags;
        ensure!(
            flags.contains(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)
                && (flags & (JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK | JOB_OBJECT_LIMIT_BREAKAWAY_OK))
                    .0
                    == 0,
            "Parent process job does not enforce containment"
        );
        Ok(())
    }

    #[cfg(any(test, feature = "recorder-worker"))]
    fn open_contained(value: &str) -> Result<Self> {
        use windows::Win32::System::JobObjects::OpenJobObjectW;
        use windows::Win32::System::SystemServices::{JOB_OBJECT_ASSIGN_PROCESS, JOB_OBJECT_QUERY};
        let value = name(value)?;
        let raw = unsafe {
            OpenJobObjectW(
                JOB_OBJECT_ASSIGN_PROCESS | JOB_OBJECT_QUERY,
                false,
                PCWSTR(value.as_ptr()),
            )?
        };
        let job = Self(
            Arc::new(unsafe { OwnedHandle::from_raw_handle(raw.0) }),
            Arc::new(Mutex::new(super::completion::Members::default())),
        );
        job.verify_contained()?;
        Ok(job)
    }
}

#[cfg(any(test, feature = "recorder-worker"))]
static PARENT: std::sync::OnceLock<JobControl> = std::sync::OnceLock::new();

pub(super) fn parent_job() -> Option<&'static JobControl> {
    #[cfg(any(test, feature = "recorder-worker"))]
    {
        PARENT.get()
    }
    #[cfg(not(any(test, feature = "recorder-worker")))]
    {
        None
    }
}

#[cfg(any(test, feature = "recorder-worker"))]
pub(crate) fn initialize_parent_job() -> Result<()> {
    let identity =
        std::env::var(PARENT_JOB_ENV).context("Parent process job identity is missing")?;
    unsafe {
        std::env::remove_var(PARENT_JOB_ENV);
    }
    let job = JobControl::open_contained(&identity)?;
    PARENT
        .set(job)
        .map_err(|_| anyhow::anyhow!("Parent process job is already configured"))
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn attach_parent(process: HANDLE) -> Result<()> {
    #[cfg(any(test, feature = "recorder-worker"))]
    if let Some(job) = PARENT.get() {
        job.assign(process)?;
    }
    #[cfg(not(any(test, feature = "recorder-worker")))]
    let _ = process;
    Ok(())
}
