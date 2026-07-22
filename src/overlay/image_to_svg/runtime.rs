use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write as _};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod asset_io;
pub(super) use asset_io::{open_output, read_asset, save_svg_edits};

#[cfg(debug_assertions)]
const RUNTIME_EXE_NAME: &str = "sgt_creation_runtime.exe";
const MAX_PARALLEL_JOBS: usize = 2;
const READY_TARGET: usize = 2;
const READY_TTL_MS: u128 = 48 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartJobRequest {
    pub image_path: String,
    pub output_dir: Option<String>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JobStatus {
    pub job_id: String,
    pub stage: String,
    pub progress_text: String,
    pub progress_key: Option<String>,
    pub phase: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub estimated_total_ms: Option<u64>,
    pub progress_ratio: Option<f64>,
    pub output_path: Option<String>,
    pub output_name: Option<String>,
    pub preview_path: Option<String>,
    pub source_image_path: String,
    pub model: String,
    pub credits_remaining: Option<u64>,
    pub error: Option<String>,
}

#[derive(Default)]
struct RuntimeState {
    jobs: HashMap<String, JobStatus>,
    order: Vec<String>,
    pids: HashMap<String, u32>,
}

impl RuntimeState {
    fn running_count(&self) -> usize {
        self.jobs
            .values()
            .filter(|job| {
                matches!(
                    job.stage.as_str(),
                    "preparing" | "generating" | "finalizing"
                )
            })
            .count()
    }
}

static STATE: LazyLock<Mutex<RuntimeState>> = LazyLock::new(|| Mutex::new(RuntimeState::default()));
static JOB_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static WARM_RUNNING: AtomicBool = AtomicBool::new(false);

#[cfg(debug_assertions)]
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

#[cfg(not(debug_assertions))]
fn dev_runtime_exe_path() -> Option<PathBuf> {
    None
}

fn runtime_command() -> Option<Command> {
    #[cfg(debug_assertions)]
    if let Some(path) = dev_runtime_exe_path() {
        return Some(Command::new(path));
    }
    if crate::overlay::creation_runtime::is_runtime_installed() {
        Some(Command::new(
            crate::overlay::creation_runtime::runtime_exe_path(),
        ))
    } else {
        dev_runtime_exe_path().map(Command::new)
    }
}

pub(super) fn default_output_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| crate::paths::app_local_data_dir().join("vectors"))
}

