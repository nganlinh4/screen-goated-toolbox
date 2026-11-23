use crate::component_registry::{canonical_regular_dir, canonical_regular_file};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::process_job::{Inheritance, JobControl, OwnedProcess};
use anyhow::{Context, Result, anyhow, bail};
#[cfg(test)]
use sgt_recorder_protocol::MAX_RESPONSE_LINE_BYTES;
use sgt_recorder_protocol::{Command, Response, TOKEN_BYTES};

mod capabilities;
mod deferred_capability;
mod dispatch;
mod environment;
mod framing;
mod progress;
mod request;
mod response_reader;
mod session_lifecycle;
mod session_writer;
mod settings;
mod startup_trace;

use capabilities::{MissingExternalCapability, prepare_external_capabilities};
use dispatch::{DispatchMessage, HEADLESS_DISPATCHER, HeadlessMessage, dispatch_loop};
#[cfg(test)]
use environment::PROVIDER_CREDENTIAL_ENV;
#[cfg(debug_assertions)]
use environment::forward_debug_cursor_source;
#[cfg(test)]
use environment::forward_provider_credentials_for_runtime;
use environment::{
    forward_provider_credentials, forward_runtime_state_root, forward_webview_runtime_roots,
    recorder_debug_port, recorder_webview_data_dir,
};
#[cfg(test)]
use framing::read_bounded_line;
use request::{ComponentDelivery, OptionalComponentUnavailable, request_with_capability_retry};
use response_reader::start_response_reader;
use session_writer::SessionWriter;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static REMOVAL_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static DISPATCH_GATE: Mutex<()> = Mutex::new(());
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
const CODEC_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const REMOVAL_STOP_TIMEOUT: Duration = Duration::from_secs(70);
const HEADLESS_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);
const DROP_WAIT: Duration = Duration::from_millis(500);
const PREPARE_TIMEOUT: Duration = Duration::from_secs(50);
const EXIT_TIMEOUT: Duration = Duration::from_secs(70);

enum ResponseEvent {
    Response(Response),
    Failed(String),
}

struct WorkerSession {
    child: OwnedProcess,
    stdin: Arc<SessionWriter>,
    responses: mpsc::Receiver<ResponseEvent>,
    token: String,
    progress: Arc<progress::ProgressSession>,
}

pub(super) struct RemovalGuard;

impl Drop for RemovalGuard {
    fn drop(&mut self) {
        REMOVAL_IN_PROGRESS.store(false, Ordering::Release);
    }
}

impl Drop for WorkerSession {
    fn drop(&mut self) {
        self.progress.retire();
        settings::retire_session(&self.token);
        // Break any in-flight pipe write before waiting for the writer lock.
        let _ = self.child.terminate();
        self.stdin.close();
        let _ = self.child.stop_within(DROP_WAIT);
    }
}

pub(super) fn launch_in_background(command: Command) {
    let Ok(_gate) = DISPATCH_GATE.lock() else {
        return;
    };
    if SHUTDOWN_IN_PROGRESS.load(Ordering::Acquire) {
        return;
    }
    if REMOVAL_IN_PROGRESS.load(Ordering::Acquire) {
        crate::log_info!("[ScreenRecord] launch skipped while recorder removal is in progress");
        return;
    }
    if matches!(&command, Command::Show | Command::Toggle) {
        startup_trace::begin();
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
    let _gate = DISPATCH_GATE
        .lock()
        .map_err(|_| anyhow!("recorder dispatch unavailable"))?;
    if SHUTDOWN_IN_PROGRESS.load(Ordering::Acquire) {
        bail!("recorder shutdown is in progress");
    }
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
    HEADLESS_DISPATCHER
        .send(HeadlessMessage::Run(command, sender))
        .map_err(|_| anyhow!("recorder headless dispatcher is unavailable"))?;
    receiver
        .recv_timeout(HEADLESS_TIMEOUT + NORMAL_TIMEOUT)
        .context("recorder headless dispatcher timed out")?
        .map_err(anyhow::Error::msg)
}

pub(super) fn shutdown() -> Result<()> {
    let gate = DISPATCH_GATE
        .lock()
        .map_err(|_| anyhow!("recorder dispatch unavailable"))?;
    if !ACTIVE.load(Ordering::Acquire) {
        return Ok(());
    }
    SHUTDOWN_IN_PROGRESS
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| anyhow!("recorder shutdown is already in progress"))?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::sync_channel(1);
    let dispatched = DISPATCHER.send(DispatchMessage::Exit(cancelled.clone(), sender));
    drop(gate);
    let result = dispatched
        .map_err(|_| anyhow!("recorder dispatcher is unavailable"))
        .and_then(|_| {
            receiver
                .recv_timeout(EXIT_TIMEOUT)
                .context("recorder shutdown timed out")
        })
        .and_then(|result| result.map_err(anyhow::Error::msg));
    if result.is_err() {
        cancelled.store(true, Ordering::Release);
    }
    SHUTDOWN_IN_PROGRESS.store(false, Ordering::Release);
    result
}

