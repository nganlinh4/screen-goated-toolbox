use super::CHILD_FLAG;
use super::protocol::{ChildEvent, HostCommand, RendererFailureKind};
use anyhow::Context;
use std::ffi::{OsString, c_void};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, Once};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
use windows::core::PCWSTR;

const HEARTBEAT_TIMEOUT_MS: u64 = 5_000;
const STARTUP_TIMEOUT_MS: u64 = 15_000;
const STABLE_GENERATION_MS: u64 = 30_000;
const INITIAL_RESTART_DELAY_MS: u64 = 250;
const MAX_RESTART_DELAY_MS: u64 = 30_000;
const GPU_FAILURE_WINDOW_MS: u64 = 60_000;
const GPU_FAILURE_THRESHOLD: u32 = 3;
const SOFTWARE_RENDERING_ARGUMENT: &str = "--disable-gpu";

struct RendererProcess {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    generation: u64,
    _job: OwnedHandle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProcessState {
    Running,
    Spawned,
    Unavailable,
}

static PROCESS: LazyLock<Mutex<Option<RendererProcess>>> = LazyLock::new(|| Mutex::new(None));
static DESIRED: AtomicBool = AtomicBool::new(false);
static STARTING: AtomicBool = AtomicBool::new(false);
static TRANSITIONING: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static LIVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static LIVE_PID: AtomicU32 = AtomicU32::new(0);
static READY_GENERATION: AtomicU64 = AtomicU64::new(0);
static LAST_HEARTBEAT_MS: AtomicU64 = AtomicU64::new(0);
static READY_SINCE_MS: AtomicU64 = AtomicU64::new(0);
static RESTART_FAILURES: AtomicU32 = AtomicU32::new(0);
static RESTART_NOT_BEFORE_MS: AtomicU64 = AtomicU64::new(0);
static GPU_FAILURES: AtomicU32 = AtomicU32::new(0);
static LAST_GPU_FAILURE_MS: AtomicU64 = AtomicU64::new(0);
static SOFTWARE_RENDERING_REQUIRED: AtomicBool = AtomicBool::new(false);
static WATCHDOG: Once = Once::new();
static MONOTONIC_EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

pub(super) fn request_process() {
    DESIRED.store(true, Ordering::SeqCst);
    start_watchdog();
}

pub(crate) fn wait_until_ready(timeout: Duration) -> bool {
    super::parent::warmup();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let live = LIVE_GENERATION.load(Ordering::SeqCst);
        if live != 0 && READY_GENERATION.load(Ordering::SeqCst) == live {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

pub(crate) fn restart_and_wait(timeout: Duration) -> bool {
    let previous_generation = GENERATION.load(Ordering::SeqCst);
    super::parent::request_restart();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let live = LIVE_GENERATION.load(Ordering::SeqCst);
        if live > previous_generation && READY_GENERATION.load(Ordering::SeqCst) == live {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

pub(super) fn ensure_process() -> ProcessState {
    request_process();
    if LIVE_GENERATION.load(Ordering::SeqCst) != 0 && PROCESS.lock().unwrap().is_some() {
        return ProcessState::Running;
    }
    if now_ms() < RESTART_NOT_BEFORE_MS.load(Ordering::SeqCst)
        || STARTING.swap(true, Ordering::SeqCst)
    {
        return ProcessState::Unavailable;
    }
    let state = match spawn_process() {
        Ok(()) => ProcessState::Spawned,
        Err(error) => {
            schedule_restart_backoff();
            crate::log_info!("[RealtimeCompositor] failed to start renderer: {error:#}");
            ProcessState::Unavailable
        }
    };
    STARTING.store(false, Ordering::SeqCst);
    state
}

fn spawn_process() -> anyhow::Result<()> {
    let inherited_arguments = std::env::var_os("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS");
    let fallback_required = SOFTWARE_RENDERING_REQUIRED.load(Ordering::SeqCst);
    let software_rendering = fallback_required
        || inherited_arguments
            .as_deref()
            .is_some_and(has_software_rendering_argument);
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg(CHILD_FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if fallback_required {
        command.env(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            browser_arguments_with_software_fallback(inherited_arguments),
        );
    }
    let mut child = command.spawn()?;
    let initialized = initialize_child(&mut child);
    let (job, stdin, stdout, stderr) = match initialized {
        Ok(initialized) => initialized,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let pid = child.id();
    LIVE_GENERATION.store(generation, Ordering::SeqCst);
    LIVE_PID.store(pid, Ordering::SeqCst);
    READY_GENERATION.store(0, Ordering::SeqCst);
    READY_SINCE_MS.store(0, Ordering::SeqCst);
    LAST_HEARTBEAT_MS.store(now_ms(), Ordering::SeqCst);
    *PROCESS.lock().unwrap() = Some(RendererProcess {
        child,
        stdin: BufWriter::new(stdin),
        generation,
        _job: job,
    });
    std::thread::spawn(move || read_events(stdout, generation));
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            crate::log_info!("[RealtimeCompositor] child generation={generation}: {line}");
        }
    });
    crate::log_info!(
        "[RealtimeCompositor] renderer spawned generation={generation} pid={pid} software_rendering={software_rendering}"
    );
    Ok(())
}

fn initialize_child(
    child: &mut Child,
) -> anyhow::Result<(
    OwnedHandle,
    ChildStdin,
    std::process::ChildStdout,
    std::process::ChildStderr,
)> {
    let job = create_kill_on_close_job(child)?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("realtime renderer stdin was not created"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("realtime renderer stdout was not created"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("realtime renderer stderr was not created"))?;
    Ok((job, stdin, stdout, stderr))
}

fn create_kill_on_close_job(child: &Child) -> anyhow::Result<OwnedHandle> {
    let raw = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
        .context("create realtime compositor job")?;
    let job = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        SetInformationJobObject(
            HANDLE(job.as_raw_handle()),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast::<c_void>(),
            std::mem::size_of_val(&limits) as u32,
        )
        .context("configure realtime compositor job")?;
        AssignProcessToJobObject(HANDLE(job.as_raw_handle()), HANDLE(child.as_raw_handle()))
            .context("contain realtime compositor process")?;
    }
    Ok(job)
}

pub(super) fn write_command(command: &HostCommand) -> anyhow::Result<()> {
    let mut process = PROCESS.lock().unwrap();
    let renderer = process
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("realtime compositor is unavailable"))?;
    write_command_to(&mut renderer.stdin, command)
}

fn write_command_to(
    writer: &mut BufWriter<ChildStdin>,
    command: &HostCommand,
) -> anyhow::Result<()> {
    serde_json::to_writer(&mut *writer, command)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn read_events(stdout: std::process::ChildStdout, generation: u64) {
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let event = match serde_json::from_str::<ChildEvent>(&line) {
            Ok(event) => event,
            Err(error) => {
                crate::log_info!(
                    "[RealtimeCompositor] invalid child event generation={generation} bytes={} error={error}",
                    line.len()
                );
                continue;
            }
        };
        if LIVE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        match event {
            ChildEvent::Ready => renderer_ready(generation),
            ChildEvent::Heartbeat => renderer_heartbeat(),
            ChildEvent::ResyncRequested => super::parent::queue_snapshot(),
            ChildEvent::RendererFailure { kind } => handle_renderer_failure(generation, kind),
            other => super::parent::handle_child_event(other),
        }
    }
    fail_generation(generation, "renderer disconnected", false);
}

fn renderer_ready(generation: u64) {
    let now = now_ms();
    LAST_HEARTBEAT_MS.store(now, Ordering::SeqCst);
    READY_SINCE_MS.store(now, Ordering::SeqCst);
    READY_GENERATION.store(generation, Ordering::SeqCst);
    crate::log_info!("[RealtimeCompositor] renderer ready generation={generation}");
}

fn renderer_heartbeat() {
    let now = now_ms();
    LAST_HEARTBEAT_MS.store(now, Ordering::SeqCst);
    if now.saturating_sub(READY_SINCE_MS.load(Ordering::SeqCst)) >= STABLE_GENERATION_MS {
        RESTART_FAILURES.store(0, Ordering::SeqCst);
        RESTART_NOT_BEFORE_MS.store(0, Ordering::SeqCst);
        if now.saturating_sub(LAST_GPU_FAILURE_MS.load(Ordering::SeqCst)) >= STABLE_GENERATION_MS {
            GPU_FAILURES.store(0, Ordering::SeqCst);
        }
    }
}

fn handle_renderer_failure(generation: u64, kind: RendererFailureKind) {
    if kind == RendererFailureKind::GpuProcessExited {
        let now = now_ms();
        let previous_at = LAST_GPU_FAILURE_MS.swap(now, Ordering::SeqCst);
        let previous_count = GPU_FAILURES.load(Ordering::SeqCst);
        let (count, fallback) = gpu_failure_state(previous_at, previous_count, now);
        GPU_FAILURES.store(count, Ordering::SeqCst);
        if fallback {
            SOFTWARE_RENDERING_REQUIRED.store(true, Ordering::SeqCst);
            fail_generation(generation, "repeated GPU process failures", true);
        }
        return;
    }
    fail_generation(generation, kind.as_str(), true);
}

fn start_watchdog() {
    WATCHDOG.call_once(|| {
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if !DESIRED.load(Ordering::SeqCst) {
                    continue;
                }
                let live = LIVE_GENERATION.load(Ordering::SeqCst);
                if crate::overlay::compositor_process::watchdog_should_restart(
                    DESIRED.load(Ordering::SeqCst),
                    TRANSITIONING.load(Ordering::SeqCst),
                    live,
                ) {
                    super::parent::request_restart();
                    continue;
                }
                if live == 0 {
                    continue;
                }
                let timeout = if READY_GENERATION.load(Ordering::SeqCst) == live {
                    HEARTBEAT_TIMEOUT_MS
                } else {
                    STARTUP_TIMEOUT_MS
                };
                if now_ms().saturating_sub(LAST_HEARTBEAT_MS.load(Ordering::SeqCst)) > timeout {
                    fail_generation(live, "heartbeat timed out", true);
                }
            }
        });
    });
}

