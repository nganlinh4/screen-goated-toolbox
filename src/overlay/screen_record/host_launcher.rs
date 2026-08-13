use std::ffi::{OsString, c_void};
use std::io::{BufRead, BufReader, Write as _};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use sgt_recorder_protocol::{
    Command, MAX_JSON_BYTES, MAX_RESPONSE_LINE_BYTES, PROTOCOL_VERSION, RESPONSE_PREFIX, Request,
    Response, TOKEN_BYTES,
};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
};
use windows::core::PCWSTR;

mod environment;
mod capabilities;

use capabilities::{MissingExternalCapability, prepare_external_capabilities};
use environment::{
    forward_provider_credentials, forward_webview_runtime_roots, recorder_debug_port,
    recorder_webview_data_dir,
};
#[cfg(test)]
use environment::PROVIDER_CREDENTIAL_ENV;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static REMOVAL_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);
static DISPATCHER: LazyLock<mpsc::Sender<DispatchMessage>> = LazyLock::new(|| {
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("sgt-recorder-dispatch".to_string())
        .spawn(move || dispatch_loop(receiver))
        .expect("spawn recorder dispatcher");
    sender
});
const NORMAL_TIMEOUT: Duration = Duration::from_secs(15);
const REMOVAL_STOP_TIMEOUT: Duration = Duration::from_secs(17);
const HEADLESS_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);
const DROP_WAIT: Duration = Duration::from_millis(500);
const RECORDER_WEBVIEW_PROFILE: &str = "screen-recorder-worker";

enum DispatchMessage {
    Interactive(Command),
    Headless(Command, mpsc::SyncSender<Result<serde_json::Value, String>>),
    Stop(mpsc::SyncSender<()>),
}

enum ResponseEvent {
    Response(Response),
    Failed(String),
}

struct WorkerSession {
    child: Child,
    stdin: ChildStdin,
    responses: mpsc::Receiver<ResponseEvent>,
    token: String,
    job: OwnedHandle,
    _components: crate::component_registry::recorder::RecorderComponents,
    _external_capabilities: Vec<crate::component_registry::external_tools::ExternalToolUse>,
}

pub(super) struct RemovalGuard;

impl Drop for RemovalGuard {
    fn drop(&mut self) {
        REMOVAL_IN_PROGRESS.store(false, Ordering::Release);
    }
}

impl Drop for WorkerSession {
    fn drop(&mut self) {
        terminate_and_reap(&mut self.child, &self.job);
    }
}

