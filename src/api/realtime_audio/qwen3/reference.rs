use super::server;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SERVER_BOOT_TIMEOUT: Duration = Duration::from_secs(90);
const SERVER_POLL_INTERVAL: Duration = Duration::from_millis(500);
const SERVER_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const STDERR_TAIL_LIMIT: usize = 16 * 1024;

static QWEN_LOCAL_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(SERVER_REQUEST_TIMEOUT))
        .build();
    config.into()
});

#[derive(Deserialize)]
struct HealthResponse {
    status: String,
}

#[derive(Deserialize)]
struct StreamingSessionResponse {
    session_id: u64,
}

#[derive(Deserialize)]
struct LegacyStreamingTranscriptResponse {
    text: String,
}

#[derive(Deserialize)]
pub struct StreamingTranscriptResponse {
    pub language: String,
    #[serde(default)]
    pub fixed_text: String,
    #[serde(default)]
    pub draft_text: String,
    pub text: String,
}

#[derive(Serialize)]
struct CreateStreamingSessionRequest {
    chunk_size_ms: u32,
    unfixed_chunk_num: usize,
    unfixed_token_num: usize,
}

#[derive(Serialize)]
struct StreamingTranscriptionRequest<'a> {
    language: Option<&'a str>,
    finalize: bool,
}

pub struct QwenReferenceServer {
    child: Child,
    base_url: String,
    stderr_tail: Arc<Mutex<String>>,
}

impl QwenReferenceServer {
    pub fn start(model_dir: &Path) -> Result<Self> {
        let server_path = discover_server_path()?;
        let port = reserve_local_port()?;
        let base_url = format!("http://127.0.0.1:{port}");
        let mut command = Command::new(&server_path);
        command
            .arg("--model-dir")
            .arg(model_dir)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        if let Some(runtime_path) = server_runtime_path(&server_path) {
            prepend_env_path(&mut command, &runtime_path);
        }

        let mut child = command.spawn().map_err(|err| {
            anyhow!(
                "Failed to start Qwen3 reference ASR server '{}': {err}",
                server_path.display()
            )
        })?;
        let stderr_tail = spawn_stderr_drain(&mut child);

        let mut server = Self {
            child,
            base_url,
            stderr_tail,
        };
        server.wait_until_ready()?;
        Ok(server)
    }

    pub fn create_streaming_session(
        &mut self,
        chunk_size_ms: u32,
        unfixed_chunk_num: usize,
        unfixed_token_num: usize,
    ) -> Result<u64> {
        let mut response = QWEN_LOCAL_AGENT
            .post(&format!("{}/v1/audio/streaming/sessions", self.base_url))
            .config()
            .http_status_as_error(false)
            .build()
            .send_json(CreateStreamingSessionRequest {
                chunk_size_ms,
                unfixed_chunk_num,
                unfixed_token_num,
            })
            .map_err(|err| {
                anyhow!(
                    "Failed to create Qwen3 streaming session: {err}{}",
                    self.server_status_context()
                )
            })?;

        if !response.status().is_success() {
            return Err(self.http_status_error(
                "Failed to create Qwen3 streaming session",
                &mut response,
            ));
        }

        let payload: StreamingSessionResponse = response
            .body_mut()
            .read_json()
            .map_err(|err| anyhow!("Failed to parse Qwen3 streaming session response: {err}"))?;
        Ok(payload.session_id)
    }

    pub fn append_streaming_audio(&mut self, session_id: u64, samples: &[i16]) -> Result<()> {
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }

        let mut response = QWEN_LOCAL_AGENT
            .post(&format!(
                "{}/v1/audio/streaming/sessions/{session_id}/audio",
                self.base_url
            ))
            .config()
            .http_status_as_error(false)
            .build()
            .header("Content-Type", "application/octet-stream")
            .send(&bytes)
            .map_err(|err| {
                anyhow!(
                    "Failed to append audio to Qwen3 streaming session: {err}{}",
                    self.server_status_context()
                )
            })?;

