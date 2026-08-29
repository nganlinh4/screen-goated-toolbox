//! Authenticated loopback entrance for the visible Computer Control runtime.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use serde_json::{Value, json};

use super::runtime::control::{TextCommandDisposition, text_command_queued, turn_idle};

const MAXIMUM_REQUEST_BYTES: usize = 8_192;
const DISCOVERY_FILE: &str = "computer-control-api.json";

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
        listener
            .set_nonblocking(true)
            .context("configure Computer Control loopback API")?;
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
                if let Err(error) = handle_connection(stream, token) {
                    crate::log_info!("[ComputerControlApi] Request failed: {error:#}");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => {
                crate::log_info!("[ComputerControlApi] Listener stopped: {error}");
                return;
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream, token: &str) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = match read_request(&mut stream) {
        Ok(request) => request,
        Err(error) if error.to_string().contains("exceeds byte limit") => {
            return write_response(
                &mut stream,
                413,
                json!({"ok": false, "code": "request_too_large"}),
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

struct Request {
    method: String,
    path: String,
    authorization: Option<String>,
    content_type: Option<String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Result<Request> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1_024];
    let header_end = loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Err(anyhow!("request ended before headers"));
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAXIMUM_REQUEST_BYTES {
            return Err(anyhow!("request exceeds byte limit"));
        }
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end])?;
    let mut lines = headers.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default().to_string();
    let path = request_line.next().unwrap_or_default().to_string();
    let mut content_length = 0_usize;
    let mut authorization = None;
    let mut content_type = None;
    for line in lines.filter(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "authorization" => authorization = Some(value.trim().to_string()),
            "content-type" => {
                content_type = Some(
                    value
                        .split(';')
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_ascii_lowercase(),
                )
            }
            "content-length" => content_length = value.trim().parse()?,
            _ => {}
        }
    }
    if header_end + content_length > MAXIMUM_REQUEST_BYTES {
        return Err(anyhow!("request exceeds byte limit"));
    }
    while bytes.len() < header_end + content_length {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Err(anyhow!("request body ended early"));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    Ok(Request {
        method,
        path,
        authorization,
        content_type,
        body: bytes[header_end..header_end + content_length].to_vec(),
    })
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
        "launch" => {
            super::show_overlay();
            if super::is_active() {
                response(202, status_body())
            } else {
                response(409, json!({"ok": false, "code": "startup_rejected"}))
            }
        }
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
        TextCommandDisposition::Queued => response(202, json!({"ok": true, "state": "queued"})),
        TextCommandDisposition::Busy => response(409, json!({"ok": false, "code": "busy"})),
        TextCommandDisposition::Invalid => {
            response(400, json!({"ok": false, "code": "invalid_prompt"}))
        }
    }
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
mod tests {
    use super::*;

    #[test]
    fn routing_rejects_unknown_and_untyped_requests() {
        let request = Request {
            method: "POST".into(),
            path: "/v1/control".into(),
            authorization: None,
            content_type: Some("application/json".into()),
            body: br#"{"operation":"future"}"#.to_vec(),
        };
        assert_eq!(route(&request).status, 400);
        let mut wrong_type = request;
        wrong_type.content_type = Some("text/plain".into());
        assert_eq!(route(&wrong_type).status, 415);
    }
}
