use super::{storage, wide};
use anyhow::{Context, Result, bail};
use std::fs::OpenOptions;
use std::os::windows::fs::FileExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use windows::Win32::Foundation::{FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Diagnostics::Debug::{
    EXCEPTION_POINTERS, MINIDUMP_EXCEPTION_INFORMATION, MiniDumpNormal, MiniDumpWithThreadInfo,
    MiniDumpWithUnloadedModules, MiniDumpWriteDump,
};
use windows::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NO_WINDOW, CreateEventW, EVENT_MODIFY_STATE,
    GetCurrentProcess, GetCurrentThreadId, GetExitCodeProcess, GetProcessTimes, INFINITE,
    OpenEventW, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, PROCESS_VM_READ, SYNCHRONIZATION_SYNCHRONIZE, SetEvent,
    WaitForMultipleObjects, WaitForSingleObject,
};
use windows::core::{BOOL, PCWSTR};

pub(super) const FLAG: &str = "--internal-crash-monitor";
const SNAPSHOT_TIMEOUT_MS: u32 = 10_000;
pub(super) const PANIC_DUMP: &str = "panic.dmp";
pub(super) const LATEST_PANIC_DUMP: &str = "panic-latest.dmp";

pub(super) struct Client {
    request: OwnedHandle,
    done: OwnedHandle,
    /// Opened at startup: the exception filter may run with a corrupted heap,
    /// so a request must not allocate, open files or flush to disk.
    request_file: std::fs::File,
    panic_dumps: AtomicU32,
    native_dumped: AtomicBool,
    busy: std::sync::Mutex<()>,
}

impl Client {
    pub fn request_dump(&self, exception: usize) {
        let Ok(_guard) = self.busy.try_lock() else {
            return;
        };
        // A caught panic must not use up the snapshot a later fatal one needs:
        // the observer keeps the first panic dump and replaces a second slot
        // with the newest, so the budget only bounds a panic storm.
        let allowed = if exception == 0 {
            self.panic_dumps.fetch_add(1, Ordering::Relaxed) < super::MAX_PANIC_REPORTS
        } else {
            !self.native_dumped.swap(true, Ordering::Relaxed)
        };
        if !allowed {
            return;
        }
        let request = encode_request(unsafe { GetCurrentThreadId() }, exception);
        if !matches!(self.request_file.seek_write(&request, 0), Ok(REQUEST_LEN)) {
            return;
        }
        unsafe {
            if SetEvent(handle(&self.request)).is_ok() {
                let _ = WaitForSingleObject(handle(&self.done), SNAPSHOT_TIMEOUT_MS);
            }
        }
    }
}

fn event(session: &str, suffix: &str, create: bool) -> Result<OwnedHandle> {
    let name = wide(&format!("Local\\SGTCrash-{session}-{suffix}"));
    let raw = unsafe {
        if create {
            CreateEventW(None, false, false, PCWSTR(name.as_ptr()))
        } else {
            OpenEventW(
                EVENT_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE,
                false,
                PCWSTR(name.as_ptr()),
            )
        }
    }
    .context("open diagnostic event")?;
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.0) })
}

pub(super) fn start(identity: &storage::Identity, directory: &Path) -> Result<Client> {
    let session = identity.session();
    let request = event(&session, "request", true)?;
    let done = event(&session, "done", true)?;
    let mut command = Command::new(std::env::current_exe()?);
    #[cfg(not(test))]
    command.arg(FLAG).arg(&session);
    #[cfg(test)]
    command
        .args([
            "--exact",
            "crash_report::tests::monitor_child",
            "--ignored",
            "--nocapture",
        ])
        .env("SGT_TEST_MONITOR_SESSION", &session);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = spawn_outside_job(&mut command).context("start diagnostic observer")?;
    if let Err(error) = await_acknowledgement(&done, &child, SNAPSHOT_TIMEOUT_MS) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    // Dropping Child keeps the independent observer alive. It owns a real
    // process handle before startup completes, so PID reuse cannot misattribute
    // an exit. No inherited protocol handles keep renderer pipes open.
    drop(child);
    Ok(Client {
        request,
        done,
        request_file: OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(directory.join(REQUEST_FILE))
            .context("open crash snapshot request")?,
        panic_dumps: AtomicU32::new(0),
        native_dumped: AtomicBool::new(false),
        busy: std::sync::Mutex::new(()),
    })
}

/// Startup continues only once the observer holds this process's handle, so
/// even an immediate exit is observed. An observer that exits first cannot
/// acknowledge, so stop waiting then instead of stalling for the full timeout.
fn await_acknowledgement(
    done: &OwnedHandle,
    observer: &std::process::Child,
    timeout_ms: u32,
) -> Result<()> {
    let observer = HANDLE(observer.as_raw_handle());
    // `done` is first: an acknowledgement counts even if the observer then exited.
    match unsafe { WaitForMultipleObjects(&[handle(done), observer], false, timeout_ms) } {
        value if value == WAIT_OBJECT_0 => Ok(()),
        value if value.0 == WAIT_OBJECT_0.0 + 1 => {
            let mut code = 0;
            unsafe { GetExitCodeProcess(observer, &mut code) }?;
            bail!("diagnostic observer exited before acknowledging startup (0x{code:08X})")
        }
        WAIT_TIMEOUT => bail!("diagnostic observer did not acknowledge startup"),
        value => bail!("diagnostic observer wait failed: {}", value.0),
    }
}