fn terminate_and_reap(child: &mut Child, job: &OwnedHandle) {
    let _ = unsafe { TerminateJobObject(HANDLE(job.as_raw_handle()), 1) };
    let deadline = Instant::now() + DROP_WAIT;
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub(super) fn launch_in_background(command: Command) {
    if REMOVAL_IN_PROGRESS.load(Ordering::Acquire) {
        crate::log_info!("[ScreenRecord] launch skipped while recorder removal is in progress");
        return;
    }
    ACTIVE.store(true, Ordering::Release);
    if DISPATCHER
        .send(DispatchMessage::Interactive(command))
        .is_err()
    {
        ACTIVE.store(false, Ordering::Release);
    }
}

pub(super) fn send_if_running(command: Command) -> Result<serde_json::Value> {
    if REMOVAL_IN_PROGRESS.load(Ordering::Acquire) {
        bail!("recorder removal is in progress");
    }
    if !ACTIVE.load(Ordering::Acquire) {
        bail!("recorder worker is not running");
    }
    DISPATCHER
        .send(DispatchMessage::Interactive(command))
        .map_err(|_| anyhow!("recorder dispatcher is unavailable"))?;
    Ok(serde_json::json!({ "status": "queued" }))
}

pub(super) fn run_headless(command: Command) -> Result<serde_json::Value> {
    if REMOVAL_IN_PROGRESS.load(Ordering::Acquire) {
        bail!("recorder removal is in progress");
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    DISPATCHER
        .send(DispatchMessage::Headless(command, sender))
        .map_err(|_| anyhow!("recorder dispatcher is unavailable"))?;
    receiver
        .recv_timeout(HEADLESS_TIMEOUT + NORMAL_TIMEOUT)
        .context("recorder headless dispatcher timed out")?
        .map_err(anyhow::Error::msg)
}

pub(super) fn shutdown() {
    if ACTIVE.swap(false, Ordering::AcqRel) {
        let _ = DISPATCHER.send(DispatchMessage::Interactive(Command::Cleanup));
        let _ = DISPATCHER.send(DispatchMessage::Interactive(Command::Shutdown));
    }
}

pub(super) fn stop_for_removal() -> Result<RemovalGuard> {
    REMOVAL_IN_PROGRESS
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| anyhow!("recorder removal is already in progress"))?;
    let guard = RemovalGuard;
    let (sender, receiver) = mpsc::sync_channel(1);
    if DISPATCHER.send(DispatchMessage::Stop(sender)).is_err() {
        drop(guard);
        bail!("recorder dispatcher is unavailable");
    }
    if receiver.recv_timeout(REMOVAL_STOP_TIMEOUT).is_err() {
        drop(guard);
        bail!("recorder worker did not stop before removal");
    }
    Ok(guard)
}

fn dispatch_loop(receiver: mpsc::Receiver<DispatchMessage>) {
    let mut interactive = None;
    for message in receiver {
        match message {
            DispatchMessage::Interactive(command) => {
                let shutdown = matches!(command, Command::Shutdown);
                let result = dispatch_interactive(&mut interactive, command);
                if shutdown || result.is_err() {
                    interactive.take();
                    ACTIVE.store(false, Ordering::Release);
                }
                if let Err(error) = result {
                    report_launch_error(&error);
                }
            }
            DispatchMessage::Headless(command, sender) => {
                let result = dispatch_headless(command).map_err(|error| format!("{error:#}"));
                let _ = sender.send(result);
            }
            DispatchMessage::Stop(sender) => {
                ACTIVE.store(false, Ordering::Release);
                if let Some(session) = interactive.as_mut()
                    && let Err(error) = session.request(Command::Shutdown, NORMAL_TIMEOUT)
                {
                    crate::log_info!(
                        "[ScreenRecord] graceful shutdown failed; forcing worker exit: {error:#}"
                    );
                }
                // WorkerSession::drop terminates the job, waits for the process to
                // exit, then releases component leases and locked file handles.
                interactive.take();
                let _ = sender.send(());
            }
        }
    }
}

fn dispatch_interactive(session: &mut Option<WorkerSession>, command: Command) -> Result<()> {
    for pending in super::pending_commands() {
        request_with_capability_retry(session, pending, NORMAL_TIMEOUT)?;
    }
    request_with_capability_retry(session, command, NORMAL_TIMEOUT)?;
    Ok(())
}

fn dispatch_headless(command: Command) -> Result<serde_json::Value> {
    let mut session = None;
    let result = request_with_capability_retry(&mut session, command, HEADLESS_TIMEOUT);
    if let Some(session) = session.as_mut() {
        let _ = session.request(Command::Shutdown, NORMAL_TIMEOUT);
    }
    result
}

fn request_with_capability_retry(
    session: &mut Option<WorkerSession>,
    command: Command,
    timeout: Duration,
) -> Result<serde_json::Value> {
    if session.is_none() {
        let cancelled = AtomicBool::new(false);
        *session = Some(launch(&cancelled, None)?);
    }
    let first = session
        .as_mut()
        .ok_or_else(|| anyhow!("recorder worker did not start"))?
        .request(command.clone(), timeout);
    let error = match first {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    let Some(capability) = error
        .downcast_ref::<MissingExternalCapability>()
        .map(|missing| missing.tool)
    else {
        return Err(error);
    };

    session.take();
    let cancelled = AtomicBool::new(false);
    *session = Some(launch(&cancelled, Some(capability))?);
    session
        .as_mut()
        .ok_or_else(|| anyhow!("recorder worker did not restart"))?
        .request(command, timeout)
        .context("recorder command failed after one capability repair retry")
}

fn report_launch_error(error: &anyhow::Error) {
    crate::log_info!("[ScreenRecord] worker failed: {error:#}");
    crate::overlay::auto_copy_badge::show_detailed_notification(
        "Screen Recorder unavailable",
        &format!("{error:#}"),
        crate::overlay::auto_copy_badge::NotificationType::Error,
    );
}

fn launch(
    cancelled: &AtomicBool,
    requested: Option<crate::component_registry::external_tools::ExternalTool>,
) -> Result<WorkerSession> {
    let components = crate::component_registry::recorder::ensure_ready_with_badge(cancelled)?;
    let external_capabilities = prepare_external_capabilities(cancelled, requested)?;
    let executable = canonical_file(&components.worker_path, "worker")?;
    let web_root = canonical_dir(&components.web_root, "web package")?;
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| anyhow!("SystemRoot is unavailable"))?;
    let system32 = canonical_dir(&system_root.join("System32"), "Windows System32")?;
    let token = launch_token()?;
    let product_font_url = crate::overlay::html_components::font_manager::product_font_url()
        .context("start the shared product-font service")?;
    let temp = std::env::temp_dir();
    let workspace = recorder_worker_workspace()?;
    let webview_data_dir = recorder_webview_data_dir(std::env::var_os(
        "SGT_SCREEN_RECORD_WEBVIEW2_DATA_DIR",
    ))?;

    let mut command = ProcessCommand::new(&executable);
    command
        .current_dir(workspace)
        .env_clear()
        .env("SystemRoot", &system_root)
        .env("SystemDrive", required_system_drive()?)
        .env("WINDIR", &system_root)
        .env("PATH", &system32)
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .env("ProgramData", required_directory_env("ProgramData")?)
        .env(
            "ALLUSERSPROFILE",
            required_directory_env("ALLUSERSPROFILE")?,
        )
        .env("LOCALAPPDATA", required_directory_env("LOCALAPPDATA")?)
        .env("APPDATA", required_directory_env("APPDATA")?)
        .env("USERPROFILE", required_directory_env("USERPROFILE")?)
        .env("SGT_RECORDER_WEB_ROOT", &web_root)
        .env("SGT_PRODUCT_FONT_URL", product_font_url)
        .env("SGT_SCREEN_RECORD_WEBVIEW2_DATA_DIR", webview_data_dir)
        .env("SGT_RECORDER_LAUNCH_TOKEN", &token)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(port) = recorder_debug_port(std::env::var_os("SGT_RECORDER_WEBVIEW2_DEBUG_PORT")) {
        command.env("SGT_RECORDER_WEBVIEW2_DEBUG_PORT", port);
    }
    forward_webview_runtime_roots(&mut command, |name| std::env::var_os(name));
    for component in &external_capabilities {
        match component.tool() {
            crate::component_registry::external_tools::ExternalTool::Ffmpeg => {
                command.env(
                    "SGT_FFMPEG_PATH",
                    canonical_file(&component.executable(), "FFmpeg capability")?,
                );
            }
            tool => bail!("recorder does not accept the {} capability", tool.id()),
        }
    }
    forward_provider_credentials(&mut command, |name| std::env::var_os(name));
    let mut child = command
        .spawn()
        .with_context(|| format!("start recorder worker '{}'", executable.display()))?;
    let stdin = child
        .stdin
        .take()
        .context("recorder worker stdin missing")?;
    let stdout = child
        .stdout
        .take()
        .context("recorder worker stdout missing")?;
    let job = match create_job(&child) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            let _ = child.try_wait();
            return Err(error);
        }
    };
    let responses = start_response_reader(stdout);
    let mut session = WorkerSession {
        child,
        stdin,
        responses,
        token,
        job,
        _components: components,
        _external_capabilities: external_capabilities,
    };
    session.request(Command::Ping, NORMAL_TIMEOUT)?;
    Ok(session)
}