        if !response.status().is_success() {
            return Err(self.http_status_error(
                "Failed to append audio to Qwen3 streaming session",
                &mut response,
            ));
        }

        Ok(())
    }

    pub fn transcribe_streaming_session(
        &mut self,
        session_id: u64,
        language: Option<&str>,
        finalize: bool,
    ) -> Result<StreamingTranscriptResponse> {
        let mut response = QWEN_LOCAL_AGENT
            .post(&format!(
                "{}/v1/audio/streaming/sessions/{session_id}/transcriptions",
                self.base_url
            ))
            .config()
            .http_status_as_error(false)
            .build()
            .send_json(StreamingTranscriptionRequest { language, finalize })
            .map_err(|err| {
                anyhow!(
                    "Qwen3 streaming transcription request failed: {err}{}",
                    self.server_status_context()
                )
            })?;

        if !response.status().is_success() {
            return Err(self.http_status_error(
                "Qwen3 streaming transcription request failed",
                &mut response,
            ));
        }

        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|err| anyhow!("Failed to read Qwen3 streaming response body: {err}"))?;

        match serde_json::from_str::<StreamingTranscriptResponse>(&body) {
            Ok(parsed) => Ok(parsed),
            Err(streaming_error) => match serde_json::from_str::<LegacyStreamingTranscriptResponse>(&body) {
                Ok(legacy) => Ok(StreamingTranscriptResponse {
                    language: String::new(),
                    fixed_text: String::new(),
                    draft_text: legacy.text.clone(),
                    text: legacy.text,
                }),
                Err(_) => Err(anyhow!(
                    "Failed to parse Qwen3 streaming response: {streaming_error}\n\nRaw response:\n{}",
                    body.trim()
                )),
            },
        }
    }

    pub fn reset_streaming_session(&mut self, session_id: u64) -> Result<()> {
        let mut response = QWEN_LOCAL_AGENT
            .post(&format!(
                "{}/v1/audio/streaming/sessions/{session_id}/reset",
                self.base_url
            ))
            .config()
            .http_status_as_error(false)
            .build()
            .send_empty()
            .map_err(|err| {
                anyhow!(
                    "Failed to reset Qwen3 streaming session: {err}{}",
                    self.server_status_context()
                )
            })?;

        if !response.status().is_success() {
            return Err(self.http_status_error(
                "Failed to reset Qwen3 streaming session",
                &mut response,
            ));
        }

        Ok(())
    }

    fn wait_until_ready(&mut self) -> Result<()> {
        let deadline = Instant::now() + SERVER_BOOT_TIMEOUT;

        while Instant::now() < deadline {
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|err| anyhow!("Failed to poll Qwen3 reference server process: {err}"))?
            {
                return Err(anyhow!(
                    "Qwen3 reference ASR server exited before becoming ready: {status}{}",
                    self.recent_stderr()
                        .as_deref()
                        .map(|text| format!("\n\nServer stderr:\n{text}"))
                        .unwrap_or_default()
                ));
            }

            match QWEN_LOCAL_AGENT
                .get(&format!("{}/health", self.base_url))
                .call()
                .and_then(|response| response.into_body().read_json::<HealthResponse>())
            {
                Ok(health) if health.status == "ok" => return Ok(()),
                Ok(_) | Err(_) => std::thread::sleep(SERVER_POLL_INTERVAL),
            }
        }

        Err(anyhow!(
            "Timed out waiting for Qwen3 reference ASR server to become healthy"
        ))
    }

    fn recent_stderr(&self) -> Option<String> {
        let stderr = self.stderr_tail.lock().ok()?.trim().to_string();
        if stderr.is_empty() {
            None
        } else {
            Some(stderr)
        }
    }

    fn server_status_context(&mut self) -> String {
        let mut context = String::new();
        if let Ok(Some(status)) = self.child.try_wait() {
            context.push_str(&format!("\n\nServer process exited: {status}"));
        }
        if let Some(stderr) = self.recent_stderr() {
            context.push_str(&format!("\n\nRecent server stderr:\n{stderr}"));
        }
        context
    }

    fn http_status_error(
        &mut self,
        prefix: &str,
        response: &mut ureq::http::Response<ureq::Body>,
    ) -> anyhow::Error {
        let status = response.status();
        let body = response
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| String::new());
        let body = body.trim();
        anyhow!(
            "{prefix}: http status: {}{}{}",
            status.as_u16(),
            if body.is_empty() {
                String::new()
            } else {
                format!("\n\nServer response:\n{body}")
            },
            self.server_status_context()
        )
    }

    pub fn shutdown(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

fn spawn_stderr_drain(child: &mut Child) -> Arc<Mutex<String>> {
    let tail = Arc::new(Mutex::new(String::new()));
    if let Some(stderr) = child.stderr.take() {
        let tail_clone = tail.clone();
        std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if line.starts_with("[QwenBackend]") || line.starts_with("[QwenAB]") {
                eprintln!("{line}");
            }
            append_stderr_tail(&tail_clone, &line);
        }
    });
    }
    tail
}

