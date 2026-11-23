//! Authenticated loopback entrance for the visible Computer Control runtime.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use serde_json::{Value, json};

use super::overlay::SessionUnavailable;
use super::runtime::control::{TextCommandDisposition, text_command_queued, turn_idle};

const MAXIMUM_REQUEST_BYTES: usize = 8_192;
const DISCOVERY_FILE: &str = "computer-control-api.json";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

mod request;
use request::{ReadError, Request, read_request};

pub(crate) struct ServerGuard {
    stop: Arc<AtomicBool>,
    endpoint: String,
    token: String,
    worker: Option<JoinHandle<()>>,
    discovery_path: PathBuf,
}

impl ServerGuard {
    pub(crate) fn start() -> Result<Self> {
        let listener =
            TcpListener::bind("127.0.0.1:0").context("bind Computer Control loopback API")?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let token = random_token()?;
        let discovery_path = crate::paths::app_runtime_local_data_dir().join(DISCOVERY_FILE);
        publish_discovery(&discovery_path, &endpoint, &token)?;

        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_token = token.clone();
        let worker = std::thread::spawn(move || serve(listener, &worker_token, &worker_stop));
        Ok(Self {
            stop,
            endpoint,
            token,
            worker: Some(worker),
            discovery_path,
        })
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.endpoint.trim_start_matches("http://"));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        remove_owned_discovery(&self.discovery_path, &self.token);
    }
}

fn serve(listener: TcpListener, token: &str, stop: &AtomicBool) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                if let Err(error) = handle_connection(stream, token) {
                    cc_log!("[ComputerControlApi] Request failed: {error:#}");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                cc_log!("[ComputerControlApi] Listener stopped: {error}");
                return;
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream, token: &str) -> Result<()> {
    let deadline = Instant::now() + REQUEST_TIMEOUT;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = match read_request(&mut stream, deadline) {
        Ok(request) => request,
        Err(error) if error.downcast_ref::<ReadError>() == Some(&ReadError::TooLarge) => {
            return write_response(
                &mut stream,
                413,
                json!({"ok": false, "code": "request_too_large"}),
            );
        }
        Err(error) if error.downcast_ref::<ReadError>() == Some(&ReadError::Timeout) => {
            return write_response(
                &mut stream,
                408,
                json!({"ok": false, "code": "request_timeout"}),
            );
        }
        Err(error) => return Err(error),
    };
    let expected_authorization = format!("Bearer {token}");
    if request.authorization.as_deref() != Some(expected_authorization.as_str()) {
        return write_response(
            &mut stream,
            401,
            json!({"ok": false, "code": "unauthorized"}),
        );
    }
    let response = route(&request);
    write_response(&mut stream, response.status, response.body)
}

struct ApiResponse {
    status: u16,
    body: Value,
}

#[derive(Deserialize)]
struct ControlRequest {
    operation: String,
    #[serde(default)]
    prompt: Option<String>,
}

fn route(request: &Request) -> ApiResponse {
    if request.method == "GET" && request.path == "/v1/status" {
        return response(200, status_body());
    }
    if request.method != "POST" || request.path != "/v1/control" {
        return response(404, json!({"ok": false, "code": "not_found"}));
    }
    if request.content_type.as_deref() != Some("application/json") {
        return response(415, json!({"ok": false, "code": "json_required"}));
    }
    let Ok(command) = serde_json::from_slice::<ControlRequest>(&request.body) else {
        return response(400, json!({"ok": false, "code": "invalid_request"}));
    };
    match command.operation.as_str() {
        "launch" => match super::overlay::start_session() {
            Ok(()) => response(202, status_body()),
            Err(reason) => unavailable(reason),
        },
        "submit_turn" => match command.prompt {
            Some(prompt) => submit(prompt),
            None => response(400, json!({"ok": false, "code": "prompt_required"})),
        },
        "status" => response(200, status_body()),
        "cancel" | "stop" => {
            super::stop_overlay();
            response(202, json!({"ok": true, "state": "stopping"}))
        }
        _ => response(400, json!({"ok": false, "code": "unknown_operation"})),
    }
}

fn submit(prompt: String) -> ApiResponse {
    match super::overlay::launch_and_submit(prompt) {
        Ok(TextCommandDisposition::Queued) => response(202, json!({"ok": true, "state": "queued"})),
        Ok(TextCommandDisposition::Busy) => response(409, json!({"ok": false, "code": "busy"})),
        Ok(TextCommandDisposition::Invalid) => {
            response(400, json!({"ok": false, "code": "invalid_prompt"}))
        }
        Err(reason) => unavailable(reason),
    }
}

/// A launch or command the session cannot take, with a code clients can act
/// on: retry after the old session exits, or fix the API key.
fn unavailable(reason: SessionUnavailable) -> ApiResponse {
    let code = match reason {
        SessionUnavailable::Stopping => "stopping",
        SessionUnavailable::Credentials => "credentials_required",
    };
    response(409, json!({"ok": false, "code": code}))
}

fn status_body() -> Value {
    json!({
        "ok": true,
        "active": super::is_active(),
        "idle": turn_idle() && !text_command_queued(),
        "queued": text_command_queued(),
    })
}

fn response(status: u16, body: Value) -> ApiResponse {
    ApiResponse { status, body }
}

fn write_response(stream: &mut TcpStream, status: u16, body: Value) -> Result<()> {
    let body = serde_json::to_vec(&body)?;
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    Ok(())
}

fn random_token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).context("generate Computer Control API token")?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn publish_discovery(path: &std::path::Path, endpoint: &str, token: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("discovery path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let body = serde_json::to_vec_pretty(&json!({
        "schemaVersion": 1,
        "endpoint": endpoint,
        "token": token,
        "processId": std::process::id(),
    }))?;
    std::fs::write(&temporary, body)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn remove_owned_discovery(path: &std::path::Path, token: &str) {
    let owned = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| {
            value
                .get("token")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .is_some_and(|candidate| candidate == token);
    if owned {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests;
