use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write as _};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod asset_io;
mod output_guard;
mod preparation;
mod public_progress;
#[cfg(test)]
mod tests;
pub(super) use asset_io::{open_output, read_asset};
use output_guard::{cancelled_job_output, validate_runtime_result};
pub(super) use preparation::{prepare_runtime, runtime_preparation_status};

const MAX_PARALLEL_JOBS: usize = 2;
const MAX_QUEUED_JOBS: usize = 50;
const OPERATION: &str = "create_image_from_reference";
const MAX_PROMPT_CHARACTERS: usize = 4_000;
const MAX_REFERENCE_IMAGES: usize = 20;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartJobRequest {
    #[serde(default)]
    pub image_paths: Vec<String>,
    #[serde(default)]
    pub image_path: Option<String>,
    pub output_dir: Option<String>,
    pub prompt: String,
    #[serde(default)]
    output_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JobStatus {
    pub job_id: String,
    pub operation: String,
    pub stage: String,
    pub progress_text: String,
    pub progress_key: Option<String>,
    pub phase: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub estimated_total_ms: Option<u64>,
    pub progress_ratio: Option<f64>,
    pub output_path: Option<String>,
    pub output_name: Option<String>,
    pub source_image_path: String,
    pub source_image_paths: Vec<String>,
    pub prompt: String,
    pub mime_type: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub error: Option<String>,
}

#[derive(Default)]
struct RuntimeState {
    jobs: HashMap<String, JobStatus>,
    requests: HashMap<String, StartJobRequest>,
    order: Vec<String>,
    pids: HashMap<String, u32>,
}

impl RuntimeState {
    fn running_count(&self) -> usize {
        self.jobs.values().filter(|job| is_busy(&job.stage)).count()
    }

    fn pending_count(&self) -> usize {
        self.jobs
            .values()
            .filter(|job| job.stage == "queued" || is_busy(&job.stage))
            .count()
    }
}

static STATE: LazyLock<Mutex<RuntimeState>> = LazyLock::new(|| Mutex::new(RuntimeState::default()));
static JOB_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn runtime_command() -> Option<Command> {
    crate::overlay::creation_runtime::shared_runtime_path().map(Command::new)
}

pub(super) fn default_output_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| crate::paths::app_local_data_dir().join("images"))
}

fn next_job_id() -> String {
    format!(
        "image_{}_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        JOB_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

pub(super) fn start_job(mut request: StartJobRequest) -> Result<JobStatus, String> {
    normalize_reference_paths(&mut request)?;
    request.prompt = request.prompt.trim().to_string();
    if request.prompt.is_empty() {
        return Err("Describe the image you want to create.".to_string());
    }
    if request.prompt.chars().count() > MAX_PROMPT_CHARACTERS {
        return Err("Image instructions are too long.".to_string());
    }
    let output_dir = request
        .output_dir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_output_dir);
    std::fs::create_dir_all(&output_dir)
        .map_err(|error| format!("Could not create {}: {error}", output_dir.display()))?;
    request.output_dir = Some(output_dir.to_string_lossy().to_string());

    let job_id = next_job_id();
    let filename_suffix = job_id.strip_prefix("image_").unwrap_or(job_id.as_str());
    request.output_name = Some(format!("Created Image {filename_suffix}.png"));
    let status = JobStatus {
        job_id: job_id.clone(),
        operation: OPERATION.to_string(),
        stage: "queued".to_string(),
        progress_text: public_progress::text("queued").to_string(),
        progress_key: Some(public_progress::key("queued")),
        phase: Some("queued".to_string()),
        elapsed_ms: Some(0),
        estimated_total_ms: Some(180_000),
        progress_ratio: Some(0.0),
        output_path: None,
        output_name: None,
        source_image_path: request.image_paths.first().cloned().unwrap_or_default(),
        source_image_paths: request.image_paths.clone(),
        prompt: request.prompt.clone(),
        mime_type: None,
        width: None,
        height: None,
        error: None,
    };
    {
        let mut state = STATE
            .lock()
            .map_err(|_| "Image job state is unavailable".to_string())?;
        if state.pending_count() >= MAX_QUEUED_JOBS {
            return Err("The image queue is full.".to_string());
        }
        state.order.push(job_id.clone());
        state.jobs.insert(job_id.clone(), status);
        state.requests.insert(job_id.clone(), request);
    }
    schedule_next();
    job_status(&job_id).ok_or_else(|| "Image job could not be queued.".to_string())
}

fn normalize_reference_paths(request: &mut StartJobRequest) -> Result<(), String> {
    let mut references = std::mem::take(&mut request.image_paths);
    if let Some(legacy) = request.image_path.take() {
        references.push(legacy);
    }
    let mut seen = std::collections::HashSet::new();
    references.retain_mut(|path| {
        *path = path.trim().to_string();
        !path.is_empty() && seen.insert(path.to_lowercase())
    });
    if references.len() > MAX_REFERENCE_IMAGES {
        return Err(format!(
            "An image session supports up to {MAX_REFERENCE_IMAGES} references."
        ));
    }
    if references.iter().any(|path| !PathBuf::from(path).is_file()) {
        return Err("One or more reference images are unavailable.".to_string());
    }
    request.image_path = references.first().cloned();
    request.image_paths = references;
    Ok(())
}

pub(super) fn job_statuses() -> Vec<JobStatus> {
    STATE
        .lock()
        .map(|state| {
            state
                .order
                .iter()
                .filter_map(|id| state.jobs.get(id).cloned())
                .collect()
        })
        .unwrap_or_default()
}

fn job_status(job_id: &str) -> Option<JobStatus> {
    STATE
        .lock()
        .ok()
        .and_then(|state| state.jobs.get(job_id).cloned())
}

fn job_cancelled(job_id: &str) -> bool {
    job_status(job_id).is_some_and(|status| status.stage == "cancelled")
}

pub(super) fn remap_result_path(previous: &str, current: &str) {
    let current_name = PathBuf::from(current)
        .file_name()
        .map(|name| name.to_string_lossy().to_string());
    if let Ok(mut state) = STATE.lock() {
        for job in state.jobs.values_mut() {
            if job
                .output_path
                .as_deref()
                .is_some_and(|path| path.eq_ignore_ascii_case(previous))
            {
                job.output_path = Some(current.to_string());
                job.output_name = current_name.clone();
            }
        }
    }
}

pub(super) fn forget_result_path(path: &str) {
    if let Ok(mut state) = STATE.lock() {
        for job in state.jobs.values_mut() {
            if job
                .output_path
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(path))
            {
                job.output_path = None;
                job.output_name = None;
            }
        }
    }
}

