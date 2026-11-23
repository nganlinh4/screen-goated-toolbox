use crate::component_registry::{canonical_regular_dir, canonical_regular_file};
use crate::process_job::{HelperCommand, Input, Output as ProcessOutput, OwnedProcess};
use std::io::{BufReader, Write as _};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Duration;

use super::DataRejection;
use super::framing::read_bounded_line;
use anyhow::{Context, Result, anyhow, bail};
use sgt_computer_control_protocol::{
    Command, MAX_JSON_BYTES, MAX_RESPONSE_LINE_BYTES, Output, PROTOCOL_VERSION, RESPONSE_PREFIX,
    Request, Response, TOKEN_BYTES,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const DROP_WAIT: Duration = Duration::from_millis(500);
/// Bound for both the Shutdown reply and the clean exit that follows it.
const SHUTDOWN_WAIT: Duration = Duration::from_millis(500);
const ENGINE_VERSION: &str = "1.0.0";

enum ResponseEvent {
    Response(Response),
    Failed(String),
}

enum EngineReply {
    Output(Output),
    Rejected(String),
}

pub(super) struct EngineProcess {
    pub(super) tool_declarations: std::collections::HashMap<String, serde_json::Value>,
    child: OwnedProcess,
    stdin: Input,
    responses: mpsc::Receiver<ResponseEvent>,
    token: String,
    next_request_id: u64,
}

impl EngineProcess {
    pub(super) fn launch(cancelled: &AtomicBool) -> Result<Self> {
        let component =
            crate::component_registry::computer_control::ensure_engine_with_badge(cancelled)?;
        let executable = canonical_regular_file(component.executable(), "Computer Control engine")?;
        let system_root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| anyhow!("SystemRoot is unavailable"))?;
        let system32 = canonical_regular_dir(&system_root.join("System32"), "Windows System32")?;
        let token = launch_token()?;
        let workspace = crate::component_registry::worker_workspace(
            crate::component_registry::computer_control::ID,
        )?;
        let mut command = HelperCommand::new(&executable);
        command
            .arg("--stdio")
            .current_dir(workspace)
            .env_clear()
            .env("SystemRoot", &system_root)
            .env("WINDIR", &system_root)
            .env("PATH", &system32)
            .pipe_stdin()
            .stderr_null();
        crate::paths::configure_isolated_child_environment(&mut command)?;
        command.env("SGT_CC_ENGINE_LAUNCH_TOKEN", &token);
        let mut child = OwnedProcess::spawn_helper(&command, component)
            .with_context(|| format!("start Computer Control engine '{}'", executable.display()))?;
        let stdin = child
            .take_stdin()
            .context("Computer Control engine stdin missing")?;
        let stdout = child
            .take_stdout()
            .context("Computer Control engine stdout missing")?;
        child.resume()?;
        let responses = start_response_reader(stdout);
        let mut process = Self {
            tool_declarations: std::collections::HashMap::new(),
            child,
            stdin,
            responses,
            token,
            next_request_id: 1,
        };
        // The host is the compatibility authority: it pins ENGINE_VERSION and
        // the architecture below. `host_version` is a required, informational
        // field of the frozen v1 wire contract that engine 1.0.0 deserializes
        // strictly, so it stays on the wire and engines must not gate on it.
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
        self.request_within(command, REQUEST_TIMEOUT)
    }

    fn request_within(&mut self, command: Command, timeout: Duration) -> Result<Output> {
        command.validate().map_err(DataRejection::request)?;
        let request_id = self.next_request_id;
        let next_request_id = self
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
            return Err(DataRejection::request(
                "Computer Control engine request exceeds protocol limit",
            )
            .into());
        }
        // No request or ID is committed until local admission has succeeded.
        self.next_request_id = next_request_id;
        match self.request_inner(request_id, &body, timeout) {
            Ok(EngineReply::Output(output)) => Ok(output),
            Ok(EngineReply::Rejected(error)) => Err(DataRejection::application(error).into()),
            Err(error) => {
                let _ = self.child.terminate();
                Err(error)
            }
        }
    }

    fn request_inner(
        &mut self,
        request_id: u64,
        body: &[u8],
        timeout: Duration,
    ) -> Result<EngineReply> {
        if let Some(status) = self.child.try_wait()? {
            bail!("Computer Control engine exited with {status}");
        }
        self.stdin.write_all(body)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        match self.responses.recv_timeout(timeout) {
            Ok(ResponseEvent::Response(response)) => {
                response
                    .validate(&self.token, request_id)
                    .map_err(anyhow::Error::msg)?;
                if let Some(error) = response.error {
                    return Ok(EngineReply::Rejected(error));
                }
                response
                    .output
                    .map(EngineReply::Output)
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

    /// Ask the engine to exit through its protocol first, then always retire
    /// the job so an engine that ignores the request cannot outlive the
    /// session. Returns whether the engine acknowledged and exited on its own.
    pub(super) fn shutdown(&mut self) -> bool {
        let acknowledged = matches!(
            self.request_within(Command::Shutdown, SHUTDOWN_WAIT),
            Ok(Output::Shutdown)
        ) && self.exited_within(SHUTDOWN_WAIT);
        let _ = self.child.terminate();
        acknowledged
    }

    fn exited_within(&mut self, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while !self.child.stopped() {
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        true
    }
}

impl Drop for EngineProcess {
    fn drop(&mut self) {
        let _ = self.child.terminate();
        let _ = self.child.stop_within(DROP_WAIT);
    }
}

fn start_response_reader(stdout: ProcessOutput) -> mpsc::Receiver<ResponseEvent> {
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

#[cfg(test)]
#[path = "process_native_tests.rs"]
mod native_tests;
