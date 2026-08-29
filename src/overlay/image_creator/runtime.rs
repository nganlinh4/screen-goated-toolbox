use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod asset_io;
mod completion;
mod dispatch;
mod output_guard;
mod preparation;
mod process;
mod public_progress;
mod restore;
#[cfg(test)]
mod tests;
pub(super) use asset_io::open_output;
use completion::{
    cleanup_request_staging, finish, finish_retaining_intent, is_non_publishable, job_cancelled,
    job_status, settle_reconciled_completions, update_progress,
};
use output_guard::validate_runtime_result;
pub(super) use preparation::{prepare_runtime, runtime_preparation_status};
use process::run_job;
use restore::ensure_recovery_started;

const MAX_PARALLEL_JOBS: usize = 2;
const MAX_QUEUED_JOBS: usize = 50;
const MAX_RETAINED_TERMINAL_JOBS: usize = 64;
const OPERATION: &str = "create_image";
const MAX_PROMPT_CHARACTERS: usize = 4_000;
const MAX_REFERENCE_IMAGES: usize = 20;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartJobRequest {
    #[serde(default)]
    pub image_paths: Vec<String>,
    #[serde(default)]
    pub image_path: Option<String>,
    #[serde(default)]
    pub source_descriptors: Vec<crate::overlay::creation_source::SourceDescriptor>,
    pub output_dir: Option<String>,
    #[serde(default)]
    final_output_dir: Option<String>,
    pub prompt: String,
    #[serde(default)]
    output_name: Option<String>,
    #[serde(skip)]
    dispatch_id: String,
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
    pub output_dir: String,
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
    request_fingerprints: HashMap<String, String>,
    deadlines: HashMap<String, u64>,
    recovered_jobs: std::collections::HashSet<String>,
    pending_completions: HashMap<String, JobStatus>,
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

    fn prune_terminal_jobs(&mut self) {
        let terminal_ids = self
            .order
            .iter()
            .filter(|id| {
                self.jobs
                    .get(*id)
                    .is_some_and(|job| job.stage != "queued" && !is_busy(&job.stage))
            })
            .cloned()
            .collect::<Vec<_>>();
        let remove_count = terminal_ids
            .len()
            .saturating_sub(MAX_RETAINED_TERMINAL_JOBS);
        for id in terminal_ids.into_iter().take(remove_count) {
            self.jobs.remove(&id);
            self.requests.remove(&id);
            self.request_fingerprints.remove(&id);
            self.deadlines.remove(&id);
            self.recovered_jobs.remove(&id);
            self.pending_completions.remove(&id);
            self.pids.remove(&id);
        }
        self.order.retain(|id| self.jobs.contains_key(id));
    }
}

static STATE: LazyLock<Mutex<RuntimeState>> = LazyLock::new(|| Mutex::new(RuntimeState::default()));
fn runtime_command() -> Option<Command> {
    crate::overlay::creation_runtime::shared_runtime_path().map(Command::new)
}

pub(super) fn default_output_dir() -> PathBuf {
    crate::paths::app_local_data_dir().join("images")
}