pub(super) fn fail_live_renderer(reason: &str, terminate: bool) {
    let generation = LIVE_GENERATION.load(Ordering::SeqCst);
    if generation != 0 {
        fail_generation(generation, reason, terminate);
    } else {
        super::parent::request_restart();
    }
}

fn fail_generation(generation: u64, reason: &str, terminate: bool) {
    if LIVE_GENERATION.load(Ordering::SeqCst) != generation {
        return;
    }
    if terminate {
        terminate_generation(generation);
    }
    if LIVE_GENERATION
        .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    LIVE_PID.store(0, Ordering::SeqCst);
    READY_GENERATION.store(0, Ordering::SeqCst);
    READY_SINCE_MS.store(0, Ordering::SeqCst);
    schedule_restart_backoff();
    crate::log_info!(
        "[RealtimeCompositor] renderer failed generation={generation} reason={reason}"
    );
    super::parent::request_restart();
}

fn terminate_generation(generation: u64) {
    let pid = LIVE_PID.load(Ordering::SeqCst);
    if pid == 0 || LIVE_GENERATION.load(Ordering::SeqCst) != generation {
        return;
    }
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) else {
            return;
        };
        if LIVE_GENERATION.load(Ordering::SeqCst) == generation
            && LIVE_PID.load(Ordering::SeqCst) == pid
        {
            let _ = TerminateProcess(handle, 0x5354_4703);
        }
        let _ = CloseHandle(handle);
    }
}

