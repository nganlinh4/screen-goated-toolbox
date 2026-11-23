//! Retain process resources until both parent reaping and tree exit are proved.
use std::ops::{Deref, DerefMut};
#[cfg(not(feature = "recorder-worker"))]
use std::process::Command;
use std::sync::{Condvar, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(not(feature = "recorder-worker"))]
use super::Inheritance;
use super::{ContainedChild, JobControl};
use anyhow::{Result, anyhow, bail};

type Resources = Vec<Box<dyn Send>>;
static PENDING: LazyLock<(Mutex<Vec<Owner>>, Condvar)> =
    LazyLock::new(|| (Mutex::new(Vec::new()), Condvar::new()));
static REAPER: OnceLock<Result<(), String>> = OnceLock::new();

struct Owner {
    child: ContainedChild,
    jobs: Vec<JobControl>,
    _resources: Resources,
}
impl Owner {
    fn stopped(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
            && matches!(self.child.active_processes(), Ok(0))
            && self
                .jobs
                .iter()
                .all(|job| matches!(job.active_processes(), Ok(0)))
    }
    fn terminate(&self) -> Result<()> {
        let mut failure = None;
        for job in &self.jobs {
            if let Err(error) = job.terminate() {
                failure.get_or_insert(error);
            }
        }
        if let Err(error) = self.child.terminate() {
            failure.get_or_insert(error);
        }
        failure.map_or(Ok(()), Err)
    }
}

pub(crate) struct OwnedProcess(Option<Owner>);
impl OwnedProcess {
    pub(crate) fn spawn_helper(
        command: &super::HelperCommand,
        resources: impl Send + 'static,
    ) -> Result<Self> {
        ensure_reaper()?;
        let resources: Resources = vec![Box::new(resources)];
        let handle = super::create_job()?;
        if command.permits_explicit_breakaway() {
            super::permit_explicit_breakaway(&handle)?;
        }
        let job = JobControl(
            std::sync::Arc::new(handle),
            std::sync::Arc::new(Mutex::new(super::completion::Members::default())),
        );
        let (child, thread) = super::helper::spawn(command, &job)?;
        Ok(Self(Some(Owner {
            child: ContainedChild {
                child: super::child::Child::Helper(child),
                job,
                primary_thread: Some(thread),
                assigned: true,
            },
            jobs: Vec::new(),
            _resources: resources,
        })))
    }
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn spawn(command: &mut Command, resources: impl Send + 'static) -> Result<Self> {
        Self::spawn_with_policy(command, resources, Inheritance::Contained)
    }

    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn spawn_with_policy(
        command: &mut Command,
        resources: impl Send + 'static,
        inheritance: Inheritance,
    ) -> Result<Self> {
        let mut resources: Resources = vec![Box::new(resources)];
        let child = ContainedChild::spawn_retained(command, &mut resources, inheritance)?;
        Ok(Self(Some(Owner {
            child,
            jobs: Vec::new(),
            _resources: resources,
        })))
    }

    pub(crate) fn terminate(&self) -> Result<()> {
        self.0.as_ref().expect("process owner exists").terminate()
    }

    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn retain_job(&mut self, job: JobControl) {
        self.0
            .as_mut()
            .expect("process owner exists")
            .jobs
            .push(job);
    }

    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn group(&self) -> JobGroup {
        let owner = self.0.as_ref().expect("process owner exists");
        let mut jobs = owner.jobs.clone();
        jobs.push(owner.child.control());
        JobGroup(jobs)
    }

    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn retain(&mut self, resource: impl Send + 'static) {
        self.0
            .as_mut()
            .expect("process owner exists")
            ._resources
            .push(Box::new(resource));
    }

    pub(crate) fn stopped(&mut self) -> bool {
        self.0.as_mut().expect("process owner exists").stopped()
    }

    pub(crate) fn stop_within(&mut self, timeout: Duration) -> Result<()> {
        if self.stopped() {
            return Ok(());
        }
        self.terminate()?;
        let deadline = Instant::now() + timeout;
        loop {
            if self.stopped() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                bail!("Process tree cleanup is still pending");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Deref for OwnedProcess {
    type Target = ContainedChild;
    fn deref(&self) -> &Self::Target {
        &self.0.as_ref().expect("process owner exists").child
    }
}
impl DerefMut for OwnedProcess {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0.as_mut().expect("process owner exists").child
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if let Some(owner) = self.0.take() {
            retire(owner);
        }
    }
}

pub(super) fn ensure_reaper() -> Result<()> {
    REAPER
        .get_or_init(|| {
            std::thread::Builder::new()
                .name("sgt-process-reaper".into())
                .spawn(reap)
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| anyhow!("Process cleanup is unavailable: {error}"))?;
    Ok(())
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn retire_failed_launch(child: ContainedChild, resources: Resources) {
    retire(Owner {
        child,
        jobs: Vec::new(),
        _resources: resources,
    });
}

fn retire(mut owner: Owner) {
    if owner.stopped() {
        return;
    }
    let _ = owner.terminate();
    let (pending, wake) = &*PENDING;
    pending
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .push(owner);
    wake.notify_one();
}

fn reap() {
    let (pending, wake) = &*PENDING;
    loop {
        let owners = {
            let mut queue = pending.lock().unwrap_or_else(|error| error.into_inner());
            while queue.is_empty() {
                queue = wake.wait(queue).unwrap_or_else(|error| error.into_inner());
            }
            std::mem::take(&mut *queue)
        };
        let mut retained = Vec::new();
        for mut owner in owners {
            if !owner.stopped() {
                let _ = owner.terminate();
                retained.push(owner);
            }
        }
        if !retained.is_empty() {
            let mut queue = pending.lock().unwrap_or_else(|error| error.into_inner());
            queue.extend(retained);
            drop(
                wake.wait_timeout(queue, Duration::from_millis(100))
                    .unwrap_or_else(|error| error.into_inner()),
            );
        }
    }
}

#[cfg(not(feature = "recorder-worker"))]
#[derive(Clone)]
pub(crate) struct JobGroup(Vec<JobControl>);
#[cfg(not(feature = "recorder-worker"))]
impl JobGroup {
    pub(crate) fn terminate(&self) -> Result<()> {
        let mut failure = None;
        for job in &self.0 {
            if let Err(error) = job.terminate() {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }
}