#[cfg(test)]
pub(super) fn worker_process_is_active() -> bool {
    session_lifecycle::active_job_registered()
}

pub(super) fn stop_for_removal() -> Result<RemovalGuard> {
    let gate = DISPATCH_GATE
        .lock()
        .map_err(|_| anyhow!("recorder dispatch unavailable"))?;
    REMOVAL_IN_PROGRESS
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| anyhow!("recorder removal is already in progress"))?;
    let guard = RemovalGuard;
    let active = ACTIVE.load(Ordering::Acquire);
    drop(gate);
    // A headless job runs in its own worker beside the window; stop it too.
    session_lifecycle::cancel_slot_work(session_lifecycle::JobSlot::Headless);
    if active {
        shutdown()?;
    } else {
        session_lifecycle::cancel_active_work();
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    let cancelled = Arc::new(AtomicBool::new(false));
    if DISPATCHER
        .send(DispatchMessage::Stop(cancelled.clone(), sender))
        .is_err()
    {
        drop(guard);
        bail!("recorder dispatcher is unavailable");
    }
    let result = receiver
        .recv_timeout(REMOVAL_STOP_TIMEOUT)
        .context("recorder worker did not stop before removal")
        .and_then(|result| result.map_err(anyhow::Error::msg));
    if result.is_err() {
        cancelled.store(true, Ordering::Release);
    }
    result?;
    dispatch::drain_headless(&HEADLESS_DISPATCHER, REMOVAL_STOP_TIMEOUT)?;
    Ok(guard)
}