pub(super) fn start_job(mut request: StartJobRequest) -> Result<JobStatus, String> {
    if !crate::creation_feature_availability::image_creator_release_enabled() {
        return Err("Image creation is temporarily unavailable.".to_string());
    }
    crate::overlay::creation_close::ensure_accepting("image")?;
    ensure_recovery_started();
    let inspected_sources = normalize_reference_paths(&mut request)?;
    let source_bytes = inspected_sources
        .iter()
        .map(|source| source.size_bytes)
        .sum();
    request.prompt = request.prompt.trim().to_string();
    if request.prompt.is_empty() {
        return Err("Describe the image you want to create.".to_string());
    }
    if request.prompt.chars().count() > MAX_PROMPT_CHARACTERS {
        return Err("Image instructions are too long.".to_string());
    }
    let final_output_dir = request
        .output_dir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_output_dir);
    std::fs::create_dir_all(&final_output_dir)
        .map_err(|error| format!("Could not create {}: {error}", final_output_dir.display()))?;
    let final_output_dir = std::fs::canonicalize(&final_output_dir)
        .map_err(|error| format!("Could not use {}: {error}", final_output_dir.display()))?;
    crate::overlay::creation_intent_journal::validate_persisted_path(&final_output_dir)?;
    request.dispatch_id = dispatch::next_dispatch_id()?;

    let job_id = dispatch::next_job_id()?;
    let filename_suffix = job_id.strip_prefix("image_").unwrap_or(job_id.as_str());
    request.output_name = Some(format!("Created Image {filename_suffix}.png"));
    crate::overlay::creation_output::require_unoccupied(
        &final_output_dir,
        request
            .output_name
            .as_deref()
            .ok_or_else(|| "Image output assignment is missing.".to_string())?,
    )?;
    let staging = crate::overlay::creation_output::prepare_staging(
        &request.dispatch_id,
        request
            .output_name
            .as_deref()
            .ok_or_else(|| "Image output assignment is missing.".to_string())?,
    )?;
    request.output_dir = Some(staging.directory().to_string_lossy().to_string());
    request.final_output_dir = Some(final_output_dir.to_string_lossy().to_string());
    let has_references = !request.image_paths.is_empty();
    let status = JobStatus {
        job_id: job_id.clone(),
        operation: OPERATION.to_string(),
        stage: "queued".to_string(),
        progress_text: public_progress::text("queued", has_references).to_string(),
        progress_key: Some(public_progress::key("queued")),
        phase: Some("queued".to_string()),
        elapsed_ms: Some(0),
        estimated_total_ms: Some(180_000),
        progress_ratio: Some(0.0),
        output_path: None,
        output_name: None,
        source_image_path: request.image_paths.first().cloned().unwrap_or_default(),
        source_image_paths: request.image_paths.clone(),
        output_dir: final_output_dir.to_string_lossy().to_string(),
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
        crate::overlay::creation_close::ensure_accepting("image")?;
        if state.pending_count() >= MAX_QUEUED_JOBS {
            return Err("The image queue is full.".to_string());
        }
        let recorded = crate::overlay::generation_history::admit_and_record(
            "image",
            &final_output_dir,
            source_bytes,
            inspected_sources.len(),
            || {
                let snapshot = if inspected_sources.is_empty() {
                    None
                } else {
                    Some(crate::overlay::creation_source_snapshot::prepare(
                        "image",
                        &request.dispatch_id,
                        &inspected_sources,
                    )?)
                };
                if let Some(snapshot) = &snapshot {
                    request.image_paths = snapshot.paths();
                    request.image_path = request.image_paths.first().cloned();
                    request.source_descriptors = snapshot.descriptors().to_vec();
                }
                let frozen_arguments = serde_json::to_value(&request)
                    .map_err(|_| "Image request could not be saved.".to_string())?;
                let recorded = crate::overlay::creation_intent_journal::record(
                    "image",
                    &job_id,
                    &request.dispatch_id,
                    frozen_arguments,
                )?;
                if let Some(snapshot) = snapshot {
                    snapshot.persist();
                }
                Ok(recorded)
            },
        )?;
        let _staging_dir = staging.persist();
        state.order.push(job_id.clone());
        state.jobs.insert(job_id.clone(), status);
        state.requests.insert(job_id.clone(), request);
        state
            .request_fingerprints
            .insert(job_id.clone(), recorded.arguments_fingerprint);
        state
            .deadlines
            .insert(job_id.clone(), recorded.deadline_at_ms);
    }
    let active_demand = STATE.lock().map(|state| state.pending_count()).unwrap_or(1);
    crate::overlay::creation_runtime::maintain_readiness_for_demand("image", active_demand, false);
    schedule_next();
    job_status(&job_id).ok_or_else(|| "Image job could not be queued.".to_string())
}

fn normalize_reference_paths(
    request: &mut StartJobRequest,
) -> Result<Vec<crate::overlay::creation_source::InspectedImage>, String> {
    let mut references = std::mem::take(&mut request.image_paths);
    let legacy = request.image_path.take();
    if references.is_empty()
        && let Some(legacy) = legacy
    {
        references.push(legacy);
    }
    references
        .iter_mut()
        .for_each(|path| *path = path.trim().to_string());
    references.retain(|path| !path.is_empty());
    if references.len() > MAX_REFERENCE_IMAGES {
        return Err(format!(
            "An image session supports up to {MAX_REFERENCE_IMAGES} references."
        ));
    }
    let mut total_bytes = 0_u64;
    let mut validated = Vec::with_capacity(references.len());
    let mut inspected_sources = Vec::with_capacity(references.len());
    for reference in references {
        let inspected = asset_io::inspect_reference(&reference)?;
        let normalized = inspected.path.to_string_lossy().to_string();
        total_bytes = total_bytes
            .checked_add(inspected.size_bytes)
            .ok_or_else(|| "Reference image size is outside the supported range.".to_string())?;
        if total_bytes > asset_io::MAX_TOTAL_REFERENCE_BYTES {
            return Err("Reference images exceed 100 MiB in total.".to_string());
        }
        validated.push(normalized);
        inspected_sources.push(inspected);
    }
    request.image_path = validated.first().cloned();
    request.image_paths = validated;
    request.source_descriptors.clear();
    Ok(inspected_sources)
}

fn revalidate_request_sources(request: &StartJobRequest) -> Result<(), String> {
    if request.source_descriptors.len() != request.image_paths.len() {
        return Err("Image references changed after they were selected.".to_string());
    }
    for (path, descriptor) in request.image_paths.iter().zip(&request.source_descriptors) {
        if descriptor.path != *path {
            return Err("Image references changed after they were selected.".to_string());
        }
    }
    crate::overlay::creation_source_snapshot::validate_sources(&request.source_descriptors)
}

