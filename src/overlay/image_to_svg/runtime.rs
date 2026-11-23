use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod asset_io;
mod asset_protocol;
mod completion;
mod process;
mod recovery;
mod svg_expansion;
mod svg_security;
pub(super) use asset_io::{open_output, read_asset, save_svg_edits};
pub(super) use asset_protocol::issue_static_asset;
use completion::{
    cleanup_request_staging, finish, finish_retaining_intent, job_cancelled,
    settle_reconciled_completions, update_progress,
};
use process::run_job;
use recovery::ensure_recovery_started;

const MAX_PARALLEL_JOBS: usize = 2;
// The picker admits 100 images per import, so the queue must admit a whole
// batch: a smaller cap silently failed everything past it. Terminal statuses
// are retained past the cap so a finished job cannot be pruned while its
// completion is still unread.
const MAX_QUEUED_JOBS: usize = 100;
const MAX_RETAINED_TERMINAL_JOBS: usize = 128;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartJobRequest {
    pub image_path: String,
    #[serde(default)]
    pub source_descriptors: Vec<crate::overlay::creation_source::SourceDescriptor>,
    pub output_dir: Option<String>,
    #[serde(default)]
    final_output_dir: Option<String>,
    pub model: String,
    #[serde(
        default = "default_background_mode",
        skip_serializing_if = "background_is_opaque"
    )]
    pub background_mode: String,
    #[serde(default)]
    output_name: String,
    #[serde(skip, default)]
    dispatch_id: String,
}

#[derive(Debug, Clone, Default, Serialize)]
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
    pub source_image_path: String,
    pub output_dir: String,
    pub model: String,
    pub background_mode: String,
    pub error: Option<String>,
}

#[derive(Default)]
struct RuntimeState {
    jobs: HashMap<String, JobStatus>,
    requests: HashMap<String, (StartJobRequest, PathBuf)>,
    request_fingerprints: HashMap<String, String>,
    deadlines: HashMap<String, u64>,
    recovered_jobs: std::collections::HashSet<String>,
    pending_completions: HashMap<String, JobStatus>,
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
                self.jobs.get(*id).is_some_and(|job| {
                    matches!(job.stage.as_str(), "done" | "failed" | "cancelled")
                })
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
static ADMISSION: crate::overlay::creation_lifecycle::AdmissionGate =
    crate::overlay::creation_lifecycle::AdmissionGate::new();

fn is_busy(stage: &str) -> bool {
    matches!(stage, "preparing" | "generating" | "finalizing")
}

fn runtime_command() -> Option<Command> {
    crate::overlay::creation_runtime::shared_runtime_path()
        .and_then(|path| crate::overlay::creation_runtime::configured_command(path).ok())
}

pub(super) fn default_output_dir() -> Result<PathBuf, String> {
    crate::paths::user_downloads_dir()
}

fn next_job_id() -> Result<String, String> {
    crate::overlay::creation_identity::random_id("svg_")
}

fn next_dispatch_id() -> Result<String, String> {
    crate::overlay::creation_identity::random_id("svg-dispatch-")
}

fn default_background_mode() -> String {
    "opaque".to_string()
}

fn background_is_opaque(value: &str) -> bool {
    value == "opaque"
}

fn normalize_background_mode(value: &str) -> String {
    match value {
        "auto" => "auto",
        "transparent" => "transparent",
        _ => "opaque",
    }
    .to_string()
}

pub(super) fn start_job(mut request: StartJobRequest) -> Result<JobStatus, String> {
    crate::overlay::creation_close::ensure_accepting("svg")?;
    ensure_recovery_started();
    let _admission = ADMISSION.acquire();
    if request.image_path.trim().is_empty() {
        return Err("Pick an image first.".to_string());
    }
    if let Some(error) = super::input_policy::error(std::path::Path::new(&request.image_path)) {
        return Err(error.to_string());
    }
    let inspected = crate::overlay::creation_source::inspect_image(&request.image_path)?;
    request.image_path = inspected.path.to_string_lossy().to_string();
    request.source_descriptors.clear();
    let source_bytes = inspected.size_bytes;
    request.model = match request.model.as_str() {
        "detail" => "detail".to_string(),
        _ => "simple".to_string(),
    };
    request.background_mode = normalize_background_mode(&request.background_mode);
    request.dispatch_id = next_dispatch_id()?;
    let final_output_dir = match request
        .output_dir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        Some(path) => PathBuf::from(path),
        None => default_output_dir()?,
    };
    std::fs::create_dir_all(&final_output_dir)
        .map_err(|error| format!("Could not create {}: {error}", final_output_dir.display()))?;
    let final_output_dir = std::fs::canonicalize(&final_output_dir)
        .map_err(|error| format!("Could not use {}: {error}", final_output_dir.display()))?;
    crate::overlay::creation_intent_journal::validate_persisted_path(&final_output_dir)?;
    request.output_name = crate::overlay::creation_output::assigned_name(
        &request.image_path,
        &request.dispatch_id,
        None,
        "svg",
    )?;
    crate::overlay::creation_output::require_unoccupied(&final_output_dir, &request.output_name)?;
    let lifecycle = crate::overlay::creation_lifecycle::acquire()?;
    let staging = crate::overlay::creation_output::prepare_staging(
        &request.dispatch_id,
        &request.output_name,
    )?;
    request.output_dir = Some(staging.directory().to_string_lossy().to_string());
    request.final_output_dir = Some(final_output_dir.to_string_lossy().to_string());

