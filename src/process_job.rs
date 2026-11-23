//! Suspended launch and owned Windows process-tree containment.

use anyhow::{Context, Result, anyhow, bail};
use std::ffi::c_void;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
#[cfg(not(feature = "recorder-worker"))]
use std::os::windows::process::CommandExt;
#[cfg(not(feature = "recorder-worker"))]
use std::process::Command;
use std::process::ExitStatus;
use std::sync::{Arc, Mutex};
#[cfg(not(feature = "recorder-worker"))]
use windows::Win32::Foundation::ERROR_NO_MORE_FILES;
use windows::Win32::Foundation::HANDLE;
#[cfg(not(feature = "recorder-worker"))]
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::JobObjects::{
    CreateJobObjectW, JOB_OBJECT_LIMIT, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject,
};
#[cfg(not(feature = "recorder-worker"))]
use windows::Win32::System::Threading::{OpenThread, THREAD_SUSPEND_RESUME};
use windows::Win32::System::Threading::{ResumeThread, TerminateProcess};
#[cfg(not(feature = "recorder-worker"))]
use windows::core::HRESULT;
use windows::core::PCWSTR;

#[cfg(not(feature = "recorder-worker"))]
const CREATE_SUSPENDED: u32 = 0x0000_0004;
#[cfg(not(feature = "recorder-worker"))]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[path = "process_job/child.rs"]
mod child;
use child::Child;
pub(crate) use child::{Input, Output};
#[path = "process_job/launch_slot.rs"]
mod launch_slot;
#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "process_job/launch_tests.rs"]
mod launch_tests;
pub(crate) use launch_slot::LaunchSlot;
#[path = "process_job/batch_line.rs"]
mod batch_line;
#[path = "process_job/helper.rs"]
mod helper;
pub(crate) use helper::HelperCommand;
#[path = "process_job/environment.rs"]
mod environment;

#[path = "process_job/completion.rs"]
mod completion;
#[path = "process_job/policy.rs"]
mod policy;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use policy::Inheritance;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use policy::PARENT_JOB_ENV;
#[cfg(any(test, feature = "recorder-worker"))]
pub(crate) use policy::initialize_parent_job;
#[path = "process_job/owned.rs"]
mod owned;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use owned::JobGroup;
pub(crate) use owned::OwnedProcess;

pub(crate) struct ContainedChild {
    child: Child,
    job: JobControl,
    primary_thread: Option<OwnedHandle>,
    assigned: bool,
}

impl AsRawHandle for ContainedChild {
    fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
        self.child.handle().0
    }
}

impl ContainedChild {
    /// Create the child suspended, then attach it before any user command executes.
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn spawn(command: &mut Command) -> Result<Self> {
        Self::spawn_retained(command, &mut Vec::new(), Inheritance::Contained)
    }

    #[cfg(not(feature = "recorder-worker"))]
    fn spawn_retained(
        command: &mut Command,
        resources: &mut Vec<Box<dyn Send>>,
        inheritance: Inheritance,
    ) -> Result<Self> {
        owned::ensure_reaper()?;
        let job = JobControl(
            Arc::new(create_job()?),
            Arc::new(Mutex::new(completion::Members::default())),
        );
        policy::configure(&job, inheritance)?;
        let child = command
            .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
            .spawn()
            .context("launch suspended command process")?;
        let mut child = Self {
            child: Child::Standard(child),
            job,
            primary_thread: None,
            assigned: false,
        };
        let prepared = (|| -> Result<()> {
            policy::attach_parent(child.child.handle())?;
            child.job.assign(child.child.handle())?;
            child.assigned = true;
            child.primary_thread = Some(find_primary_thread(child.id())?);
            Ok(())
        })();
        match prepared {
            Ok(()) => Ok(child),
            Err(error) => {
                owned::retire_failed_launch(child, std::mem::take(resources));
                Err(error)
            }
        }
    }

    /// Create the child suspended and already inside its job in one native
    /// call (`PROC_THREAD_ATTRIBUTE_JOB_LIST`). Unlike [`Self::spawn`], no
    /// instant exists where a creator crash could leave it outside containment.
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn spawn_atomic(command: &HelperCommand) -> Result<Self> {
        owned::ensure_reaper()?;
        let job = JobControl(
            Arc::new(create_job()?),
            Arc::new(Mutex::new(completion::Members::default())),
        );
        policy::configure(&job, Inheritance::Contained)?;
        let (child, thread) = helper::spawn(command, &job)?;
        Ok(Self {
            child: Child::Helper(child),
            job,
            primary_thread: Some(thread),
            assigned: true,
        })
    }

    #[cfg(all(test, not(feature = "recorder-worker")))]
    pub(crate) fn assigned_at_creation(&self) -> bool {
        matches!(self.child, Child::Helper(_))
    }

