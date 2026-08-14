use super::CHILD_FLAG;
use super::mailbox::{CommandBuffer, PushResult};
use super::protocol::{ChildEvent, HostCommand, RendererFailureKind, StatusSnapshot};
use anyhow::Context;
use std::ffi::{OsString, c_void};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
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

#[derive(Default)]
struct PendingDelivery {
    commands: CommandBuffer,
    restart: bool,
    warmup: bool,
    snapshot_required: bool,
}

struct DeliveryBatch {
    commands: Vec<HostCommand>,
    restart: bool,
    warmup: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessState {
    Running,
    Spawned,
    Unavailable,
}

pub(super) static SNAPSHOT: LazyLock<Mutex<StatusSnapshot>> = LazyLock::new(|| {
    Mutex::new(StatusSnapshot {
        is_dark: crate::overlay::is_dark_mode(),
        selection: super::protocol::SelectionScene {
            capture_visible: true,
            ..Default::default()
        },
        ..Default::default()
    })
});
static PROCESS: LazyLock<Mutex<Option<RendererProcess>>> = LazyLock::new(|| Mutex::new(None));
static PENDING: LazyLock<Mutex<PendingDelivery>> =
    LazyLock::new(|| Mutex::new(PendingDelivery::default()));
static SIGNAL: LazyLock<SyncSender<()>> = LazyLock::new(|| {
    let (sender, receiver) = sync_channel(1);
    std::thread::Builder::new()
        .name("sgt-status-delivery".to_string())
        .spawn(move || delivery_loop(receiver))
        .expect("failed to start status compositor delivery thread");
    sender
});
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
static CAPTURE_APPLIED: AtomicU64 = AtomicU64::new(0);
static WATCHDOG: Once = Once::new();
static MONOTONIC_EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

pub(super) fn warmup() {
    start_watchdog();
    PENDING.lock().unwrap().warmup = true;
    signal_delivery();
}

pub(super) fn send(command: HostCommand) {
    start_watchdog();
    let mut pending = PENDING.lock().unwrap();
    if pending.commands.push(command) != PushResult::Queued {
        pending.snapshot_required = true;
    }
    drop(pending);
    signal_delivery();
}

pub(super) fn wait_for_capture(request_id: u64, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if CAPTURE_APPLIED.load(Ordering::SeqCst) >= request_id {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

pub(super) fn wait_until_ready(timeout: Duration) -> bool {
    warmup();
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let live = LIVE_GENERATION.load(Ordering::SeqCst);
        if live != 0 && READY_GENERATION.load(Ordering::SeqCst) == live {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

pub(super) fn restart_and_wait(timeout: Duration) -> bool {
    let previous_generation = GENERATION.load(Ordering::SeqCst);
    request_restart();
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

fn delivery_loop(receiver: Receiver<()>) {
    while receiver.recv().is_ok() {
        while receiver.try_recv().is_ok() {}
        let batch = take_pending();
        if batch.restart {
            let _ = restart_now();
            continue;
        }
        let process = if batch.warmup || !batch.commands.is_empty() {
            ensure_process()
        } else {
            ProcessState::Unavailable
        };
        match process {
            ProcessState::Spawned | ProcessState::Unavailable => continue,
            ProcessState::Running => {}
        }
        for command in batch.commands {
            if let Err(error) = write_command(&command) {
                crate::log_info!("[StatusCompositor] command delivery failed: {error:#}");
                fail_live_renderer("command delivery failed", true);
                break;
            }
        }
    }
}

fn take_pending() -> DeliveryBatch {
    let mut pending = PENDING.lock().unwrap();
    if pending.snapshot_required {
        let scene = SNAPSHOT.lock().unwrap().clone();
        pending
            .commands
            .replace_with_snapshot(HostCommand::Snapshot { scene });
        pending.snapshot_required = false;
    }
    DeliveryBatch {
        commands: pending.commands.drain(),
        restart: std::mem::take(&mut pending.restart),
        warmup: std::mem::take(&mut pending.warmup),
    }
}

fn queue_snapshot() {
    let mut pending = PENDING.lock().unwrap();
    let scene = SNAPSHOT.lock().unwrap().clone();
    pending
        .commands
        .replace_with_snapshot(HostCommand::Snapshot { scene });
    pending.snapshot_required = false;
    drop(pending);
    signal_delivery();
}

fn request_restart() {
    PENDING.lock().unwrap().restart = true;
    signal_delivery();
}

fn signal_delivery() {
    let _ = SIGNAL.try_send(());
}

fn ensure_process() -> ProcessState {
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
            crate::log_info!("[StatusCompositor] failed to start renderer: {error:#}");
            ProcessState::Unavailable
        }
    };
    STARTING.store(false, Ordering::SeqCst);
    spawned
}

fn spawn_process() -> anyhow::Result<()> {
    let inherited_arguments = std::env::var_os("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS");
    let fallback_required = SOFTWARE_RENDERING_REQUIRED.load(Ordering::SeqCst);
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
    let mut stdin = BufWriter::new(stdin);
    let snapshot = HostCommand::Snapshot {
        scene: SNAPSHOT.lock().unwrap().clone(),
    };
    if let Err(error) = write_to(&mut stdin, &snapshot) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let pid = child.id();
    LIVE_GENERATION.store(generation, Ordering::SeqCst);
    LIVE_PID.store(pid, Ordering::SeqCst);
    READY_GENERATION.store(0, Ordering::SeqCst);
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
            crate::log_info!("[StatusCompositor] child generation={generation}: {line}");
        }
    });
    crate::log_info!("[StatusCompositor] renderer spawned generation={generation} pid={pid}");
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
        .ok_or_else(|| anyhow::anyhow!("status renderer stdin was not created"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("status renderer stdout was not created"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("status renderer stderr was not created"))?;
    Ok((job, stdin, stdout, stderr))
}

fn create_kill_on_close_job(child: &Child) -> anyhow::Result<OwnedHandle> {
    let raw = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
        .context("create status compositor job")?;
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
        .context("configure status compositor job")?;
        AssignProcessToJobObject(HANDLE(job.as_raw_handle()), HANDLE(child.as_raw_handle()))
            .context("contain status compositor process")?;
    }
    Ok(job)
}

fn write_command(command: &HostCommand) -> anyhow::Result<()> {
    let mut process = PROCESS.lock().unwrap();
    let renderer = process
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("status compositor is unavailable"))?;
    write_to(&mut renderer.stdin, command)
}

fn write_to(writer: &mut BufWriter<ChildStdin>, command: &HostCommand) -> anyhow::Result<()> {
    serde_json::to_writer(&mut *writer, command)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn read_events(stdout: std::process::ChildStdout, generation: u64) {
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let Ok(event) = serde_json::from_str::<ChildEvent>(&line) else {
            crate::log_info!(
                "[StatusCompositor] invalid event generation={generation} bytes={}",
                line.len()
            );
            continue;
        };
        if LIVE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        match event {
            ChildEvent::Ready => {
                READY_GENERATION.store(generation, Ordering::SeqCst);
                let ready_at = now_ms();
                LAST_HEARTBEAT_MS.store(ready_at, Ordering::SeqCst);
                READY_SINCE_MS.store(ready_at, Ordering::SeqCst);
                crate::log_info!("[StatusCompositor] renderer ready generation={generation}");
            }
            ChildEvent::Heartbeat => {
                let heartbeat_at = now_ms();
                LAST_HEARTBEAT_MS.store(heartbeat_at, Ordering::SeqCst);
                if heartbeat_at.saturating_sub(READY_SINCE_MS.load(Ordering::SeqCst))
                    >= STABLE_GENERATION_MS
                {
                    RESTART_FAILURES.store(0, Ordering::SeqCst);
                    RESTART_NOT_BEFORE_MS.store(0, Ordering::SeqCst);
                    if heartbeat_at.saturating_sub(LAST_GPU_FAILURE_MS.load(Ordering::SeqCst))
                        >= STABLE_GENERATION_MS
                    {
                        GPU_FAILURES.store(0, Ordering::SeqCst);
                    }
                }
            }
            ChildEvent::RecordingReady => crate::overlay::recording::compositor_ready(),
            ChildEvent::RecordingPauseToggle => {
                crate::overlay::recording::compositor_toggle_pause()
            }
            ChildEvent::RecordingCancel => crate::overlay::recording::compositor_cancel(),
            ChildEvent::RecordingMoved { rect } => {
                if let Some(recording) = SNAPSHOT.lock().unwrap().recording.as_mut() {
                    recording.rect = rect;
                }
            }
            ChildEvent::NotificationFinished { through_id } => {
                SNAPSHOT
                    .lock()
                    .unwrap()
                    .notifications
                    .retain(|notification| notification.id > through_id);
            }
            ChildEvent::SelectionCaptureApplied { request_id } => {
                CAPTURE_APPLIED.fetch_max(request_id, Ordering::SeqCst);
            }
            ChildEvent::ResyncRequested => {
                crate::log_info!(
                    "[StatusCompositor] renderer requested snapshot generation={generation}"
                );
                queue_snapshot();
            }
            ChildEvent::RendererFailure { kind } => handle_renderer_failure(generation, kind),
            ChildEvent::RendererError { source, error } => {
                crate::log_info!("[StatusCompositor] renderer error source={source} error={error}")
            }
        }
    }
    fail_generation(generation, "renderer disconnected", false);
}

fn handle_renderer_failure(generation: u64, kind: RendererFailureKind) {
    if kind == RendererFailureKind::GpuProcessExited {
        let now = now_ms();
        let previous_at = LAST_GPU_FAILURE_MS.swap(now, Ordering::SeqCst);
        let previous_count = GPU_FAILURES.load(Ordering::SeqCst);
        let count = if now.saturating_sub(previous_at) <= GPU_FAILURE_WINDOW_MS {
            previous_count.saturating_add(1)
        } else {
            1
        };
        GPU_FAILURES.store(count, Ordering::SeqCst);
        if count >= GPU_FAILURE_THRESHOLD {
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
                let live = LIVE_GENERATION.load(Ordering::SeqCst);
                if live == 0 {
                    request_restart();
                    continue;
                }
                let timeout = if READY_GENERATION.load(Ordering::SeqCst) == live {
                    HEARTBEAT_TIMEOUT_MS
                } else {
                    15_000
                };
                if now_ms().saturating_sub(LAST_HEARTBEAT_MS.load(Ordering::SeqCst)) > timeout {
                    fail_generation(live, "heartbeat timed out", true);
                }
            }
        });
    });
}