    let job_id = next_job_id()?;
    let status = JobStatus {
        job_id: job_id.clone(),
        stage: "queued".to_string(),
        progress_text: "Queued".to_string(),
        progress_key: Some("svg.queued".to_string()),
        phase: Some("queued".to_string()),
        elapsed_ms: Some(0),
        estimated_total_ms: None,
        progress_ratio: Some(0.0),
        output_path: None,
        output_name: None,
        source_image_path: request.image_path.clone(),
        output_dir: final_output_dir.to_string_lossy().to_string(),
        model: request.model.clone(),
        background_mode: request.background_mode.clone(),
        error: None,
    };
    crate::overlay::creation_lifecycle::register_admission(
        &STATE,
        |state| {
            crate::overlay::creation_close::ensure_accepting("svg")?;
            if state.pending_count() >= MAX_QUEUED_JOBS {
                return Err("The vector queue is full.".to_string());
            }
            Ok(state
                .jobs
                .iter()
                .filter(|(_, job)| job.stage == "queued" || is_busy(&job.stage))
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>())
        },
        |known| {
            if crate::overlay::creation_lifecycle::count_pending("svg", known)? >= MAX_QUEUED_JOBS {
                return Err("The vector queue is full.".to_string());
            }
            let mut publication_uncertain = false;
            let admission = crate::overlay::generation_history::admit_and_record(
                "svg",
                &final_output_dir,
                source_bytes,
                1,
                || {
                    let snapshot = crate::overlay::creation_source_snapshot::prepare(
                        "svg",
                        &request.dispatch_id,
                        &[inspected],
                    )?;
                    request.image_path = snapshot.paths()[0].clone();
                    request.source_descriptors = snapshot.descriptors().to_vec();
                    let frozen = serde_json::to_value(&request)
                        .map_err(|_| "Vector request could not be saved.".to_string())?;
                    let recorded = match crate::overlay::creation_intent_journal::record(
                        "svg",
                        &job_id,
                        &request.dispatch_id,
                        frozen,
                    ) {
                        Ok(recorded) => recorded,
                        Err(error) => {
                            if error.is_uncertain() {
                                publication_uncertain = true;
                                snapshot.persist();
                            }
                            return Err(error.to_string());
                        }
                    };
                    snapshot.persist();
                    Ok(recorded)
                },
            );
            let recorded = match admission {
                Ok(recorded) => recorded,
                Err(error) => {
                    if publication_uncertain {
                        staging.persist();
                    }
                    return Err(error);
                }
            };
            let staging_dir = staging.persist();

            Ok((request, recorded, staging_dir))
        },
        |state, (request, recorded, staging_dir)| {
            state.order.push(job_id.clone());
            state.jobs.insert(job_id.clone(), status.clone());
            state
                .requests
                .insert(job_id.clone(), (request, staging_dir));
            state
                .request_fingerprints
                .insert(job_id.clone(), recorded.arguments_fingerprint);
            state
                .deadlines
                .insert(job_id.clone(), recorded.deadline_at_ms);
        },
    )?;
    drop(lifecycle);

    #[cfg(test)]
    if std::env::var_os("SGT_TEST_INTENT_PUBLICATION_UNCERTAIN").is_some() {
        return Ok(status);
    }
    if !crate::overlay::creation_close::is_closing("svg") {
        crate::overlay::creation_runtime::maintain_readiness("svg", false);
        schedule_next();
    }
    Ok(status)
}

pub(super) fn job_statuses() -> Vec<JobStatus> {
    ensure_recovery_started();
    crate::overlay::creation_delivery::schedule_reconciliation("svg");
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

pub(super) fn recovery_pending() -> bool {
    ensure_recovery_started();
    recovery::recovery_pending()
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

pub(super) fn is_known_result_path(path: &std::path::Path) -> bool {
    let known_in_session = STATE.lock().is_ok_and(|state| {
        state.jobs.values().any(|status| {
            status
                .output_path
                .as_deref()
                .and_then(|value| std::fs::canonicalize(value).ok())
                .as_deref()
                == Some(path)
        })
    });
    known_in_session
        || crate::overlay::generation_history::list("svg").is_ok_and(|entries| {
            entries.iter().any(|entry| {
                std::fs::canonicalize(&entry.output_path).ok().as_deref() == Some(path)
            })
        })
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
        if crate::overlay::creation_close::is_closing("svg") {
            return;
        }
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
            let Some((request, output_dir)) = state.requests.get(&job_id).cloned() else {
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
                job.progress_text = "Getting ready".to_string();
                job.progress_key = Some("svg.preparing".to_string());
                job.phase = Some("preparing".to_string());
            }
            (
                job_id,
                request,
                output_dir,
                request_fingerprint,
                deadline_at_ms,
                recovered,
            )
        };
        std::thread::spawn(move || run_job(next.0, next.1, next.2, next.3, next.4, next.5));
    }
}

mod cancellation;
pub(super) use cancellation::{cancel_for_shutdown, cancel_job};

pub(super) fn runtime_preparation_status() -> String {
    crate::overlay::creation_runtime::readiness("svg")
}

fn start_preparation() {
    if !crate::overlay::creation_close::is_closing("svg") {
        crate::overlay::creation_runtime::maintain_readiness("svg", true);
    }
}

pub(super) fn prepare_runtime() -> String {
    start_preparation();
    "preparing".to_string()
}

#[cfg(windows)]
fn hide_command_window(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_command_window(_command: &mut Command) {}

#[cfg(test)]
mod tests;
