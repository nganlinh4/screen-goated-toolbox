use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod preparation;
mod process;

use process::{CommandNoWindowExt as _, run_runtime_operation};

pub(super) const RUNTIME_EXE_NAME: &str = "sgt_creation_runtime.exe";
const MAX_ASSET_BYTES: u64 = 100 * 1024 * 1024;
const MAX_PARALLEL_JOBS: usize = 2;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartJobRequest {
    pub image_path: String,
    pub output_dir: Option<String>,
    pub polycount: u32,
    pub mode: String,
    pub output_format: String,
    pub auto_segment: bool,
    pub segmentation_mode: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JobStatus {
    pub job_id: Option<String>,
    pub stage: String,
    pub progress_text: String,
    pub phase: Option<String>,
    pub workspace_state: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub estimated_total_ms: Option<u64>,
    pub progress_ratio: Option<f64>,
    pub timing_sample_count: Option<u64>,
    pub output_path: Option<String>,
    pub output_name: Option<String>,
    pub preview_path: Option<String>,
    pub source_image_path: Option<String>,
    pub is_segmented: bool,
    pub can_segment: bool,
    pub error: Option<String>,
    pub runtime_status: String,
}

#[derive(Debug, Clone)]
struct Continuation {
    task_id: String,
    profile_dir: String,
    image_path: String,
    output_dir: PathBuf,
    previous_output_path: PathBuf,
    preview_path: Option<String>,
}

#[derive(Default)]
struct RuntimeState {
    jobs: HashMap<String, JobStatus>,
    job_order: Vec<String>,
    pids: HashMap<String, u32>,
    continuations: HashMap<String, Continuation>,
}

impl RuntimeState {
    fn running_count(&self) -> usize {
        self.jobs
            .values()
            .filter(|status| status_is_busy(&status.stage))
            .count()
    }

    fn insert_job(&mut self, job_id: String, status: JobStatus) {
        if !self.jobs.contains_key(&job_id) {
            self.job_order.push(job_id.clone());
        }
        self.jobs.insert(job_id, status);
    }

    fn latest_status(&self) -> Option<JobStatus> {
        self.job_order
            .iter()
            .rev()
            .find_map(|job_id| self.jobs.get(job_id).cloned())
    }
}

fn status_is_busy(stage: &str) -> bool {
    matches!(
        stage,
        "preparing" | "visualizing" | "generating" | "segmenting" | "finalizing"
    )
}

enum RuntimeOperation {
    Generate {
        request: StartJobRequest,
        output_dir: PathBuf,
    },
    Segment {
        continuation: Continuation,
    },
}

impl RuntimeOperation {
    fn source_image_path(&self) -> &str {
        match self {
            Self::Generate { request, .. } => &request.image_path,
            Self::Segment { continuation } => &continuation.image_path,
        }
    }
}

static STATE: LazyLock<Mutex<RuntimeState>> = LazyLock::new(|| Mutex::new(RuntimeState::default()));
static JOB_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn runtime_exe_path() -> PathBuf {
    crate::overlay::creation_runtime::runtime_exe_path()
}

pub(super) fn prepare_runtime() -> String {
    preparation::prepare_runtime()
}

pub(super) fn runtime_preparation_status() -> String {
    preparation::runtime_preparation_status()
}

pub(super) fn start_preparation_maintainer(install_if_missing: bool) {
    preparation::start_preparation_maintainer(install_if_missing);
}

fn dev_runtime_exe_path() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("native")
        .join("sgt_3d_generator_runtime")
        .join("target");
    ["debug", "release"]
        .into_iter()
        .map(|profile| root.join(profile).join(RUNTIME_EXE_NAME))
        .find(|path| path.is_file())
}

fn runtime_command() -> Option<Command> {
    #[cfg(debug_assertions)]
    if let Some(path) = dev_runtime_exe_path() {
        return Some(Command::new(path));
    }
    if crate::overlay::creation_runtime::is_runtime_installed() {
        Some(Command::new(runtime_exe_path()))
    } else {
        dev_runtime_exe_path().map(Command::new)
    }
}