fn fail_live_renderer(reason: &str, terminate: bool) {
    let generation = LIVE_GENERATION.load(Ordering::SeqCst);
    if generation != 0 {
        fail_generation(generation, reason, terminate);
    } else {
        request_restart();
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
    crate::log_info!("[StatusCompositor] renderer failed generation={generation} reason={reason}");
    request_restart();
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
            let _ = TerminateProcess(handle, 0x5354_4701);
        }
        let _ = CloseHandle(handle);
    }
}

fn schedule_restart_backoff() {
    let failure = RESTART_FAILURES.fetch_add(1, Ordering::SeqCst) + 1;
    let delay = restart_delay_ms(failure);
    RESTART_NOT_BEFORE_MS.fetch_max(now_ms().saturating_add(delay), Ordering::SeqCst);
}

fn restart_delay_ms(failure: u32) -> u64 {
    let shift = failure.saturating_sub(1).min(7);
    INITIAL_RESTART_DELAY_MS
        .saturating_mul(1_u64 << shift)
        .min(MAX_RESTART_DELAY_MS)
}

fn restart_now() -> ProcessState {
    let mut old = PROCESS.lock().unwrap().take();
    if let Some(renderer) = old.as_mut() {
        LIVE_GENERATION
            .compare_exchange(renderer.generation, 0, Ordering::SeqCst, Ordering::SeqCst)
            .ok();
        LIVE_PID.store(0, Ordering::SeqCst);
        READY_GENERATION.store(0, Ordering::SeqCst);
        READY_SINCE_MS.store(0, Ordering::SeqCst);
        let _ = renderer.child.kill();
        let _ = renderer.child.wait();
    }
    drop(old);
    ensure_process()
}

fn browser_arguments_with_software_fallback(existing: Option<OsString>) -> OsString {
    let mut arguments = existing.unwrap_or_default();
    if !arguments
        .to_string_lossy()
        .split_whitespace()
        .any(|argument| argument == SOFTWARE_RENDERING_ARGUMENT)
    {
        if !arguments.is_empty() {
            arguments.push(" ");
        }
        arguments.push(SOFTWARE_RENDERING_ARGUMENT);
    }
    arguments
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
    fn software_fallback_preserves_existing_browser_arguments() {
        let arguments = browser_arguments_with_software_fallback(Some(OsString::from("--foo")));
        assert!(arguments.to_string_lossy().contains("--foo"));
        assert!(
            arguments
                .to_string_lossy()
                .contains(SOFTWARE_RENDERING_ARGUMENT)
        );
    }
}
