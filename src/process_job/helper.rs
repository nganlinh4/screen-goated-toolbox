//! Atomic job-list launch with checked configuration and captured output.
use super::{JobControl, handle};
use anyhow::{Context, Result, ensure};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::os::windows::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use windows::Win32::Foundation::{
    HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
    InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
};
use windows::core::{PCWSTR, PWSTR};

pub(crate) struct HelperCommand {
    executable: PathBuf,
    batch_script: Option<PathBuf>,
    arguments: Vec<OsString>,
    directory: Option<PathBuf>,
    environment: super::environment::Changes,
    inherit_environment: bool,
    piped_input: bool,
    piped_output: bool,
    piped_error: bool,
    explicit_breakaway: bool,
}
impl HelperCommand {
    pub(crate) fn new(executable: impl AsRef<Path>) -> Self {
        Self {
            executable: executable.as_ref().to_owned(),
            batch_script: None,
            arguments: Vec::new(),
            directory: None,
            environment: Vec::new(),
            inherit_environment: true,
            piped_input: false,
            piped_output: true,
            piped_error: true,
            explicit_breakaway: false,
        }
    }
    /// Run a `.bat`/`.cmd` script through `command_processor` (an absolute
    /// `cmd.exe`) with the same command line Rust std's `Command` builds.
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn batch_script(
        command_processor: impl AsRef<Path>,
        script: impl AsRef<Path>,
    ) -> Self {
        let mut command = Self::new(command_processor);
        command.batch_script = Some(script.as_ref().to_owned());
        command
    }
    pub(crate) fn args(
        &mut self,
        values: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> &mut Self {
        self.arguments
            .extend(values.into_iter().map(|value| value.as_ref().to_owned()));
        self
    }
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn arg(&mut self, value: impl AsRef<OsStr>) -> &mut Self {
        self.arguments.push(value.as_ref().to_owned());
        self
    }
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn current_dir(&mut self, directory: impl AsRef<Path>) -> &mut Self {
        self.directory = Some(directory.as_ref().to_owned());
        self
    }
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn env(&mut self, name: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.environment
            .push((name.as_ref().to_owned(), Some(value.as_ref().to_owned())));
        self
    }
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn env_remove(&mut self, name: impl AsRef<OsStr>) -> &mut Self {
        self.environment.push((name.as_ref().to_owned(), None));
        self
    }
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn env_clear(&mut self) -> &mut Self {
        self.inherit_environment = false;
        self.environment.clear();
        self
    }
    pub(crate) fn pipe_stdin(&mut self) -> &mut Self {
        self.piped_input = true;
        self
    }
    #[cfg(any(test, feature = "recorder-worker"))]
    pub(crate) fn stdout_null(&mut self) -> &mut Self {
        self.piped_output = false;
        self
    }
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn stderr_null(&mut self) -> &mut Self {
        self.piped_error = false;
        self
    }
    /// Let a descendant that explicitly asks for `CREATE_BREAKAWAY_FROM_JOB`
    /// leave the helper's job; descendants that do not ask stay contained.
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn permit_explicit_breakaway(&mut self) -> &mut Self {
        self.explicit_breakaway = true;
        self
    }
    pub(super) fn permits_explicit_breakaway(&self) -> bool {
        self.explicit_breakaway
    }
}

pub(super) struct Child {
    pub(super) process: OwnedHandle,
    #[cfg(not(feature = "recorder-worker"))]
    pub(super) id: u32,
    pub(super) stdin: Option<File>,
    pub(super) stdout: Option<File>,
    pub(super) stderr: Option<File>,
}
impl Child {
    pub(super) fn try_wait(&self) -> std::io::Result<Option<ExitStatus>> {
        match unsafe { WaitForSingleObject(handle(&self.process), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut status = 0;
                unsafe { GetExitCodeProcess(handle(&self.process), &mut status) }?;
                Ok(Some(ExitStatus::from_raw(status)))
            }
            _ => Err(std::io::Error::last_os_error()),
        }
    }
}

struct Attributes(Vec<usize>);
impl Attributes {
    fn pointer(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        LPPROC_THREAD_ATTRIBUTE_LIST(self.0.as_mut_ptr().cast())
    }
    fn new() -> Result<Self> {
        let mut size = 0;
        let _ = unsafe { InitializeProcThreadAttributeList(None, 2, None, &mut size) };
        ensure!(size > 0, "Process attribute allocation is unavailable");
        let mut storage = vec![0_usize; size.div_ceil(std::mem::size_of::<usize>())];
        let pointer = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
        unsafe {
            InitializeProcThreadAttributeList(Some(pointer), 2, None, &mut size)?;
        }
        Ok(Self(storage))
    }
    fn set(&mut self, attribute: u32, values: &[HANDLE]) -> Result<()> {
        unsafe {
            UpdateProcThreadAttribute(
                self.pointer(),
                0,
                attribute as usize,
                Some(values.as_ptr().cast()),
                std::mem::size_of_val(values),
                None,
                None,
            )?;
        }
        Ok(())
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.pointer());
        }
    }
}

