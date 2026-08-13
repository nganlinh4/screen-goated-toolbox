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
pub(super) const SOFTWARE_RENDERING_ARGUMENT: &str = "--disable-gpu";

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
static STARTING: AtomicBool = AtomicBool::new(false);
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

pub(crate) fn wait_until_ready(timeout: Duration) -> bool {
    super::delivery::warmup();
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
    super::delivery::request_restart();
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
    if LIVE_GENERATION.load(Ordering::SeqCst) != 0 && PROCESS.lock().unwrap().is_some() {
        return ProcessState::Running;
    }
    if now_ms() < RESTART_NOT_BEFORE_MS.load(Ordering::SeqCst)
        || STARTING.swap(true, Ordering::SeqCst)
    {
        return ProcessState::Unavailable;
    }
    let spawned = match spawn_process() {
        Ok(()) => ProcessState::Spawned,
        Err(error) => {
            schedule_restart_backoff();
            crate::log_info!("[ResultCompositor] failed to start renderer: {error:#}");
            ProcessState::Unavailable
        }
    };
    STARTING.store(false, Ordering::SeqCst);
    spawned
}

fn spawn_process() -> anyhow::Result<()> {
    let inherited_browser_arguments = std::env::var_os("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS");
    let fallback_required = SOFTWARE_RENDERING_REQUIRED.load(Ordering::SeqCst);
    let software_rendering = fallback_required
        || inherited_browser_arguments
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
            browser_arguments_with_software_fallback(inherited_browser_arguments),
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
    let mut stdin = BufWriter::new(stdin);
    write_to(
        &mut stdin,
        &HostCommand::Snapshot {
            cards: super::parent::scene_snapshot(),
        },
    )?;
    write_to(
        &mut stdin,
        &HostCommand::Theme {
            theme: super::parent::current_theme(),
        },
    )?;

    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let pid = child.id();
    LIVE_GENERATION.store(generation, Ordering::SeqCst);
    LIVE_PID.store(pid, Ordering::SeqCst);
    READY_GENERATION.store(0, Ordering::SeqCst);
    READY_SINCE_MS.store(0, Ordering::SeqCst);
    LAST_HEARTBEAT_MS.store(now_ms(), Ordering::SeqCst);
    *PROCESS.lock().unwrap() = Some(RendererProcess {
        child,
        stdin,
        generation,
        _job: job,
    });
    std::thread::spawn(move || read_events(stdout, generation));
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            crate::log_info!("[ResultCompositor] child generation={generation}: {line}");
        }
    });
    crate::log_info!(
        "[ResultCompositor] renderer spawned generation={generation} pid={pid} software_rendering={software_rendering}"
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
        .ok_or_else(|| anyhow::anyhow!("result renderer stdin was not created"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("result renderer stdout was not created"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("result renderer stderr was not created"))?;
    Ok((job, stdin, stdout, stderr))
}

fn create_kill_on_close_job(child: &Child) -> anyhow::Result<OwnedHandle> {
    let raw = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
        .context("create result compositor job")?;
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
        .context("configure result compositor job")?;
        AssignProcessToJobObject(HANDLE(job.as_raw_handle()), HANDLE(child.as_raw_handle()))
            .context("contain result compositor process")?;
    }
    Ok(job)
}

pub(super) fn write_command(command: &HostCommand) -> anyhow::Result<()> {
    let mut process = PROCESS.lock().unwrap();
    let renderer = process
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("result compositor is unavailable"))?;
    write_to(&mut renderer.stdin, command)
}

fn write_to(writer: &mut impl Write, command: &HostCommand) -> anyhow::Result<()> {
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
                    "[ResultCompositor] invalid child event generation={generation} bytes={} error={error}",
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
            ChildEvent::ResyncRequested => {
                crate::log_info!(
                    "[ResultCompositor] renderer requested snapshot generation={generation}"
                );
                super::delivery::queue_snapshot();
            }
            ChildEvent::RendererFailure { kind } => handle_renderer_failure(generation, kind),
            other => super::parent::handle_child_event(other, generation),
        }
    }
    fail_generation(generation, "renderer disconnected", false);
}

fn renderer_ready(generation: u64) {
    let ready_at = now_ms();
    LAST_HEARTBEAT_MS.store(ready_at, Ordering::SeqCst);
    READY_SINCE_MS.store(ready_at, Ordering::SeqCst);
    READY_GENERATION.store(generation, Ordering::SeqCst);
    crate::log_info!("[ResultCompositor] renderer ready generation={generation}");
}

fn renderer_heartbeat() {
    let heartbeat_at = now_ms();
    LAST_HEARTBEAT_MS.store(heartbeat_at, Ordering::SeqCst);
    if heartbeat_at.saturating_sub(READY_SINCE_MS.load(Ordering::SeqCst)) >= STABLE_GENERATION_MS {
        RESTART_FAILURES.store(0, Ordering::SeqCst);
        RESTART_NOT_BEFORE_MS.store(0, Ordering::SeqCst);
        if heartbeat_at.saturating_sub(LAST_GPU_FAILURE_MS.load(Ordering::SeqCst))
            >= STABLE_GENERATION_MS
        {
            GPU_FAILURES.store(0, Ordering::SeqCst);
        }
    }
}