fn update_progress(job_id: &str, value: &Value) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    let Some(job) = state.jobs.get_mut(job_id) else {
        return;
    };
    if job.stage == "cancelled" {
        return;
    }
    if let Some(stage) = value.get("stage").and_then(Value::as_str) {
        job.stage = public_progress::stage(stage).to_string();
        job.progress_text = public_progress::text(&job.stage).to_string();
        job.progress_key = Some(public_progress::key(&job.stage));
        job.phase = Some(job.stage.clone());
    }
    job.elapsed_ms = value
        .get("elapsedMs")
        .and_then(Value::as_u64)
        .or(job.elapsed_ms);
    job.estimated_total_ms = value
        .get("estimatedTotalMs")
        .and_then(Value::as_u64)
        .or(job.estimated_total_ms);
    job.progress_ratio = value
        .get("progressRatio")
        .and_then(Value::as_f64)
        .or(job.progress_ratio);
}

fn finish(job_id: &str, result: Result<Value, String>) {
    let mut completed = None;
    let mut cancelled_output = None;
    if let Ok(mut state) = STATE.lock() {
        state.pids.remove(job_id);
        let request = state.requests.remove(job_id);
        if let Some(job) = state.jobs.get_mut(job_id) {
            if job.stage == "cancelled" {
                cancelled_output = request.as_ref().and_then(cancelled_job_output);
            } else {
                match result {
                    Ok(value) => {
                        job.stage = "done".to_string();
                        job.progress_text = "Image ready".to_string();
                        job.progress_key = Some("image.ready".to_string());
                        job.phase = Some("complete".to_string());
                        job.progress_ratio = Some(1.0);
                        job.output_path = value
                            .get("outputPath")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        job.output_name = value
                            .get("outputName")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        job.mime_type = value
                            .get("mimeType")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        job.width = value
                            .get("width")
                            .and_then(Value::as_u64)
                            .and_then(|value| u32::try_from(value).ok());
                        job.height = value
                            .get("height")
                            .and_then(Value::as_u64)
                            .and_then(|value| u32::try_from(value).ok());
                        completed = Some(job.clone());
                    }
                    Err(_error) => {
                        job.stage = "failed".to_string();
                        job.progress_text = public_progress::text("failed").to_string();
                        job.progress_key = Some(public_progress::key("failed"));
                        job.phase = Some("failed".to_string());
                        job.error = Some("Image creation could not finish. Try again.".to_string());
                    }
                }
            }
        }
    }
    if let Some(output) = cancelled_output {
        let _ = std::fs::remove_file(output);
    }
    if let Some(job) = completed
        && let Some(output_path) = job.output_path.as_deref()
        && let Err(error) = crate::overlay::generation_history::record(
            "image",
            &job.source_image_path,
            output_path,
            json!({
                "operation": job.operation,
                "prompt": job.prompt,
                "sourceImagePaths": job.source_image_paths,
                "mimeType": job.mime_type,
                "width": job.width,
                "height": job.height,
            }),
        )
    {
        crate::log_info!("[Image creator] Could not record result history: {error}");
    }
    preparation::start_preparation();
    schedule_next();
}