impl WorkerSession {
    fn request(&mut self, command: Command, timeout: Duration) -> Result<serde_json::Value> {
        let result = (|| {
            if let Some(status) = self.child.try_wait()? {
                bail!("recorder worker exited with {status}");
            }
            command.validate().map_err(anyhow::Error::msg)?;
            let request_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
            let request = Request {
                protocol_version: PROTOCOL_VERSION,
                token: self.token.clone(),
                request_id,
                command,
            };
            let body = serde_json::to_vec(&request)?;
            if body.len() > MAX_JSON_BYTES {
                bail!("recorder request exceeds protocol limit");
            }
            self.stdin.write_all(&body)?;
            self.stdin.write_all(b"\n")?;
            self.stdin.flush()?;
            match self.responses.recv_timeout(timeout) {
                Ok(ResponseEvent::Response(response)) => {
                    response
                        .validate(&self.token, request_id)
                        .map_err(anyhow::Error::msg)?;
                    if let Some(error) = response.error {
                        if let Some(tool) =
                            crate::component_registry::capabilities::requested_external_tool(&error)
                        {
                            return Err(MissingExternalCapability { tool }.into());
                        }
                        bail!("{error}");
                    }
                    response
                        .result
                        .ok_or_else(|| anyhow!("recorder worker returned an empty response"))
                }
                Ok(ResponseEvent::Failed(error)) => bail!("{error}"),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    bail!("recorder worker response timed out")
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    bail!("recorder worker response channel closed")
                }
            }
        })();
        if result.is_err() {
            let _ = unsafe { TerminateJobObject(HANDLE(self.job.as_raw_handle()), 1) };
        }
        result
    }
}