fn pipe(security: &SECURITY_ATTRIBUTES, input: bool) -> Result<(File, OwnedHandle)> {
    let (mut read, mut write) = (HANDLE::default(), HANDLE::default());
    unsafe {
        CreatePipe(&mut read, &mut write, Some(security), 0)?;
    }
    let read = unsafe { OwnedHandle::from_raw_handle(read.0) };
    let write = unsafe { OwnedHandle::from_raw_handle(write.0) };
    let (parent, child) = if input { (write, read) } else { (read, write) };
    unsafe {
        SetHandleInformation(handle(&parent), HANDLE_FLAG_INHERIT.0, Default::default())?;
    }
    Ok((File::from(parent), child))
}

fn input(security: &SECURITY_ATTRIBUTES, piped: bool) -> Result<(Option<File>, OwnedHandle)> {
    if piped {
        let (parent, child) = pipe(security, true)?;
        return Ok((Some(parent), child));
    }
    Ok((None, null_handle(security, FILE_GENERIC_READ.0)?))
}

fn output_pipe(security: &SECURITY_ATTRIBUTES, piped: bool) -> Result<(Option<File>, OwnedHandle)> {
    if piped {
        let (parent, child) = pipe(security, false)?;
        return Ok((Some(parent), child));
    }
    Ok((None, null_handle(security, FILE_GENERIC_WRITE.0)?))
}

fn null_handle(security: &SECURITY_ATTRIBUTES, access: u32) -> Result<OwnedHandle> {
    let null: Vec<_> = "NUL\0".encode_utf16().collect();
    let input = unsafe {
        CreateFileW(
            PCWSTR(null.as_ptr()),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            Some(security),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )?
    };
    Ok(unsafe { OwnedHandle::from_raw_handle(input.0) })
}

fn quoted(value: &OsStr, output: &mut Vec<u16>) -> Result<()> {
    let units: Vec<_> = value.encode_wide().collect();
    ensure!(
        !units.contains(&0),
        "Helper argument contains a null character"
    );
    output.push(b'"' as u16);
    let mut slashes = 0;
    for unit in units {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            b'\\' as u16,
            if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        output.push(unit);
        slashes = 0;
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    output.push(b'"' as u16);
    Ok(())
}

pub(super) fn spawn(command: &HelperCommand, job: &JobControl) -> Result<(Child, OwnedHandle)> {
    ensure!(
        command.executable.is_absolute(),
        "Helper executable must be absolute"
    );
    let mut executable: Vec<_> = command.executable.as_os_str().encode_wide().collect();
    ensure!(
        !executable.contains(&0),
        "Helper executable contains a null character"
    );
    executable.push(0);
    let environment =
        super::environment::prepare(&command.environment, command.inherit_environment)?;
    let directory = command
        .directory
        .as_ref()
        .map(|path| -> Result<Vec<u16>> {
            ensure!(path.is_absolute(), "Helper directory must be absolute");
            let mut units: Vec<_> = path.as_os_str().encode_wide().collect();
            ensure!(
                !units.contains(&0),
                "Helper directory contains a null character"
            );
            units.push(0);
            Ok(units)
        })
        .transpose()?;
    let mut line = match &command.batch_script {
        Some(script) => super::batch_line::command_line(script.as_os_str(), &command.arguments)?,
        None => {
            let mut line = Vec::new();
            quoted(command.executable.as_os_str(), &mut line)?;
            for argument in &command.arguments {
                line.push(b' ' as u16);
                quoted(argument, &mut line)?;
            }
            line
        }
    };
    ensure!(
        line.len() < 32767,
        "Helper command exceeds the native limit"
    );
    line.push(0);
    let security = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        bInheritHandle: true.into(),
        ..Default::default()
    };
    let (stdout, output) = output_pipe(&security, command.piped_output)?;
    let (stderr, error) = output_pipe(&security, command.piped_error)?;
    let (stdin, input) = input(&security, command.piped_input)?;
    let handles = [handle(&input), handle(&output), handle(&error)];
    let mut jobs = Vec::new();
    if let Some(parent) = super::policy::parent_job() {
        jobs.push(handle(&parent.0));
    }
    jobs.push(handle(&job.0));
    let mut attributes = Attributes::new()?;
    attributes.set(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &handles)?;
    attributes.set(PROC_THREAD_ATTRIBUTE_JOB_LIST, &jobs)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes.pointer();
    let mut process = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            PCWSTR(executable.as_ptr()),
            Some(PWSTR(line.as_mut_ptr())),
            None,
            None,
            true,
            CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | EXTENDED_STARTUPINFO_PRESENT
                | CREATE_UNICODE_ENVIRONMENT,
            environment.as_ref().map(|block| block.as_ptr().cast()),
            directory
                .as_ref()
                .map_or(PCWSTR::null(), |path| PCWSTR(path.as_ptr())),
            &startup.StartupInfo,
            &mut process,
        )
    }
    .context("Launch contained helper atomically")?;
    // From this point the OS owns both job assignments, even if the creator dies.
    Ok((
        Child {
            process: unsafe { OwnedHandle::from_raw_handle(process.hProcess.0) },
            #[cfg(not(feature = "recorder-worker"))]
            id: process.dwProcessId,
            stdin,
            stdout,
            stderr,
        },
        unsafe { OwnedHandle::from_raw_handle(process.hThread.0) },
    ))
}