fn schedule_next() {
    let next = {
        let Ok(mut state) = STATE.lock() else {
            return;
        };
        if state.running_count() >= MAX_PARALLEL_JOBS {
            return;
        }
        let Some(job_id) = state
            .order
            .iter()
            .find(|id| state.jobs.get(*id).is_some_and(|job| job.stage == "queued"))
            .cloned()
        else {
            return;
        };
        let Some(request) = state.requests.get(&job_id).cloned() else {
            return;
        };
        if let Some(job) = state.jobs.get_mut(&job_id) {
            job.stage = "preparing".to_string();
            job.progress_text = public_progress::text("preparing").to_string();
            job.progress_key = Some(public_progress::key("preparing"));
            job.phase = Some("preparing".to_string());
        }
        Some((job_id, request))
    };
    if let Some((job_id, request)) = next {
        std::thread::spawn(move || run_job(job_id, request));
    }
}

fn run_job(job_id: String, request: StartJobRequest) {
    if job_cancelled(&job_id) {
        finish(&job_id, Err("Cancelled".to_string()));
        return;
    }
    if runtime_command().is_none() {
        let stop = Arc::new(AtomicBool::new(false));
        if let Err(error) = crate::overlay::creation_runtime::download_runtime(stop, true) {
            finish(&job_id, Err(error.to_string()));
            return;
        }
    }
    if job_cancelled(&job_id) {
        finish(&job_id, Err("Cancelled".to_string()));
        return;
    }
    let Some(mut command) = runtime_command() else {
        finish(&job_id, Err("Creation engine is unavailable.".to_string()));
        return;
    };
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_command_window(&mut command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            finish(
                &job_id,
                Err(format!("Could not start creation engine: {error}")),
            );
            return;
        }
    };
    let claimed = STATE.lock().is_ok_and(|mut state| {
        if state
            .jobs
            .get(&job_id)
            .is_some_and(|job| job.stage == "cancelled")
        {
            false
        } else {
            state.pids.insert(job_id.clone(), child.id());
            true
        }
    });
    if !claimed {
        let _ = child.kill();
        let _ = child.wait();
        finish(&job_id, Err("Cancelled".to_string()));
        return;
    }
    let message = json!({
        "id": job_id,
        "cmd": "start_image_job",
        "args": {
            "imagePaths": &request.image_paths,
            "imagePath": &request.image_path,
            "outputDir": &request.output_dir,
            "outputName": &request.output_name,
            "prompt": &request.prompt,
        }
    });
    let write_result = child
        .stdin
        .take()
        .ok_or_else(|| "Creation engine input is unavailable".to_string())
        .and_then(|mut stdin| writeln!(stdin, "{message}").map_err(|error| error.to_string()));
    if let Err(error) = write_result {
        let _ = child.kill();
        finish(&job_id, Err(error));
        return;
    }
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        finish(
            &job_id,
            Err("Creation engine output is unavailable".to_string()),
        );
        return;
    };
    let mut final_result = None;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("event").and_then(Value::as_str) == Some("progress") {
            update_progress(&job_id, &value);
        } else if value.get("ok").and_then(Value::as_bool) == Some(true) {
            final_result = value.get("result").cloned().map(Ok);
        } else if value.get("ok").and_then(Value::as_bool) == Some(false) {
            final_result = Some(Err(value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Image creation failed")
                .to_string()));
        }
    }
    let status = child.wait();
    let result = final_result
        .unwrap_or_else(|| {
            Err(match status {
                Ok(value) => format!("Creation engine ended before returning an image ({value})"),
                Err(error) => format!("Creation engine ended unexpectedly: {error}"),
            })
        })
        .and_then(|value| validate_runtime_result(&request, value));
    finish(&job_id, result);
}

pub(super) fn cancel_job(job_id: Option<&str>) -> Vec<JobStatus> {
    let pids = if let Ok(mut state) = STATE.lock() {
        let targets: Vec<String> = match job_id {
            Some(id) => vec![id.to_string()],
            None => state
                .jobs
                .iter()
                .filter(|(_, job)| job.stage == "queued" || is_busy(&job.stage))
                .map(|(id, _)| id.clone())
                .collect(),
        };
        let mut pids = Vec::new();
        for id in targets {
            let mut remove_request = false;
            if let Some(job) = state.jobs.get_mut(&id)
                && (job.stage == "queued" || is_busy(&job.stage))
            {
                remove_request = job.stage == "queued";
                job.stage = "cancelled".to_string();
                job.progress_text = "Cancelled".to_string();
            }
            if remove_request {
                state.requests.remove(&id);
            }
            if let Some(pid) = state.pids.remove(&id) {
                pids.push(pid);
            }
        }
        pids
    } else {
        Vec::new()
    };
    for pid in pids {
        let mut command = Command::new("taskkill");
        command.args(["/PID", &pid.to_string(), "/T", "/F"]);
        hide_command_window(&mut command);
        let _ = command.stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    schedule_next();
    job_statuses()
}

fn is_busy(stage: &str) -> bool {
    matches!(
        stage,
        "preparing" | "uploading" | "generating" | "finalizing"
    )
}

#[cfg(windows)]
fn hide_command_window(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_command_window(_command: &mut Command) {}
