//! Creation process registration and shared resource-retaining tree ownership.
use crate::component_registry::ComponentLease;
use crate::process_job::{ContainedChild, JobControl, OwnedProcess};
use anyhow::Result;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::process::Command;
use std::sync::{LazyLock, Mutex};

static JOBS: LazyLock<Mutex<HashMap<u32, JobControl>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
struct Registration {
    pid: u32,
    control: JobControl,
}
impl Drop for Registration {
    fn drop(&mut self) {
        let mut jobs = JOBS.lock().unwrap_or_else(|error| error.into_inner());
        if jobs
            .get(&self.pid)
            .is_some_and(|control| control.same_job(&self.control))
        {
            jobs.remove(&self.pid);
        }
    }
}

pub(crate) struct CreationProcess(Option<OwnedProcess>);
impl CreationProcess {
    pub(crate) fn termination(&self) -> Termination {
        Termination {
            pid: self.id(),
            control: Some(self.control()),
        }
    }
    pub(crate) fn spawn(command: &mut Command, lease: ComponentLease) -> Result<Self> {
        let mut process = OwnedProcess::spawn(command, lease)?;
        let pid = process.id();
        let control = process.control();
        JOBS.lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(pid, control.clone());
        process.retain(Registration { pid, control });
        Ok(Self(Some(process)))
    }
}
impl Deref for CreationProcess {
    type Target = ContainedChild;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("owned process is present")
    }
}
impl DerefMut for CreationProcess {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().expect("owned process is present")
    }
}

pub(crate) fn terminate_owned(pid: u32) -> Option<Result<()>> {
    let control = JOBS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&pid)
        .cloned();
    control.map(|control| control.terminate())
}

/// A stable owned-job handle survives process exit and numeric PID reuse.
pub(crate) struct Termination {
    pid: u32,
    control: Option<JobControl>,
}

impl Termination {
    pub(crate) fn capture(pid: u32) -> Self {
        let control = JOBS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&pid)
            .cloned();
        Self { pid, control }
    }

    pub(crate) fn pid(&self) -> u32 {
        self.pid
    }

    pub(crate) fn terminate(self) {
        if let Some(control) = self.control
            && let Err(error) = control.terminate()
        {
            crate::log_info!("[Creation] Owned process termination failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests;