/// The observer must outlive a kill-on-close job that its process is torn down
/// with, or it cannot record that exit. A job that does not allow breakaway
/// rejects the request, and the observer then starts inside it as before.
fn spawn_outside_job(command: &mut Command) -> std::io::Result<std::process::Child> {
    const ERROR_ACCESS_DENIED: i32 = 5;
    command.creation_flags(CREATE_NO_WINDOW.0 | CREATE_BREAKAWAY_FROM_JOB.0);
    match command.spawn() {
        Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED) => {
            command.creation_flags(CREATE_NO_WINDOW.0);
            command.spawn()
        }
        result => result,
    }
}

pub(crate) fn run_if_requested() -> Option<i32> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().is_none_or(|argument| argument != FLAG) {
        return None;
    }
    let result = arguments
        .next()
        .context("missing diagnostic session")
        .and_then(|session| {
            session
                .into_string()
                .map_err(|_| anyhow::anyhow!("invalid diagnostic session"))
        })
        .and_then(|session| run(&session));
    Some(if let Err(error) = result {
        let _ = std::io::Write::write_fmt(
            &mut std::io::stderr(),
            format_args!("diagnostic observer: {error:#}\n"),
        );
        1
    } else {
        0
    })
}

pub(super) fn run(session: &str) -> Result<()> {
    if !session.starts_with("sgt-")
        || !session
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-' || b"sgt".contains(&byte))
    {
        bail!("invalid diagnostic session name");
    }
    let directory = storage::root().join(session);
    let identity = storage::read_identity(&directory)?;
    if identity.session() != session {
        bail!("diagnostic session identity mismatch");
    }
    let process = open_process(
        identity.pid,
        PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
    )?;
    if creation_time(handle(&process))? != identity.created {
        bail!("diagnostic process identity changed");
    }
    // Snapshot permissions may be unavailable even when exit observation is
    // allowed; retain exit status rather than disabling the whole observer.
    let dump_process = open_process(identity.pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ).ok();
    let request = event(session, "request", false)?;
    let done = event(session, "done", false)?;
    unsafe {
        SetEvent(handle(&done))?;
    }
    // Hashed only after startup is acknowledged, off the observed process's
    // critical path, and concurrently with any snapshot request.
    let executable = identity.executable.clone();
    let image_hash = std::thread::spawn(move || storage::image_sha256(&executable));
    loop {
        match unsafe {
            WaitForMultipleObjects(&[handle(&process), handle(&request)], false, INFINITE)
        } {
            value if value == WAIT_OBJECT_0 => break,
            value if value.0 == WAIT_OBJECT_0.0 + 1 => {
                let result = dump_process
                    .as_ref()
                    .context("snapshot process access unavailable")
                    .and_then(|process| snapshot(handle(process), identity.pid, &directory));
                let report = match result {
                    Ok(()) => "snapshot saved".to_owned(),
                    Err(error) => format!("snapshot failed: {error:#}"),
                };
                let _ = storage::write_report(&directory.join("snapshot.txt"), &report);
                unsafe {
                    SetEvent(handle(&done))?;
                }
            }
            value => bail!("diagnostic wait failed: {}", value.0),
        }
    }
    let mut code = 0;
    unsafe {
        GetExitCodeProcess(handle(&process), &mut code)?;
    }
    let sha256 = match image_hash.join() {
        Ok(Ok(hash)) => hash,
        Ok(Err(error)) => format!("unavailable ({error:#})"),
        Err(_) => "unavailable (hash thread panicked)".to_owned(),
    };
    let intended = directory.join("exit-intent.txt").exists();
    let report = format!(
        "{}\npid={} role={} version={} sha256={}\nexit_code=0x{code:08X}\nexit_intent_recorded={intended}\n{}\n",
        chrono::Local::now().to_rfc3339(),
        identity.pid,
        identity.role,
        identity.version,
        sha256,
        if intended {
            "Exit intent is recorded separately; compare it with the observed status."
        } else {
            "No exit intent was recorded; this is an unexplained exit, not proof of a crash."
        }
    );
    let _ = storage::append_event(
        &directory,
        &format!(
            "[ProcessExit] pid={} role={} exit_code=0x{code:08X} intent={intended} report={}",
            identity.pid,
            identity.role,
            directory.display()
        ),
    );
    // Keep the newest evidence; only old verified, inactive sessions are pruned.
    if let Err(error) = storage::prune(&storage::root()) {
        let _ = storage::append_event(&directory, &format!("retention deferred: {error:#}"));
    }
    // Publish exit.txt last: readers may release or remove isolated state as
    // soon as the complete exit record is visible. No later write may recreate it.
    storage::write_report(&directory.join("exit.txt"), &report)?;
    Ok(())
}