pub(super) fn job_statuses() -> Vec<JobStatus> {
    ensure_recovery_started();
    crate::overlay::creation_delivery::schedule_reconciliation("image");
    settle_reconciled_completions();
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

fn schedule_next() {
    loop {
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
            let Some(request_fingerprint) = state.request_fingerprints.get(&job_id).cloned() else {
                return;
            };
            let Some(deadline_at_ms) = state.deadlines.get(&job_id).copied() else {
                return;
            };
            let recovered = state.recovered_jobs.remove(&job_id);
            if let Some(job) = state.jobs.get_mut(&job_id) {
                job.stage = "preparing".to_string();
                job.progress_text = public_progress::text("preparing", false).to_string();
                job.progress_key = Some(public_progress::key("preparing"));
                job.phase = Some("preparing".to_string());
            }
            (
                job_id,
                request,
                request_fingerprint,
                deadline_at_ms,
                recovered,
            )
        };
        std::thread::spawn(move || run_job(next.0, next.1, next.2, next.3, next.4));
    }
}

pub(super) fn cancel_job(job_id: Option<&str>) -> Vec<JobStatus> {
    cancel_jobs(job_id, false).1
}

pub(super) fn cancel_for_shutdown() -> bool {
    cancel_jobs(None, true).0
}

fn cancel_jobs(job_id: Option<&str>, shutdown: bool) -> (bool, Vec<JobStatus>) {
    struct Candidate {
        job_id: String,
        previous_status: JobStatus,
        request: StartJobRequest,
        request_fingerprint: String,
        pid: Option<u32>,
    }
    let (candidates, mut durable) = {
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        let targets: Vec<String> = match job_id {
            Some(id) => vec![id.to_string()],
            None => state
                .jobs
                .iter()
                .filter(|(_, job)| {
                    job.stage == "queued"
                        || is_busy(&job.stage)
                        || shutdown && job.stage == "cancelling"
                })
                .map(|(id, _)| id.clone())
                .collect(),
        };
        let target_count = targets.len();
        let mut candidates = Vec::new();
        for id in targets {
            let Some(previous_status) = state.jobs.get(&id).cloned() else {
                continue;
            };
            if previous_status.stage != "queued"
                && !is_busy(&previous_status.stage)
                && !(shutdown && previous_status.stage == "cancelling")
            {
                continue;
            }
            let Some(request) = state.requests.get(&id).cloned() else {
                continue;
            };
            let Some(request_fingerprint) = state.request_fingerprints.get(&id).cloned() else {
                continue;
            };
            if previous_status.stage != "cancelling"
                && let Some(job) = state.jobs.get_mut(&id)
            {
                job.stage = "cancelling".to_string();
                job.progress_text = "Cancelling".to_string();
            }
            candidates.push(Candidate {
                job_id: id.clone(),
                previous_status,
                request,
                request_fingerprint,
                pid: state.pids.get(&id).copied(),
            });
        }
        let complete = candidates.len() == target_count;
        (candidates, complete)
    };
    let mut pids_to_kill = std::collections::HashSet::new();
    for candidate in candidates {
        let cancelled = candidate
            .request
            .output_name
            .as_deref()
            .is_some_and(|output_name| {
                crate::overlay::creation_delivery::cancel_dispatch(
                    crate::overlay::creation_delivery::CancelledDelivery {
                        product: "image",
                        job_id: candidate.job_id.clone(),
                        dispatch_id: candidate.request.dispatch_id.clone(),
                        request_fingerprint: candidate.request_fingerprint.clone(),
                        output_name: output_name.to_string(),
                    },
                )
                .is_ok()
            });
        durable &= cancelled;
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        if state
            .jobs
            .get(&candidate.job_id)
            .is_some_and(|job| job.stage == "cancelling")
        {
            if cancelled {
                if let Some(job) = state.jobs.get_mut(&candidate.job_id) {
                    job.stage = "cancelled".to_string();
                    job.progress_text = "Cancelled".to_string();
                }
                state.requests.remove(&candidate.job_id);
                state.request_fingerprints.remove(&candidate.job_id);
                state.deadlines.remove(&candidate.job_id);
                state.recovered_jobs.remove(&candidate.job_id);
                state.pending_completions.remove(&candidate.job_id);
                if let Some(pid) = state.pids.remove(&candidate.job_id).or(candidate.pid) {
                    pids_to_kill.insert(pid);
                }
                state.prune_terminal_jobs();
            } else if shutdown {
                if let Some(pid) = state.pids.remove(&candidate.job_id).or(candidate.pid) {
                    pids_to_kill.insert(pid);
                }
            } else {
                state
                    .jobs
                    .insert(candidate.job_id.clone(), candidate.previous_status);
            }
        }
        drop(state);
        if cancelled {
            cleanup_request_staging(&candidate.request);
        }
    }
    if shutdown {
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        pids_to_kill.extend(state.pids.drain().map(|(_, pid)| pid));
    } else {
        schedule_next();
    }
    for pid in pids_to_kill {
        crate::overlay::creation_recovery::terminate_process_tree(pid);
    }
    (durable, job_statuses())
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
