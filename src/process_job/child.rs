//! Common handle and pipe interface for standard and atomic helper launches.
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
#[cfg(not(feature = "recorder-worker"))]
use std::process::Child as Standard;
use std::process::ExitStatus;
use windows::Win32::Foundation::HANDLE;

pub(crate) type Input = Box<dyn Write + Send>;
pub(crate) type Output = Box<dyn Read + Send>;

pub(super) enum Child {
    #[cfg(not(feature = "recorder-worker"))]
    Standard(Standard),
    Helper(super::helper::Child),
}

impl Child {
    pub(super) fn handle(&self) -> HANDLE {
        match self {
            #[cfg(not(feature = "recorder-worker"))]
            Self::Standard(child) => HANDLE(child.as_raw_handle()),
            Self::Helper(child) => HANDLE(child.process.as_raw_handle()),
        }
    }
    #[cfg(not(feature = "recorder-worker"))]
    pub(super) fn id(&self) -> u32 {
        match self {
            #[cfg(not(feature = "recorder-worker"))]
            Self::Standard(child) => child.id(),
            Self::Helper(child) => child.id,
        }
    }
    pub(super) fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        match self {
            #[cfg(not(feature = "recorder-worker"))]
            Self::Standard(child) => child.try_wait(),
            Self::Helper(child) => child.try_wait(),
        }
    }
    pub(super) fn take_stdin(&mut self) -> Option<Input> {
        match self {
            #[cfg(not(feature = "recorder-worker"))]
            Self::Standard(child) => child.stdin.take().map(|pipe| Box::new(pipe) as Input),
            Self::Helper(child) => child.stdin.take().map(|pipe| Box::new(pipe) as Input),
        }
    }
    pub(super) fn take_stdout(&mut self) -> Option<Output> {
        match self {
            #[cfg(not(feature = "recorder-worker"))]
            Self::Standard(child) => child.stdout.take().map(|pipe| Box::new(pipe) as Output),
            Self::Helper(child) => child.stdout.take().map(|pipe| Box::new(pipe) as Output),
        }
    }
    pub(super) fn take_stderr(&mut self) -> Option<Output> {
        match self {
            #[cfg(not(feature = "recorder-worker"))]
            Self::Standard(child) => child.stderr.take().map(|pipe| Box::new(pipe) as Output),
            Self::Helper(child) => child.stderr.take().map(|pipe| Box::new(pipe) as Output),
        }
    }
}
