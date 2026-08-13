use std::ffi::c_void;
use std::io::{BufReader, Write as _};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command as ProcessCommand, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use sgt_computer_control_protocol::{
    Command, MAX_JSON_BYTES, MAX_RESPONSE_LINE_BYTES, Output, PROTOCOL_VERSION, RESPONSE_PREFIX,
    Request, Response, TOKEN_BYTES,
};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows::core::PCWSTR;

use super::framing::read_bounded_line;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const DROP_WAIT: Duration = Duration::from_millis(500);
const ENGINE_VERSION: &str = "1.0.0";

enum ResponseEvent {
    Response(Response),
    Failed(String),
}

pub(super) struct EngineProcess {
    child: Child,
    stdin: ChildStdin,
    responses: mpsc::Receiver<ResponseEvent>,
    token: String,
    next_request_id: u64,
    job: OwnedHandle,
    _component: crate::component_registry::computer_control::ComputerControlEngineUse,
}

impl EngineProcess {
    pub(super) fn launch(cancelled: &AtomicBool) -> Result<Self> {
        let component =
            crate::component_registry::computer_control::ensure_engine_with_badge(cancelled)?;
        let executable = canonical_file(component.executable(), "engine")?;
        let system_root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| anyhow!("SystemRoot is unavailable"))?;
        let system32 = canonical_dir(&system_root.join("System32"), "Windows System32")?;
        let token = launch_token()?;
        let workspace = crate::component_registry::worker_workspace(
            crate::component_registry::computer_control::ID,
        )?;
        let mut command = ProcessCommand::new(&executable);
        command
            .arg("--stdio")
            .current_dir(workspace)
            .env_clear()
            .env("SystemRoot", &system_root)
            .env("WINDIR", &system_root)
            .env("PATH", &system32)
            .env("SGT_CC_ENGINE_LAUNCH_TOKEN", &token)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        let mut child = command
            .spawn()
            .with_context(|| format!("start Computer Control engine '{}'", executable.display()))?;
        let stdin = child
            .stdin
            .take()
            .context("Computer Control engine stdin missing")?;
        let stdout = child
            .stdout
            .take()
            .context("Computer Control engine stdout missing")?;
        let job = match create_job(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.try_wait();
                return Err(error);
            }
        };
        let responses = start_response_reader(stdout);
        let mut process = Self {
            child,
            stdin,
            responses,
            token,
            next_request_id: 1,
            job,
            _component: component,
        };
        match process.request(Command::Handshake {
            host_version: env!("CARGO_PKG_VERSION").to_string(),
        })? {
            Output::Handshake {
                engine_version,
                architecture,
            } if engine_version == ENGINE_VERSION && architecture == std::env::consts::ARCH => {}
            _ => bail!("Computer Control engine ABI handshake failed"),
        }
        Ok(process)
    }

    pub(super) fn request(&mut self, command: Command) -> Result<Output> {
        let result = self.request_inner(command);
        if result.is_err() {
            terminate_job(&self.job);
        }
        result
    }

    fn request_inner(&mut self, command: Command) -> Result<Output> {
        if let Some(status) = self.child.try_wait()? {
            bail!("Computer Control engine exited with {status}");
        }
        command.validate().map_err(anyhow::Error::msg)?;
        let request_id = self.next_request_id;
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or_else(|| anyhow!("Computer Control engine request id overflow"))?;
        let request = Request {
            protocol_version: PROTOCOL_VERSION,
            token: self.token.clone(),
            request_id,
            command,
        };
        let body = serde_json::to_vec(&request)?;
        if body.len() > MAX_JSON_BYTES {
            bail!("Computer Control engine request exceeds protocol limit");
        }
        self.stdin.write_all(&body)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        match self.responses.recv_timeout(REQUEST_TIMEOUT) {
            Ok(ResponseEvent::Response(response)) => {
                response
                    .validate(&self.token, request_id)
                    .map_err(anyhow::Error::msg)?;
                if let Some(error) = response.error {
                    bail!("{error}");
                }
                response
                    .output
                    .ok_or_else(|| anyhow!("Computer Control engine returned no output"))
            }
            Ok(ResponseEvent::Failed(error)) => bail!("{error}"),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                bail!("Computer Control engine response timed out")
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("Computer Control engine response channel closed")
            }
        }
    }

    pub(super) fn shutdown(&mut self) {
        terminate_job(&self.job);
    }
}

impl Drop for EngineProcess {
    fn drop(&mut self) {
        terminate_job(&self.job);
        let deadline = Instant::now() + DROP_WAIT;
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn start_response_reader(stdout: ChildStdout) -> mpsc::Receiver<ResponseEvent> {
    let (sender, receiver) = mpsc::sync_channel(8);
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let line = match read_bounded_line(&mut reader, MAX_RESPONSE_LINE_BYTES) {
                Ok(Some(line)) => line,
                Ok(None) => {
                    let _ = sender.send(ResponseEvent::Failed(
                        "Computer Control engine closed its response stream".to_string(),
                    ));
                    return;
                }
                Err(error) => {
                    let _ = sender.send(ResponseEvent::Failed(format!(
                        "Computer Control engine response framing failed: {error:#}"
                    )));
                    return;
                }
            };
            let Some(body) = line.strip_prefix(RESPONSE_PREFIX.as_bytes()) else {
                let _ = sender.send(ResponseEvent::Failed(
                    "Computer Control engine emitted an invalid response prefix".to_string(),
                ));
                return;
            };
            match serde_json::from_slice::<Response>(body) {
                Ok(response) => {
                    if sender.send(ResponseEvent::Response(response)).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(ResponseEvent::Failed(format!(
                        "Computer Control engine returned malformed JSON: {error}"
                    )));
                    return;
                }
            }
        }
    });
    receiver
}

fn launch_token() -> Result<String> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).context("generate Computer Control launch token")?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn create_job(child: &Child) -> Result<OwnedHandle> {
    let raw = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
        .context("create Computer Control engine job")?;
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
        .context("configure Computer Control engine job")?;
        AssignProcessToJobObject(HANDLE(job.as_raw_handle()), HANDLE(child.as_raw_handle()))
            .context("contain Computer Control engine process")?;
    }
    Ok(job)
}

fn terminate_job(job: &OwnedHandle) {
    let _ = unsafe { TerminateJobObject(HANDLE(job.as_raw_handle()), 1) };
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize Computer Control {label} '{}'", path.display()))?;
    let metadata = std::fs::symlink_metadata(&canonical)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("Computer Control {label} path is unsafe");
    }
    Ok(canonical)
}

fn canonical_dir(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize {label} directory '{}'", path.display()))?;
    let metadata = std::fs::symlink_metadata(&canonical)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("{label} directory is unsafe");
    }
    Ok(canonical)
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}