fn append_stderr_tail(tail: &Arc<Mutex<String>>, line: &str) {
    if let Ok(mut buffer) = tail.lock() {
        if !buffer.is_empty() {
            buffer.push('\n');
        }
        buffer.push_str(line);

        if buffer.len() > STDERR_TAIL_LIMIT {
            let mut trim_to = buffer.len() - STDERR_TAIL_LIMIT;
            while trim_to < buffer.len() && !buffer.is_char_boundary(trim_to) {
                trim_to += 1;
            }
            let buffer_len = buffer.len();
            buffer.drain(..trim_to.min(buffer_len));
        }
    }
}

pub fn has_discoverable_server() -> bool {
    discover_server_path().is_ok()
}

impl Drop for QwenReferenceServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn reserve_local_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|err| anyhow!("Failed to allocate a localhost port for Qwen3 ASR: {err}"))?;
    let port = listener
        .local_addr()
        .map_err(|err| anyhow!("Failed to read localhost port for Qwen3 ASR: {err}"))?
        .port();
    drop(listener);
    Ok(port)
}

fn discover_server_path() -> Result<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(path) = std::env::var("SGT_QWEN3_ASR_SERVER")
        && !path.trim().is_empty()
    {
        candidates.push(PathBuf::from(path));
    }

    candidates.extend(server::local_sidecar_candidate_paths());
    candidates.push(server::get_qwen3_server_path());

    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        candidates.push(exe_dir.join("asr-server.exe"));
        candidates.push(exe_dir.join("qwen3-asr-server.exe"));
    }

    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| {
            anyhow!(
                "Qwen3 reference ASR server executable was not found. Install the SGT-managed sidecar from Downloaded Tools, build 'third_party/qwen3-asr-rs', or set SGT_QWEN3_ASR_SERVER."
            )
        })
}

fn server_runtime_path(server_path: &Path) -> Option<PathBuf> {
    let managed_runtime = server::get_qwen3_server_runtime_dir();
    if server_path == server::get_qwen3_server_path() && managed_runtime.exists() {
        return Some(managed_runtime);
    }

    let sibling_runtime = server_path
        .parent()
        .map(|parent| parent.join("libtorch").join("lib"));
    if let Some(path) = sibling_runtime.filter(|path| path.exists()) {
        return Some(path);
    }

    if server::local_sidecar_candidate_paths()
        .iter()
        .any(|candidate| candidate == server_path)
    {
        return server::get_local_qwen3_cached_runtime_dir();
    }

    None
}

fn prepend_env_path(command: &mut Command, runtime_dir: &Path) {
    let separator = if cfg!(windows) { ";" } else { ":" };
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut combined = runtime_dir.as_os_str().to_owned();
    if !existing.is_empty() {
        combined.push(separator);
        combined.push(existing);
    }
    command.env("PATH", combined);
}