fn start_response_reader(stdout: ChildStdout) -> mpsc::Receiver<ResponseEvent> {
    let (sender, receiver) = mpsc::sync_channel(32);
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let line = match read_bounded_line(&mut reader, MAX_RESPONSE_LINE_BYTES) {
                Ok(Some(line)) => line,
                Ok(None) => {
                    let _ = sender.send(ResponseEvent::Failed(
                        "recorder worker closed its response stream".to_string(),
                    ));
                    return;
                }
                Err(error) => {
                    let _ = sender.send(ResponseEvent::Failed(format!(
                        "recorder worker response framing failed: {error:#}"
                    )));
                    return;
                }
            };
            let Some(body) = line.strip_prefix(RESPONSE_PREFIX.as_bytes()) else {
                continue;
            };
            match serde_json::from_slice::<Response>(body) {
                Ok(response) => {
                    if sender.send(ResponseEvent::Response(response)).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(ResponseEvent::Failed(format!(
                        "recorder worker returned malformed JSON: {error}"
                    )));
                    return;
                }
            }
        }
    });
    receiver
}

fn read_bounded_line(reader: &mut impl BufRead, maximum: usize) -> Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                bail!("recorder response ended before newline")
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        if line.len().saturating_add(take) > maximum {
            bail!("recorder response exceeds protocol limit");
        }
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if newline.is_some() {
            while matches!(line.last(), Some(b'\n' | b'\r')) {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}

fn launch_token() -> Result<String> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).context("generate recorder launch token")?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn create_job(child: &Child) -> Result<OwnedHandle> {
    let raw = unsafe { CreateJobObjectW(None, PCWSTR::null()) }.context("create recorder job")?;
    let job = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    // Keep the worker itself bound to the host, while allowing WebView2 to
    // create its separately managed browser process tree.
    limits.BasicLimitInformation.LimitFlags =
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
    unsafe {
        SetInformationJobObject(
            HANDLE(job.as_raw_handle()),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast::<c_void>(),
            std::mem::size_of_val(&limits) as u32,
        )
        .context("configure recorder job")?;
        AssignProcessToJobObject(HANDLE(job.as_raw_handle()), HANDLE(child.as_raw_handle()))
            .context("contain recorder worker")?;
    }
    Ok(job)
}

fn required_directory_env(name: &str) -> Result<PathBuf> {
    let value = std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| anyhow!("{name} is unavailable"))?;
    canonical_dir(&value, name)
}

fn required_system_drive() -> Result<OsString> {
    let value = std::env::var_os("SystemDrive")
        .ok_or_else(|| anyhow!("SystemDrive is unavailable"))?;
    let text = value.to_string_lossy();
    if text.len() != 2
        || !text.as_bytes()[0].is_ascii_alphabetic()
        || text.as_bytes()[1] != b':'
    {
        bail!("SystemDrive is invalid");
    }
    Ok(value)
}

fn recorder_worker_workspace() -> Result<PathBuf> {
    crate::component_registry::worker_workspace(
        crate::component_registry::recorder::WORKER_ID,
    )
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize recorder {label} '{}'", path.display()))?;
    let metadata = std::fs::symlink_metadata(&canonical)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("recorder {label} path is unsafe");
    }
    Ok(canonical)
}

fn canonical_dir(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize recorder {label} '{}'", path.display()))?;
    let metadata = std::fs::symlink_metadata(&canonical)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("recorder {label} directory is unsafe");
    }
    Ok(canonical)
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(test)]
mod tests;