fn launch(
    cancelled: &AtomicBool,
    requested: Option<crate::component_registry::external_tools::ExternalTool>,
    slot: session_lifecycle::JobSlot,
    delivery: ComponentDelivery,
) -> Result<WorkerSession> {
    startup_trace::log("component-verification-start");
    if !delivery.may_install() && !crate::component_registry::recorder::is_installed() {
        return Err(OptionalComponentUnavailable.into());
    }
    let components = crate::component_registry::recorder::ensure_ready_with_badge(cancelled)?;
    startup_trace::log("component-verification-complete");
    let external_capabilities = prepare_external_capabilities(cancelled, requested)?;
    let deferred_ffmpeg = (requested.is_none() && delivery.may_install())
        .then(deferred_capability::DeferredFfmpeg::prepare)
        .transpose()?;
    let executable = canonical_regular_file(&components.worker_path, "recorder worker")?;
    let web_root = canonical_regular_dir(&components.web_root, "recorder web package")?;
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| anyhow!("SystemRoot is unavailable"))?;
    let system32 =
        canonical_regular_dir(&system_root.join("System32"), "recorder Windows System32")?;
    let token = launch_token()?;
    let progress_token = launch_token().ok();
    let progress_socket = progress_token
        .as_ref()
        .and_then(|_| progress::ProgressSession::bind());
    let product_font_url = crate::overlay::html_components::font_manager::product_font_url()
        .context("start the shared product-font service")?;
    let temp = crate::paths::child_process_temp_dir()?;
    let workspace = recorder_worker_workspace()?;
    let webview_data_dir =
        recorder_webview_data_dir(std::env::var_os("SGT_SCREEN_RECORD_WEBVIEW2_DATA_DIR"))?;

    let mut command = ProcessCommand::new(&executable);
    startup_trace::configure(&mut command);
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
    if let Some(address) = progress_socket
        .as_ref()
        .and_then(|socket| socket.local_addr().ok())
    {
        command.env("SGT_RECORDER_PROGRESS_ADDR", address.to_string());
        if let Some(progress_token) = &progress_token {
            command.env("SGT_RECORDER_PROGRESS_TOKEN", progress_token);
        }
    }
    if let Some(port) = recorder_debug_port(std::env::var_os("SGT_RECORDER_WEBVIEW2_DEBUG_PORT")) {
        command.env("SGT_RECORDER_WEBVIEW2_DEBUG_PORT", port);
    }
    #[cfg(debug_assertions)]
    if std::env::var("SGT_RECORDER_DIAGNOSTICS").as_deref() == Ok("1") {
        command.env("SGT_RECORDER_DIAGNOSTICS", "1");
    }
    forward_webview_runtime_roots(&mut command, |name| std::env::var_os(name));
    forward_runtime_state_root(&mut command, |name| std::env::var_os(name));
    #[cfg(debug_assertions)]
    forward_debug_cursor_source(
        &mut command,
        std::env::var_os("SGT_RECORDER_DEV_ASSET_ROOT"),
    );
    if let Some(capability) = &deferred_ffmpeg {
        capability.configure(&mut command);
    }
    for component in &external_capabilities {
        match component.tool() {
            crate::component_registry::external_tools::ExternalTool::Ffmpeg => {
                command.env(
                    "SGT_FFMPEG_PATH",
                    canonical_regular_file(&component.executable(), "recorder FFmpeg capability")?,
                );
            }
            tool => bail!("recorder does not accept the {} capability", tool.id()),
        }
    }
    forward_provider_credentials(&mut command, |name| std::env::var_os(name));
    let helper_name = format!("Local\\SGTHelpers-{}", launch_token()?);
    let helper_job = JobControl::named(&helper_name)?;
    command.env(crate::process_job::PARENT_JOB_ENV, &helper_name);
    let mut child = spawn_worker_process(
        &mut command,
        (components, external_capabilities, deferred_ffmpeg),
    )
    .with_context(|| format!("start recorder worker '{}'", executable.display()))?;
    child.retain_job(helper_job);
    let group = child.group();
    let active_job = session_lifecycle::ActiveJobRegistration::register(group.clone(), slot)?;
    child.retain(active_job);
    let stdin = child
        .take_stdin()
        .context("recorder worker stdin missing")?;
    let stdout = child
        .take_stdout()
        .context("recorder worker stdout missing")?;
    if cancelled.load(Ordering::Acquire) || REMOVAL_IN_PROGRESS.load(Ordering::Acquire) {
        bail!("recorder launch was cancelled");
    }
    child.resume()?;
    startup_trace::log("worker-spawned");
    settings::register_session(&token);
    let stdin = Arc::new(SessionWriter::new(token.clone(), stdin));
    let progress = progress::ProgressSession::new();
    if let (Some(socket), Some(progress_token)) = (progress_socket, progress_token) {
        progress.start(socket, progress_token);
    }
    let responses = start_response_reader(
        stdout,
        token.clone(),
        stdin.clone(),
        group,
        progress.clone(),
    );
    let mut session = WorkerSession {
        child,
        stdin,
        responses,
        token,
        progress,
    };
    let hello = session.request(Command::Ping, NORMAL_TIMEOUT)?;
    if !supports_settings_protocol(&hello) {
        bail!("recorder worker does not support the required settings protocol");
    }
    if hello
        .get("shutdownProtocol")
        .and_then(serde_json::Value::as_u64)
        != Some(u64::from(sgt_recorder_protocol::SHUTDOWN_PROTOCOL_VERSION))
    {
        bail!("recorder worker does not support the required shutdown protocol");
    }
    startup_trace::log("worker-ping-ready");
    crate::component_registry::recorder::refresh_catalog_after_open();
    Ok(session)
}

fn supports_settings_protocol(hello: &serde_json::Value) -> bool {
    hello
        .get("settingsProtocol")
        .and_then(serde_json::Value::as_u64)
        == Some(u64::from(sgt_recorder_protocol::SETTINGS_PROTOCOL_VERSION))
}

fn spawn_worker_process(
    command: &mut ProcessCommand,
    resources: impl Send + 'static,
) -> Result<OwnedProcess> {
    OwnedProcess::spawn_with_policy(command, resources, Inheritance::SilentBreakaway)
}

fn launch_token() -> Result<String> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).context("generate recorder launch token")?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn required_directory_env(name: &str) -> Result<PathBuf> {
    let value = std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| anyhow!("{name} is unavailable"))?;
    canonical_regular_dir(&value, &format!("recorder {name}"))
}

fn required_system_drive() -> Result<OsString> {
    let value =
        std::env::var_os("SystemDrive").ok_or_else(|| anyhow!("SystemDrive is unavailable"))?;
    let text = value.to_string_lossy();
    if text.len() != 2 || !text.as_bytes()[0].is_ascii_alphabetic() || text.as_bytes()[1] != b':' {
        bail!("SystemDrive is invalid");
    }
    Ok(value)
}

fn recorder_worker_workspace() -> Result<PathBuf> {
    crate::component_registry::worker_workspace(crate::component_registry::recorder::WORKER_ID)
}

#[cfg(test)]
#[path = "host_launcher/job_tests.rs"]
mod job_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod tests;