    pub(crate) fn resume(&self) -> Result<()> {
        let primary = self
            .primary_thread
            .as_ref()
            .ok_or_else(|| anyhow!("command preparation is incomplete"))?;
        let previous = unsafe { ResumeThread(handle(primary)) };
        if previous == u32::MAX {
            return Err(anyhow!(
                "resume command process: {}",
                std::io::Error::last_os_error()
            ));
        }
        if previous != 1 {
            bail!("command process had an unexpected suspension count");
        }
        Ok(())
    }

    pub(crate) fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn id(&self) -> u32 {
        self.child.id()
    }

    pub(crate) fn take_stdin(&mut self) -> Option<Input> {
        self.child.take_stdin()
    }

    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn control(&self) -> JobControl {
        self.job.clone()
    }

    pub(crate) fn take_stdout(&mut self) -> Option<Output> {
        self.child.take_stdout()
    }

    pub(crate) fn take_stderr(&mut self) -> Option<Output> {
        self.child.take_stderr()
    }

    pub(crate) fn terminate(&self) -> Result<()> {
        if self.assigned {
            self.job.terminate()
        } else {
            unsafe { TerminateProcess(self.child.handle(), 1) }
                .context("terminate suspended command")
        }
    }

    pub(crate) fn active_processes(&self) -> Result<u32> {
        if self.assigned {
            self.job.active_processes()
        } else {
            Ok(0)
        }
    }

    /// After a normal parent exit, leave intentionally launched applications alive.
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn release_descendants(&self) -> Result<()> {
        set_kill_on_close(&self.job.0, false)
    }
}

#[derive(Clone)]
pub(crate) struct JobControl(Arc<OwnedHandle>, Arc<Mutex<completion::Members>>);

impl JobControl {
    #[cfg(all(test, not(feature = "recorder-worker")))]
    pub(crate) fn member_ids(&self) -> Result<Vec<u32>> {
        completion::members(handle(&self.0))
    }
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn same_job(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(crate) fn terminate(&self) -> Result<()> {
        let mut members = self.1.lock().unwrap_or_else(|error| error.into_inner());
        let captured = members.observe(handle(&self.0));
        unsafe { TerminateJobObject(handle(&self.0), 1) }.context("terminate process job")?;
        captured
    }

    pub(crate) fn active_processes(&self) -> Result<u32> {
        let mut members = self.1.lock().unwrap_or_else(|error| error.into_inner());
        members.observe(handle(&self.0))?;
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        unsafe {
            QueryInformationJobObject(
                Some(handle(&self.0)),
                JobObjectBasicAccountingInformation,
                (&raw mut info).cast::<c_void>(),
                std::mem::size_of_val(&info) as u32,
                None,
            )
        }
        .context("query command process job")?;
        Ok(info.ActiveProcesses.max(members.pending()?))
    }
}

fn handle(value: &OwnedHandle) -> HANDLE {
    HANDLE(value.as_raw_handle())
}

fn create_job() -> Result<OwnedHandle> {
    let raw =
        unsafe { CreateJobObjectW(None, PCWSTR::null()) }.context("create command process job")?;
    let job = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    set_kill_on_close(&job, true)?;
    Ok(job)
}

fn set_kill_on_close(job: &OwnedHandle, enabled: bool) -> Result<()> {
    set_limits(
        job,
        if enabled {
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        } else {
            JOB_OBJECT_LIMIT(0)
        },
    )
}

/// Kill-on-close that still lets a child created with an explicit
/// `CREATE_BREAKAWAY_FROM_JOB` leave (never silently): a diagnostic observer
/// must outlive the tree it reports on. Every other descendant stays contained.
fn permit_explicit_breakaway(job: &OwnedHandle) -> Result<()> {
    set_limits(
        job,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    )
}

fn set_limits(job: &OwnedHandle, flags: JOB_OBJECT_LIMIT) -> Result<()> {
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = flags;
    unsafe {
        SetInformationJobObject(
            handle(job),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast::<c_void>(),
            std::mem::size_of_val(&limits) as u32,
        )
    }
    .context("configure command process job")
}

#[cfg(not(feature = "recorder-worker"))]
fn find_primary_thread(pid: u32) -> Result<OwnedHandle> {
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }
        .context("snapshot command process threads")?;
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    unsafe { Thread32First(handle(&snapshot), &mut entry) }.context("enumerate process threads")?;
    let mut thread_id = None;
    loop {
        if entry.th32OwnerProcessID == pid && thread_id.replace(entry.th32ThreadID).is_some() {
            bail!("suspended command process has multiple threads");
        }
        if let Err(error) = unsafe { Thread32Next(handle(&snapshot), &mut entry) } {
            if error.code() != HRESULT::from_win32(ERROR_NO_MORE_FILES.0) {
                return Err(error).context("continue enumerating command threads");
            }
            break;
        }
    }
    let thread_id = thread_id.ok_or_else(|| anyhow!("suspended command thread was not found"))?;
    let raw = unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, thread_id) }
        .context("open suspended command thread")?;
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.0) })
}