fn snapshot(process: HANDLE, pid: u32, directory: &Path) -> Result<()> {
    let (thread, pointer) = parse_request(&std::fs::read_to_string(directory.join(REQUEST_FILE))?)?;
    let path = if pointer == 0 {
        panic_dump_path(directory)?
    } else {
        directory.join("native.dmp")
    };
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    let exception = MINIDUMP_EXCEPTION_INFORMATION {
        ThreadId: thread,
        ExceptionPointers: pointer as *mut EXCEPTION_POINTERS,
        ClientPointers: BOOL(1),
    };
    let result = unsafe {
        MiniDumpWriteDump(
            process,
            pid,
            HANDLE(file.as_raw_handle()),
            MiniDumpNormal | MiniDumpWithThreadInfo | MiniDumpWithUnloadedModules,
            (pointer != 0).then_some(&raw const exception),
            None,
            None,
        )
    };
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error).context("capture process minidump");
    }
    file.sync_all().context("sync process minidump")?;
    if file.metadata()?.len() > 32 * 1024 * 1024 {
        drop(file);
        std::fs::remove_file(&path)?;
        bail!("minidump exceeded the 32 MiB retained-file limit; text evidence is preserved");
    }
    Ok(())
}

const REQUEST_FILE: &str = "snapshot.request";
/// A zero-padded `thread exception` line, so every request has one length and
/// overwrites the previous one in place.
pub(super) const REQUEST_LEN: usize = 32;

pub(super) fn encode_request(thread: u32, exception: usize) -> [u8; REQUEST_LEN] {
    fn digits(slot: &mut [u8], mut value: u64) {
        for byte in slot.iter_mut().rev() {
            *byte = b'0' + (value % 10) as u8;
            value /= 10;
        }
    }
    let mut request = [b' '; REQUEST_LEN];
    digits(&mut request[..10], u64::from(thread));
    digits(&mut request[11..31], exception as u64);
    request[REQUEST_LEN - 1] = b'\n';
    request
}

pub(super) fn parse_request(request: &str) -> Result<(u32, usize)> {
    let mut fields = request.split_whitespace();
    let thread = fields
        .next()
        .context("missing snapshot thread")?
        .parse::<u32>()?;
    let pointer = fields
        .next()
        .context("missing exception pointer")?
        .parse::<usize>()?;
    Ok((thread, pointer))
}

/// The first panic dump is kept as the origin; every later one replaces the
/// newest-panic slot, so disk use stays bounded at two panic dumps.
fn panic_dump_path(directory: &Path) -> Result<std::path::PathBuf> {
    let origin = directory.join(PANIC_DUMP);
    if !origin.exists() {
        return Ok(origin);
    }
    let latest = directory.join(LATEST_PANIC_DUMP);
    match std::fs::remove_file(&latest) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("replace newest panic minidump"),
    }
    Ok(latest)
}

fn open_process(
    pid: u32,
    access: windows::Win32::System::Threading::PROCESS_ACCESS_RIGHTS,
) -> Result<OwnedHandle> {
    let raw = unsafe { OpenProcess(access, false, pid) }.context("open diagnostic process")?;
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.0) })
}

fn handle(owned: &OwnedHandle) -> HANDLE {
    HANDLE(owned.as_raw_handle())
}

fn creation_time(process: HANDLE) -> Result<u64> {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user)?;
    }
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

pub(super) fn current_creation_time() -> Result<u64> {
    creation_time(unsafe { GetCurrentProcess() })
}

pub(super) fn process_may_be_alive(pid: u32, created: u64) -> bool {
    match open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE) {
        Ok(process) => creation_time(handle(&process)).map_or(true, |actual| {
            actual == created && unsafe { WaitForSingleObject(handle(&process), 0) } == WAIT_TIMEOUT
        }),
        // ERROR_INVALID_PARAMETER identifies a nonexistent PID. Access denied
        // and other uncertainty must never authorize deleting evidence.
        Err(error) => error
            .downcast_ref::<windows::core::Error>()
            .is_none_or(|error| error.code().0 as u32 != 0x80070057),
    }
}

#[cfg(test)]
mod handshake_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn startup_handshake_stops_waiting_once_the_observer_exits() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let done = event(&format!("sgt-handshake-{nonce}"), "done", true).unwrap();
        let mut observer = Command::new("cmd.exe")
            .args(["/D", "/C", "exit 7"])
            .creation_flags(CREATE_NO_WINDOW.0)
            .spawn()
            .unwrap();
        let started = Instant::now();
        let error = await_acknowledgement(&done, &observer, SNAPSHOT_TIMEOUT_MS).unwrap_err();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        assert!(format!("{error:#}").contains("(0x00000007)"), "{error:#}");
        observer.wait().unwrap();

        unsafe { SetEvent(handle(&done)).unwrap() };
        await_acknowledgement(&done, &observer, 0).unwrap();
        assert!(await_acknowledgement(&done, &observer, 0).is_err());
    }
}