fn schedule_restart_backoff() {
    let failure = RESTART_FAILURES.fetch_add(1, Ordering::SeqCst) + 1;
    RESTART_NOT_BEFORE_MS.fetch_max(
        now_ms().saturating_add(restart_delay_ms(failure)),
        Ordering::SeqCst,
    );
}

fn restart_delay_ms(failure: u32) -> u64 {
    let shift = failure.saturating_sub(1).min(7);
    INITIAL_RESTART_DELAY_MS
        .saturating_mul(1_u64 << shift)
        .min(MAX_RESTART_DELAY_MS)
}

pub(super) fn restart_now() -> ProcessState {
    TRANSITIONING.store(true, Ordering::SeqCst);
    let mut old = PROCESS.lock().unwrap().take();
    if let Some(renderer) = old.as_mut() {
        LIVE_GENERATION
            .compare_exchange(renderer.generation, 0, Ordering::SeqCst, Ordering::SeqCst)
            .ok();
        LIVE_PID.store(0, Ordering::SeqCst);
        READY_GENERATION.store(0, Ordering::SeqCst);
        let _ = write_command_to(&mut renderer.stdin, &HostCommand::Shutdown);
        crate::overlay::compositor_process::wait_for_exit_or_kill(&mut renderer.child);
    }
    drop(old);
    let state = ensure_process();
    TRANSITIONING.store(false, Ordering::SeqCst);
    state
}