fn next_job_id() -> String {
    format!(
        "svg_{}_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        JOB_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

pub(super) fn start_job(mut request: StartJobRequest) -> Result<JobStatus, String> {
    if request.image_path.trim().is_empty() {
        return Err("Pick an image first.".to_string());
    }
    if !PathBuf::from(&request.image_path).is_file() {
        return Err(format!("Image does not exist: {}", request.image_path));
    }
    request.model = match request.model.as_str() {
        "detail" => "detail".to_string(),
        _ => "simple".to_string(),
    };
    let output_dir = request
        .output_dir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_output_dir);
    std::fs::create_dir_all(&output_dir)
        .map_err(|error| format!("Could not create {}: {error}", output_dir.display()))?;

    let mut state = STATE
        .lock()
        .map_err(|_| "Vector job state is unavailable".to_string())?;
    if state.running_count() >= MAX_PARALLEL_JOBS {
        return Err("Both vector workers are busy.".to_string());
    }
    let job_id = next_job_id();
    let status = JobStatus {
        job_id: job_id.clone(),
        stage: "preparing".to_string(),
        progress_text: "Preparing vector workspace".to_string(),
        progress_key: Some("svg.preparingWorkspace".to_string()),
        phase: Some("preparing".to_string()),
        elapsed_ms: Some(0),
        estimated_total_ms: None,
        progress_ratio: Some(0.0),
        output_path: None,
        output_name: None,
        preview_path: None,
        source_image_path: request.image_path.clone(),
        model: request.model.clone(),
        credits_remaining: None,
        error: None,
    };
    state.order.push(job_id.clone());
    state.jobs.insert(job_id.clone(), status.clone());
    drop(state);

    let preview_job_id = job_id.clone();
    let preview_source = request.image_path.clone();
    std::thread::spawn(move || {
        let stop = Arc::new(AtomicBool::new(false));
        let result =
            crate::overlay::three_d_generator::download_depth_model(stop, true).and_then(|()| {
                crate::overlay::three_d_generator::create_depth_preview(&preview_source)
            });
        match result {
            Ok(path) => update_preview(&preview_job_id, path.to_string_lossy().to_string()),
            Err(error) => crate::log_info!(
                "[Image to SVG] Optional depth preview failed for {}: {error}",
                preview_source
            ),
        }
    });
    std::thread::spawn(move || run_job(job_id, request, output_dir));
    Ok(status)
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
        job.stage = stage.to_string();
    }
    if let Some(text) = value.get("progressText").and_then(Value::as_str) {
        job.progress_text = text.to_string();
    }
    if let Some(key) = value.get("progressKey").and_then(Value::as_str) {
        job.progress_key = Some(key.to_string());
    }
    if let Some(phase) = value.get("phase").and_then(Value::as_str) {
        job.phase = Some(phase.to_string());
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

fn update_preview(job_id: &str, preview_path: String) {
    if let Ok(mut state) = STATE.lock()
        && let Some(job) = state.jobs.get_mut(job_id)
        && job.stage != "cancelled"
    {
        job.preview_path = Some(preview_path);
    }
}

fn finish(job_id: &str, result: Result<Value, String>) {
    let mut completed = None;
    if let Ok(mut state) = STATE.lock() {
        state.pids.remove(job_id);
        if let Some(job) = state.jobs.get_mut(job_id)
            && job.stage != "cancelled"
        {
            match result {
                Ok(value) => {
                    job.stage = "done".to_string();
                    job.progress_text = "Vector ready".to_string();
                    job.progress_key = Some("svg.vectorReady".to_string());
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
                    job.credits_remaining = value.get("creditsRemaining").and_then(Value::as_u64);
                    completed = Some(job.clone());
                }
                Err(error) => {
                    job.stage = "failed".to_string();
                    job.progress_text = "Could not create vector".to_string();
                    job.progress_key = Some("svg.failed".to_string());
                    job.phase = Some("failed".to_string());
                    job.error = Some(error);
                }
            }
        }
    }
    if let Some(job) = completed
        && let Some(output_path) = job.output_path.as_deref()
        && let Err(error) = crate::overlay::generation_history::record(
            "svg",
            &job.source_image_path,
            output_path,
            json!({ "model": job.model }),
        )
    {
        crate::log_info!("[Image to SVG] Could not record result history: {error}");
    }
    start_preparation();
}

fn run_job(job_id: String, request: StartJobRequest, output_dir: PathBuf) {
    if runtime_command().is_none() {
        let stop = Arc::new(AtomicBool::new(false));
        if let Err(error) = crate::overlay::creation_runtime::download_runtime(stop, true) {
            finish(&job_id, Err(error.to_string()));
            return;
        }
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
    if let Ok(mut state) = STATE.lock() {
        state.pids.insert(job_id.clone(), child.id());
    }
    let message = json!({
        "id": job_id,
        "cmd": "start_svg_job",
        "args": {
            "imagePath": request.image_path,
            "outputDir": output_dir,
            "model": request.model,
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
                .unwrap_or("Vector creation failed")
                .to_string()));
        }
    }
    let status = child.wait();
    let result = final_result.unwrap_or_else(|| {
        Err(match status {
            Ok(value) => format!("Creation engine ended before returning a vector ({value})"),
            Err(error) => format!("Creation engine ended unexpectedly: {error}"),
        })
    });
    finish(&job_id, result);
}

pub(super) fn cancel_job(job_id: Option<&str>) -> Vec<JobStatus> {
    let pids = if let Ok(mut state) = STATE.lock() {
        let targets: Vec<String> = match job_id {
            Some(id) => vec![id.to_string()],
            None => state
                .jobs
                .iter()
                .filter(|(_, job)| {
                    matches!(
                        job.stage.as_str(),
                        "preparing" | "generating" | "finalizing"
                    )
                })
                .map(|(id, _)| id.clone())
                .collect(),
        };
        let mut pids = Vec::new();
        for id in targets {
            if let Some(job) = state.jobs.get_mut(&id) {
                job.stage = "cancelled".to_string();
                job.progress_text = "Cancelled".to_string();
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
    job_statuses()
}

fn accounts_dir() -> PathBuf {
    crate::paths::app_local_data_dir()
        .join("3d-generator-runtime")
        .join("svg-accounts")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadyAccount {
    profile_dir: PathBuf,
    credits: u32,
    created_at: u128,
}

fn ready_slot(slot: usize) -> bool {
    let marker = accounts_dir().join(format!("ready-{slot}.json"));
    let account = std::fs::read_to_string(&marker)
        .ok()
        .and_then(|text| serde_json::from_str::<ReadyAccount>(&text).ok());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let valid = account.as_ref().is_some_and(|value| {
        value.credits >= 4
            && value.profile_dir.is_dir()
            && now.saturating_sub(value.created_at) <= READY_TTL_MS
    });
    if !valid {
        let _ = std::fs::remove_file(marker);
    }
    valid
}

fn ready_count() -> usize {
    (0..READY_TARGET).filter(|slot| ready_slot(*slot)).count()
}

pub(super) fn runtime_preparation_status() -> String {
    let ready = ready_count();
    if ready >= READY_TARGET {
        "ready".to_string()
    } else if ready > 0 {
        "partial".to_string()
    } else if WARM_RUNNING.load(Ordering::SeqCst) {
        "preparing".to_string()
    } else if runtime_command().is_some() {
        "idle".to_string()
    } else {
        "missing".to_string()
    }
}

fn start_preparation() {
    if WARM_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    std::thread::spawn(|| {
        for attempt in 0..3 {
            if runtime_command().is_none() {
                let stop = Arc::new(AtomicBool::new(false));
                let _ = crate::overlay::creation_runtime::download_runtime(stop, true);
            }
            let missing = READY_TARGET.saturating_sub(ready_count());
            if missing == 0 {
                break;
            }
            let mut workers = Vec::with_capacity(missing);
            for _ in 0..missing {
                workers.push(std::thread::spawn(|| {
                    let Some(mut command) = runtime_command() else {
                        return;
                    };
                    command
                        .arg("--warm-svg-headless")
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null());
                    hide_command_window(&mut command);
                    let _ = command.status();
                }));
            }
            for worker in workers {
                let _ = worker.join();
            }
            if ready_count() >= READY_TARGET || attempt == 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(5 * 60));
        }
        WARM_RUNNING.store(false, Ordering::SeqCst);
    });
}

pub(super) fn prepare_runtime() -> String {
    start_preparation();
    runtime_preparation_status()
}

#[cfg(windows)]
fn hide_command_window(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_command_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_two_parallel_jobs() {
        assert_eq!(MAX_PARALLEL_JOBS, 2);
        assert_eq!(READY_TARGET, 2);
    }
}