fn runtime_status_label() -> String {
    if crate::overlay::creation_runtime::is_runtime_installed() {
        "installed".to_string()
    } else if dev_runtime_exe_path().is_some() {
        "dev-native".to_string()
    } else {
        "missing".to_string()
    }
}

pub(super) fn default_output_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| crate::paths::app_local_data_dir().join("3d-generator"))
}

fn next_job_id() -> String {
    format!(
        "mesh_{}_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        JOB_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn idle_status() -> JobStatus {
    JobStatus {
        job_id: None,
        stage: "idle".to_string(),
        progress_text: "Ready to create.".to_string(),
        phase: None,
        workspace_state: None,
        elapsed_ms: None,
        estimated_total_ms: None,
        progress_ratio: None,
        timing_sample_count: None,
        output_path: None,
        output_name: None,
        preview_path: None,
        source_image_path: None,
        is_segmented: false,
        can_segment: false,
        error: None,
        runtime_status: runtime_status_label(),
    }
}

pub(super) fn job_status(job_id: Option<&str>) -> JobStatus {
    STATE
        .lock()
        .ok()
        .and_then(|state| match job_id {
            Some(job_id) => state.jobs.get(job_id).cloned(),
            None => state.latest_status(),
        })
        .unwrap_or_else(idle_status)
}

pub(super) fn job_statuses() -> Vec<JobStatus> {
    STATE
        .lock()
        .map(|state| {
            state
                .job_order
                .iter()
                .filter_map(|job_id| state.jobs.get(job_id).cloned())
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn start_job(mut request: StartJobRequest) -> Result<JobStatus, String> {
    request.polycount = request.polycount.clamp(500, 20_000);
    if request.image_path.trim().is_empty() {
        return Err("Pick an image first.".to_string());
    }
    if !PathBuf::from(&request.image_path).exists() {
        return Err(format!("Image does not exist: {}", request.image_path));
    }

    let output_dir = request
        .output_dir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_output_dir);
    std::fs::create_dir_all(&output_dir).map_err(|err| {
        format!(
            "Could not create output directory {}: {err}",
            output_dir.display()
        )
    })?;

    if STATE
        .lock()
        .map(|state| state.running_count() >= MAX_PARALLEL_JOBS)
        .unwrap_or(true)
    {
        return Err("Both model creation workers are busy.".to_string());
    }

    let job_id = next_job_id();
    let runtime_status = runtime_status_label();

    let status = JobStatus {
        job_id: Some(job_id.clone()),
        stage: "preparing".to_string(),
        progress_text: "Preparing creation.".to_string(),
        phase: Some("preparing".to_string()),
        workspace_state: Some("checking".to_string()),
        elapsed_ms: Some(0),
        estimated_total_ms: None,
        progress_ratio: Some(0.0),
        timing_sample_count: None,
        output_path: None,
        output_name: None,
        preview_path: None,
        source_image_path: Some(request.image_path.clone()),
        is_segmented: false,
        can_segment: false,
        error: None,
        runtime_status,
    };
    if let Ok(mut state) = STATE.lock() {
        state.insert_job(job_id.clone(), status.clone());
    }
    std::thread::spawn(move || {
        run_runtime_operation(
            job_id,
            RuntimeOperation::Generate {
                request,
                output_dir,
            },
        )
    });
    Ok(status)
}

pub(super) fn start_segmentation(continuation_id: &str) -> Result<JobStatus, String> {
    if continuation_id.trim().is_empty() {
        return Err("The model continuation is missing.".to_string());
    }
    let (continuation, preview_path, runtime_status) = {
        let mut state = STATE
            .lock()
            .map_err(|_| "3D generator state is unavailable")?;
        if state.running_count() >= MAX_PARALLEL_JOBS {
            return Err("Both model creation workers are busy.".to_string());
        }
        let continuation = state
            .continuations
            .remove(continuation_id)
            .ok_or_else(|| "This model can no longer be separated into parts.".to_string())?;
        let preview_path = continuation.preview_path.clone();
        (continuation, preview_path, runtime_status_label())
    };

    let job_id = next_job_id();
    let status = JobStatus {
        job_id: Some(job_id.clone()),
        stage: "segmenting".to_string(),
        progress_text: "Separating model parts.".to_string(),
        phase: Some("separation".to_string()),
        workspace_state: None,
        elapsed_ms: Some(0),
        estimated_total_ms: None,
        progress_ratio: Some(0.0),
        timing_sample_count: None,
        output_path: Some(
            continuation
                .previous_output_path
                .to_string_lossy()
                .to_string(),
        ),
        output_name: continuation
            .previous_output_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string()),
        preview_path,
        source_image_path: Some(continuation.image_path.clone()),
        is_segmented: false,
        can_segment: true,
        error: None,
        runtime_status,
    };
    if let Ok(mut state) = STATE.lock() {
        state.insert_job(job_id.clone(), status.clone());
    }
    std::thread::spawn(move || {
        run_runtime_operation(job_id, RuntimeOperation::Segment { continuation })
    });
    Ok(status)
}

pub(super) fn cancel_job(job_id: Option<&str>) -> JobStatus {
    let (pids, status) = if let Ok(mut state) = STATE.lock() {
        let targets: Vec<String> = match job_id {
            Some(job_id) => vec![job_id.to_string()],
            None => state
                .jobs
                .iter()
                .filter(|(_, status)| status_is_busy(&status.stage))
                .map(|(job_id, _)| job_id.clone())
                .collect(),
        };
        let mut pids = Vec::new();
        for target in &targets {
            if let Some(status) = state.jobs.get_mut(target)
                && status_is_busy(&status.stage)
            {
                status.stage = "cancelled".to_string();
                status.progress_text = "Cancelled.".to_string();
                status.error = None;
            }
            if let Some(pid) = state.pids.remove(target) {
                pids.push(pid);
            }
        }
        let status = job_id
            .and_then(|job_id| state.jobs.get(job_id).cloned())
            .or_else(|| state.latest_status())
            .unwrap_or_else(idle_status);
        (pids, status)
    } else {
        (Vec::new(), idle_status())
    };

    for pid in pids {
        let mut command = Command::new("taskkill");
        command
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags_windows();
        let _ = command.status();
    }
    status
}

pub(super) fn read_asset(path: &str) -> Result<Value, String> {
    let path = PathBuf::from(path);
    let metadata = std::fs::metadata(&path)
        .map_err(|err| format!("Could not read {}: {err}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    if metadata.len() > MAX_ASSET_BYTES {
        return Err(format!("Asset is too large to preview: {}", path.display()));
    }
    let mime = match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "glb" => "model/gltf-binary",
        _ => "application/octet-stream",
    };
    let bytes =
        std::fs::read(&path).map_err(|err| format!("Could not read {}: {err}", path.display()))?;
    Ok(serde_json::json!({
        "dataUrl": format!("data:{mime};base64,{}", general_purpose::STANDARD.encode(&bytes)),
        "sizeBytes": bytes.len(),
    }))
}

pub(super) fn open_output(kind: &str, requested_path: Option<&str>) -> Result<(), String> {
    let path = requested_path
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| job_status(None).output_path.map(PathBuf::from))
        .unwrap_or_else(default_output_dir);
    let target = if kind == "folder" {
        if path.is_file() {
            path.parent()
                .map(PathBuf::from)
                .unwrap_or_else(default_output_dir)
        } else {
            path
        }
    } else {
        path
    };
    open::that(&target).map_err(|err| format!("Could not open {}: {err}", target.display()))
}