pub(super) fn shutdown_for_exit() {
    DESIRED.store(false, Ordering::SeqCst);
    TRANSITIONING.store(true, Ordering::SeqCst);
    let mut renderer = PROCESS.lock().unwrap().take();
    LIVE_GENERATION.store(0, Ordering::SeqCst);
    LIVE_PID.store(0, Ordering::SeqCst);
    READY_GENERATION.store(0, Ordering::SeqCst);
    if let Some(renderer) = renderer.as_mut() {
        let _ = write_command_to(&mut renderer.stdin, &HostCommand::Shutdown);
        crate::overlay::compositor_process::wait_for_exit_or_kill(&mut renderer.child);
    }
}

fn browser_arguments_with_software_fallback(existing: Option<OsString>) -> OsString {
    let mut arguments = existing.unwrap_or_default();
    if !has_software_rendering_argument(&arguments) {
        if !arguments.is_empty() {
            arguments.push(" ");
        }
        arguments.push(SOFTWARE_RENDERING_ARGUMENT);
    }
    arguments
}

fn has_software_rendering_argument(arguments: &std::ffi::OsStr) -> bool {
    arguments
        .to_string_lossy()
        .split_whitespace()
        .any(|argument| argument == SOFTWARE_RENDERING_ARGUMENT)
}

fn gpu_failure_state(previous_at: u64, previous_count: u32, now: u64) -> (u32, bool) {
    let count = if now.saturating_sub(previous_at) <= GPU_FAILURE_WINDOW_MS {
        previous_count.saturating_add(1)
    } else {
        1
    };
    (count, count >= GPU_FAILURE_THRESHOLD)
}

fn now_ms() -> u64 {
    MONOTONIC_EPOCH.elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_backoff_is_exponential_and_bounded() {
        assert_eq!(restart_delay_ms(1), 250);
        assert_eq!(restart_delay_ms(3), 1_000);
        assert_eq!(restart_delay_ms(u32::MAX), 30_000);
    }

    #[test]
    fn repeated_gpu_failures_request_software_rendering() {
        assert_eq!(gpu_failure_state(1_000, 1, 2_000), (2, false));
        assert_eq!(gpu_failure_state(2_000, 2, 3_000), (3, true));
        assert_eq!(gpu_failure_state(1_000, 3, 70_000), (1, false));
    }

    #[test]
    fn child_is_contained_by_a_kill_on_close_job() {
        let source = include_str!("supervisor.rs");
        assert!(source.contains("JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE"));
        assert!(source.contains("AssignProcessToJobObject"));
    }
}
