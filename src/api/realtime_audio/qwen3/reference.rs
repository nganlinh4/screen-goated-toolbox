use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SERVER_BOOT_TIMEOUT: Duration = Duration::from_secs(90);
const SERVER_POLL_INTERVAL: Duration = Duration::from_millis(500);
const SERVER_HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const STDERR_TAIL_LIMIT: usize = 16 * 1024;
const QWEN3_SERVER_ASSET_NAME: &str = "qwen3-asr-reference-windows-x64.zip";

static QWEN_LOCAL_HEALTH_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(SERVER_HEALTH_TIMEOUT))
        .build();
    config.into()
});

static QWEN_LOCAL_TRANSCRIPTION_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(SERVER_TRANSCRIPTION_TIMEOUT))
        .build();
    config.into()
});

#[derive(Deserialize)]
struct HealthResponse {
    status: String,
}

#[derive(Clone, Deserialize)]
pub struct VerboseTranscriptionSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Deserialize)]
pub struct VerboseTranscriptionResponse {
    pub language: String,
    pub duration: f64,
    pub text: String,
    #[serde(default)]
    pub segments: Vec<VerboseTranscriptionSegment>,
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

    pub fn transcribe_audio_verbose(
        &mut self,
        audio_data: Vec<u8>,
        language: Option<&str>,
    ) -> Result<VerboseTranscriptionResponse> {
        let mut request = QWEN_LOCAL_TRANSCRIPTION_AGENT
            .post(&format!("{}/v1/audio/transcriptions/raw", self.base_url))
            .config()
            .http_status_as_error(false)
            .build()
            .header("Content-Type", "audio/wav")
            .header("X-Audio-Filename", "subtitle-source.wav")
            .header("X-Response-Format", "verbose_json");
        if let Some(language) = language {
            request = request.header("X-Language", language);
        }

        let mut response = request.send(&audio_data).map_err(|err| {
            anyhow!(
                "Qwen3 subtitle transcription request failed: {err}{}",
                self.server_status_context()
            )
        })?;

        if !response.status().is_success() {
            return Err(self
                .http_status_error("Qwen3 subtitle transcription request failed", &mut response));
        }

        response
            .body_mut()
            .read_json()
            .map_err(|err| anyhow!("Failed to parse Qwen3 subtitle response: {err}"))
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

            match QWEN_LOCAL_HEALTH_AGENT
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

    candidates.extend(local_sidecar_candidate_paths());
    candidates.push(get_qwen3_server_path());

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
    let managed_runtime = get_qwen3_server_runtime_dir();
    if server_path == get_qwen3_server_path() && managed_runtime.exists() {
        return Some(managed_runtime);
    }

    let sibling_runtime = server_path
        .parent()
        .map(|parent| parent.join("libtorch").join("lib"));
    if let Some(path) = sibling_runtime.filter(|path| path.exists()) {
        return Some(path);
    }

    if local_sidecar_candidate_paths()
        .iter()
        .any(|candidate| candidate == server_path)
    {
        return get_local_qwen3_cached_runtime_dir();
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

fn get_qwen3_server_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin")
        .join("qwen3_asr_reference")
}

fn get_qwen3_server_path() -> PathBuf {
    get_qwen3_server_dir().join("asr-server.exe")
}

fn get_qwen3_server_runtime_dir() -> PathBuf {
    get_qwen3_server_dir().join("libtorch").join("lib")
}

fn cache_runtime_root(cache_dir: &Path, name: &str) -> Option<PathBuf> {
    let variant_dir = cache_dir.join(format!("libtorch-{name}"));
    let nested_root = variant_dir.join("libtorch");
    if nested_root.join("lib").exists() {
        return Some(nested_root);
    }
    if variant_dir.join("lib").exists() {
        return Some(variant_dir);
    }
    None
}

fn get_local_qwen3_cached_runtime_dir() -> Option<PathBuf> {
    let repo_root = repo_root().ok()?;
    let cache_dir = repo_root.join("tools").join("qwen3-reference-cache");
    let variant = std::fs::read_to_string(cache_dir.join("runtime-variant.txt"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let candidates = variant
        .into_iter()
        .filter_map(|name| cache_runtime_root(&cache_dir, &name).map(|root| root.join("lib")))
        .chain(std::iter::once(cache_dir.join("libtorch").join("lib")));
    candidates.into_iter().find(|path| path.exists())
}

fn local_sidecar_candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(repo_root) = repo_root() {
        candidates.push(
            repo_root
                .join("third_party")
                .join("qwen3-asr-rs")
                .join("target")
                .join("release")
                .join("asr-server.exe"),
        );
        candidates.push(
            repo_root
                .join("dist")
                .join(QWEN3_SERVER_ASSET_NAME.trim_end_matches(".zip"))
                .join("asr-server.exe"),
        );
    }

    candidates
}

fn repo_root() -> Result<PathBuf> {
    let mut seeds = Vec::new();
    if let Ok(dir) = std::env::current_dir() {
        seeds.push(dir);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        seeds.push(parent.to_path_buf());
    }

    for seed in seeds {
        let mut dir = seed;
        loop {
            if dir.join("Cargo.toml").exists() && dir.join(".claude").exists() {
                return Ok(dir);
            }
            if !dir.pop() {
                break;
            }
        }
    }

    Err(anyhow!(
        "Could not locate Screen Goated Toolbox repository root"
    ))
}