fn handle_renderer_failure(generation: u64, kind: RendererFailureKind) {
    match kind {
        RendererFailureKind::GpuProcessExited => {
            let now = now_ms();
            let previous_at = LAST_GPU_FAILURE_MS.swap(now, Ordering::SeqCst);
            let previous_count = GPU_FAILURES.load(Ordering::SeqCst);
            let (count, fallback) = gpu_failure_state(previous_at, previous_count, now);
            GPU_FAILURES.store(count, Ordering::SeqCst);
            crate::log_info!(
                "[ResultCompositor] GPU process failure generation={generation} count={count}"
            );
            if fallback {
                if !SOFTWARE_RENDERING_REQUIRED.swap(true, Ordering::SeqCst) {
                    crate::log_info!(
                        "[ResultCompositor] enabling isolated software rendering fallback"
                    );
                }
                fail_generation(generation, "repeated GPU process failures", true);
            }
        }
        RendererFailureKind::BrowserProcessExited
        | RendererFailureKind::RenderProcessExited
        | RendererFailureKind::RenderProcessUnresponsive
        | RendererFailureKind::FrameRenderProcessExited => {
            fail_generation(generation, kind.as_str(), true);
        }
    }
}

pub(super) fn start_watchdog() {
    WATCHDOG.call_once(|| {
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let live = LIVE_GENERATION.load(Ordering::SeqCst);
                if live == 0 {
                    super::delivery::request_restart();
                    continue;
                }
                let timeout = if READY_GENERATION.load(Ordering::SeqCst) == live {
                    HEARTBEAT_TIMEOUT_MS
                } else {
                    STARTUP_TIMEOUT_MS
                };
                if heartbeat_is_stale(now_ms(), LAST_HEARTBEAT_MS.load(Ordering::SeqCst), timeout) {
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
        super::delivery::request_restart();
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
    super::parent::DRAGGING.store(false, Ordering::SeqCst);
    schedule_restart_backoff();
    crate::log_info!("[ResultCompositor] renderer failed generation={generation} reason={reason}");
    super::delivery::request_restart();
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
            let _ = TerminateProcess(handle, 0x5354_4702);
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
    let mut old = PROCESS.lock().unwrap().take();
    if let Some(renderer) = old.as_mut() {
        LIVE_GENERATION
            .compare_exchange(renderer.generation, 0, Ordering::SeqCst, Ordering::SeqCst)
            .ok();
        LIVE_PID.store(0, Ordering::SeqCst);
        READY_GENERATION.store(0, Ordering::SeqCst);
        READY_SINCE_MS.store(0, Ordering::SeqCst);
        super::parent::DRAGGING.store(false, Ordering::SeqCst);
        let _ = renderer.child.kill();
        let _ = renderer.child.wait();
    }
    drop(old);
    ensure_process()
}

fn browser_arguments_with_software_fallback(existing: Option<OsString>) -> OsString {
    let mut arguments = existing.unwrap_or_default();
    if has_software_rendering_argument(&arguments) {
        return arguments;
    }
    if !arguments.is_empty() {
        arguments.push(" ");
    }
    arguments.push(SOFTWARE_RENDERING_ARGUMENT);
    arguments
}

pub(super) fn software_rendering_requested() -> bool {
    std::env::var_os("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS")
        .is_some_and(|arguments| has_software_rendering_argument(&arguments))
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

fn heartbeat_is_stale(now: u64, last: u64, timeout: u64) -> bool {
    now.saturating_sub(last) > timeout
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
        assert_eq!(restart_delay_ms(2), 500);
        assert_eq!(restart_delay_ms(3), 1_000);
        assert_eq!(restart_delay_ms(8), 30_000);
        assert_eq!(restart_delay_ms(u32::MAX), 30_000);
    }

    #[test]
    fn heartbeat_timeout_uses_saturating_elapsed_time() {
        assert!(!heartbeat_is_stale(5_000, 0, 5_000));
        assert!(heartbeat_is_stale(5_001, 0, 5_000));
        assert!(!heartbeat_is_stale(1, 2, 5_000));
    }

    #[test]
    fn repeated_gpu_failures_activate_fallback_only_inside_the_window() {
        assert_eq!(gpu_failure_state(0, 0, 100), (1, false));
        assert_eq!(gpu_failure_state(100, 1, 200), (2, false));
        assert_eq!(gpu_failure_state(200, 2, 300), (3, true));
        assert_eq!(gpu_failure_state(200, 2, 60_201), (1, false));
    }

    #[test]
    fn software_fallback_preserves_existing_webview_arguments() {
        assert_eq!(
            browser_arguments_with_software_fallback(Some(OsString::from("--foo"))),
            OsString::from("--foo --disable-gpu")
        );
        assert_eq!(
            browser_arguments_with_software_fallback(None),
            OsString::from("--disable-gpu")
        );
        assert_eq!(
            browser_arguments_with_software_fallback(Some(OsString::from("--disable-gpu"))),
            OsString::from("--disable-gpu")
        );
    }
}
