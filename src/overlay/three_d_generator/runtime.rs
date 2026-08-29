use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod capabilities;
mod companion;
mod control;
mod failure;
mod generation_mode;
mod message;
mod preparation;
mod process;
mod recovery;
mod refinement;
mod restore;
mod submission;
pub(super) use refinement::RefineRequest;
use refinement::RefinementKind;
pub(super) use submission::{start_job, start_refinement, start_segmentation};

pub(super) use control::{
    cancel_for_shutdown, cancel_job, forget_result_path, is_known_result_path, open_output,
    remap_result_path,
};
use generation_mode::GenerationMode;
use process::run_runtime_operation;
use restore::ensure_recovery_started;

const MAX_PARALLEL_JOBS: usize = 2;
const MAX_QUEUED_JOBS: usize = 50;
const MAX_RETAINED_TERMINAL_JOBS: usize = 64;
const CONTINUATION_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartJobRequest {
    pub image_path: String,
    #[serde(default)]
    pub source_descriptors: Vec<crate::overlay::creation_source::SourceDescriptor>,
    pub output_dir: Option<String>,
    #[serde(default)]
    pub final_output_dir: Option<String>,
    pub polycount: u32,
    pub mode: String,
    #[serde(default)]
    pub generation_mode: GenerationMode,
    pub output_format: String,
    pub auto_segment: bool,
    pub segmentation_mode: String,
    #[serde(default)]
    pub instruction: Option<String>,
    #[serde(default)]
    pub output_name: String,
    #[serde(skip, default)]
    pub dispatch_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JobStatus {
    pub job_id: Option<String>,
    pub stage: String,
    pub progress_text: String,
    pub phase: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub estimated_total_ms: Option<u64>,
    pub progress_ratio: Option<f64>,
    pub timing_sample_count: Option<u64>,
    pub output_path: Option<String>,
    pub output_name: Option<String>,
    pub download_path: Option<String>,
    pub download_name: Option<String>,
    pub source_image_path: Option<String>,
    pub output_dir: Option<String>,
    pub generation_mode: Option<GenerationMode>,
    pub polycount: Option<u32>,
    pub auto_segment: Option<bool>,
    pub instruction: Option<String>,
    pub project_id: Option<String>,
    pub parent_revision_id: Option<String>,
    pub revision_kind: Option<String>,
    pub supported_actions: Vec<String>,
    pub available_actions: Vec<String>,
    pub is_textured: bool,
    pub is_pbr: bool,
    pub is_rigged: bool,
    pub rig_type: Option<String>,
    pub can_refine: bool,
    pub is_segmented: bool,
    pub can_segment: bool,
    pub error: Option<String>,
    pub runtime_status: String,
}

#[derive(Debug, Clone)]
struct Continuation {
    parent_dispatch_id: String,
    dispatch_id: String,
    image_path: String,
    source_descriptor: crate::overlay::creation_source::SourceDescriptor,
    output_dir: PathBuf,
    staging_dir: PathBuf,
    output_name: String,
    previous_output_path: PathBuf,
    generation_mode: GenerationMode,
    polycount: u32,
    auto_segment: bool,
    instruction: Option<String>,
    project_id: String,
    supported_actions: Vec<String>,
    available_actions: Vec<String>,
    is_segmented: bool,
    is_textured: bool,
    is_pbr: bool,
    is_rigged: bool,
    rig_type: Option<String>,
    refinement: Option<RefineRequest>,
    expires_at_ms: u64,
}

#[derive(Default)]
struct RuntimeState {
    jobs: HashMap<String, JobStatus>,
    job_order: Vec<String>,
    pids: HashMap<String, u32>,
    continuations: HashMap<String, Continuation>,
    operations: HashMap<String, RuntimeOperation>,
    request_fingerprints: HashMap<String, String>,
    deadlines: HashMap<String, u64>,
    recovered_jobs: std::collections::HashSet<String>,
    pending_completions: HashMap<String, (JobStatus, Option<Continuation>)>,
}

impl RuntimeState {
    fn running_count(&self) -> usize {
        self.jobs
            .values()
            .filter(|status| status_is_busy(&status.stage))
            .count()
    }

    fn pending_count(&self) -> usize {
        self.jobs
            .values()
            .filter(|status| status.stage == "queued" || status_is_busy(&status.stage))
            .count()
    }

    fn insert_job(&mut self, job_id: String, status: JobStatus) {
        if !self.jobs.contains_key(&job_id) {
            self.job_order.push(job_id.clone());
        }
        self.jobs.insert(job_id, status);
        self.prune_terminal_jobs();
    }

    fn prune_terminal_jobs(&mut self) {
        let terminal_ids = self
            .job_order
            .iter()
            .filter(|job_id| {
                self.jobs.get(*job_id).is_some_and(|status| {
                    matches!(status.stage.as_str(), "done" | "failed" | "cancelled")
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        let remove_count = terminal_ids
            .len()
            .saturating_sub(MAX_RETAINED_TERMINAL_JOBS);
        for job_id in terminal_ids.into_iter().take(remove_count) {
            self.jobs.remove(&job_id);
            if let Some(continuation) = self.continuations.remove(&job_id) {
                let _ = crate::overlay::creation_source_snapshot::release_continuation(
                    std::slice::from_ref(&continuation.source_descriptor),
                    &job_id,
                );
            }
            self.pids.remove(&job_id);
            self.operations.remove(&job_id);
            self.request_fingerprints.remove(&job_id);
            self.deadlines.remove(&job_id);
            self.recovered_jobs.remove(&job_id);
            self.pending_completions.remove(&job_id);
        }
        self.job_order
            .retain(|job_id| self.jobs.contains_key(job_id));
    }

    fn latest_status(&self) -> Option<JobStatus> {
        self.job_order
            .iter()
            .rev()
            .find_map(|job_id| self.jobs.get(job_id).cloned())
    }

    fn prune_expired_continuations(&mut self, now_ms: u64) {
        let expired = self
            .continuations
            .iter()
            .filter(|(_, continuation)| continuation.expires_at_ms <= now_ms)
            .map(|(job_id, continuation)| (job_id.clone(), continuation.source_descriptor.clone()))
            .collect::<Vec<_>>();
        for (job_id, descriptor) in expired {
            self.continuations.remove(&job_id);
            let _ = crate::overlay::creation_source_snapshot::release_continuation(
                std::slice::from_ref(&descriptor),
                &job_id,
            );
            if let Some(status) = self.jobs.get_mut(&job_id) {
                status.can_segment = false;
                status.can_refine = false;
                status.available_actions.clear();
            }
        }
    }

    fn take_continuation(&mut self, job_id: &str, now_ms: u64) -> Result<Continuation, String> {
        self.prune_expired_continuations(now_ms);
        let continuation = self
            .continuations
            .remove(job_id)
            .ok_or_else(|| "This model can no longer be separated into parts.".to_string())?;
        if let Some(status) = self.jobs.get_mut(job_id) {
            status.can_segment = false;
            status.can_refine = false;
            status.available_actions.clear();
        }
        Ok(continuation)
    }

    fn peek_continuation(&mut self, job_id: &str, now_ms: u64) -> Result<Continuation, String> {
        self.prune_expired_continuations(now_ms);
        self.continuations
            .get(job_id)
            .cloned()
            .ok_or_else(|| "This model can no longer be separated into parts.".to_string())
    }
}

fn status_is_busy(stage: &str) -> bool {
    matches!(
        stage,
        "preparing" | "generating" | "segmenting" | "refining" | "finalizing"
    )
}

fn status_is_non_publishable(stage: &str) -> bool {
    matches!(stage, "cancelling" | "cancelled")
}

#[derive(Clone)]
enum RuntimeOperation {
    Generate {
        request: StartJobRequest,
        output_dir: PathBuf,
        final_output_dir: PathBuf,
    },
    Segment {
        continuation: Continuation,
    },
    Refine {
        continuation: Continuation,
    },
}

impl RuntimeOperation {
    fn source_image_path(&self) -> &str {
        match self {
            Self::Generate { request, .. } => &request.image_path,
            Self::Segment { continuation } | Self::Refine { continuation } => {
                &continuation.image_path
            }
        }
    }

    fn generation_mode(&self) -> GenerationMode {
        match self {
            Self::Generate { request, .. } => request.generation_mode,
            Self::Segment { continuation } | Self::Refine { continuation } => {
                continuation.generation_mode
            }
        }
    }

    fn polycount(&self) -> u32 {
        match self {
            Self::Generate { request, .. } => request.polycount,
            Self::Segment { continuation } | Self::Refine { continuation } => {
                continuation.polycount
            }
        }
    }

    fn auto_segment(&self) -> bool {
        match self {
            Self::Generate { request, .. } => request.auto_segment,
            Self::Segment { continuation } | Self::Refine { continuation } => {
                continuation.auto_segment
            }
        }
    }

    fn instruction(&self) -> Option<&str> {
        match self {
            Self::Generate { request, .. } => request.instruction.as_deref(),
            Self::Segment { continuation } | Self::Refine { continuation } => {
                continuation.instruction.as_deref()
            }
        }
    }

    fn output_dir(&self) -> &std::path::Path {
        match self {
            Self::Generate { output_dir, .. } => output_dir,
            Self::Segment { continuation } | Self::Refine { continuation } => {
                &continuation.staging_dir
            }
        }
    }

    fn final_output_dir(&self) -> &std::path::Path {
        match self {
            Self::Generate {
                final_output_dir, ..
            } => final_output_dir,
            Self::Segment { continuation } | Self::Refine { continuation } => {
                &continuation.output_dir
            }
        }
    }

    fn dispatch_id(&self) -> &str {
        match self {
            Self::Generate { request, .. } => &request.dispatch_id,
            Self::Segment { continuation } | Self::Refine { continuation } => {
                &continuation.dispatch_id
            }
        }
    }

    fn output_name(&self) -> &str {
        match self {
            Self::Generate { request, .. } => &request.output_name,
            Self::Segment { continuation } | Self::Refine { continuation } => {
                &continuation.output_name
            }
        }
    }
}

static STATE: LazyLock<Mutex<RuntimeState>> = LazyLock::new(|| Mutex::new(RuntimeState::default()));

pub(super) fn prepare_runtime() -> String {
    preparation::prepare_runtime()
}

pub(super) fn runtime_preparation_status() -> String {
    preparation::runtime_preparation_status()
}

pub(super) fn product_capabilities() -> Value {
    capabilities::product_capabilities()
}

fn runtime_command() -> Option<Command> {
    crate::overlay::creation_runtime::shared_runtime_path().map(Command::new)
}

fn runtime_status_label() -> String {
    if crate::overlay::creation_runtime::shared_runtime_path().is_some() {
        "installed".to_string()
    } else {
        "missing".to_string()
    }
}

pub(super) fn default_output_dir() -> Result<PathBuf, String> {
    let directory = crate::paths::app_local_data_dir().join("3d-generator");
    std::fs::create_dir_all(&directory)
        .map_err(|_| "The 3D project library is unavailable.".to_string())?;
    std::fs::canonicalize(directory)
        .map_err(|_| "The 3D project library is unavailable.".to_string())
}

fn next_job_id() -> Result<String, String> {
    crate::overlay::creation_identity::random_id("mesh_")
}

fn next_dispatch_id() -> Result<String, String> {
    crate::overlay::creation_identity::random_id("dispatch-")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn continuation_expiry(reported: Option<u64>, now: u64) -> Option<u64> {
    let latest = now.saturating_add(CONTINUATION_WINDOW_MS);
    match reported {
        Some(expires) if expires <= now => None,
        Some(expires) => Some(expires.min(latest)),
        None => Some(latest),
    }
}

fn idle_status() -> JobStatus {
    JobStatus {
        job_id: None,
        stage: "idle".to_string(),
        progress_text: "Ready to create.".to_string(),
        phase: None,
        elapsed_ms: None,
        estimated_total_ms: None,
        progress_ratio: None,
        timing_sample_count: None,
        output_path: None,
        output_name: None,
        download_path: None,
        download_name: None,
        source_image_path: None,
        output_dir: None,
        generation_mode: None,
        polycount: None,
        auto_segment: None,
        instruction: None,
        project_id: None,
        parent_revision_id: None,
        revision_kind: None,
        supported_actions: Vec::new(),
        available_actions: Vec::new(),
        is_textured: false,
        is_pbr: false,
        is_rigged: false,
        rig_type: None,
        can_refine: false,
        is_segmented: false,
        can_segment: false,
        error: None,
        runtime_status: runtime_status_label(),
    }
}

pub(super) fn job_status(job_id: Option<&str>) -> JobStatus {
    ensure_recovery_started();
    STATE
        .lock()
        .ok()
        .and_then(|mut state| {
            state.prune_expired_continuations(now_ms());
            match job_id {
                Some(job_id) => state.jobs.get(job_id).cloned(),
                None => state.latest_status(),
            }
        })
        .unwrap_or_else(idle_status)
}

pub(super) fn job_statuses() -> Vec<JobStatus> {
    ensure_recovery_started();
    crate::overlay::creation_delivery::schedule_reconciliation("3d");
    settle_reconciled_completions();
    STATE
        .lock()
        .map(|mut state| {
            state.prune_expired_continuations(now_ms());
            state
                .job_order
                .iter()
                .filter_map(|job_id| state.jobs.get(job_id).cloned())
                .collect()
        })
        .unwrap_or_default()
}

fn settle_reconciled_completions() {
    let Ok(active) = crate::overlay::creation_intent_journal::load("3d") else {
        return;
    };
    let active = active
        .into_iter()
        .map(|intent| intent.job_id)
        .collect::<std::collections::HashSet<_>>();
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    let completed = state
        .pending_completions
        .keys()
        .filter(|job_id| {
            !active.contains(*job_id)
                && state
                    .jobs
                    .get(*job_id)
                    .is_some_and(|status| !status_is_non_publishable(&status.stage))
        })
        .cloned()
        .collect::<Vec<_>>();
    for job_id in completed {
        if let Some((status, continuation)) = state.pending_completions.remove(&job_id) {
            state.jobs.insert(job_id.clone(), status);
            if let Some(continuation) = continuation {
                state.continuations.insert(job_id.clone(), continuation);
            }
            state.operations.remove(&job_id);
            state.request_fingerprints.remove(&job_id);
            state.deadlines.remove(&job_id);
            state.recovered_jobs.remove(&job_id);
        }
    }
    state.prune_terminal_jobs();
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
                .job_order
                .iter()
                .find(|id| state.jobs.get(*id).is_some_and(|job| job.stage == "queued"))
                .cloned()
            else {
                return;
            };
            let Some(operation) = state.operations.get(&job_id).cloned() else {
                return;
            };
            let Some(request_fingerprint) = state.request_fingerprints.get(&job_id).cloned() else {
                return;
            };
            let Some(deadline_at_ms) = state.deadlines.get(&job_id).copied() else {
                return;
            };
            let recovered = state.recovered_jobs.remove(&job_id);
            if let Some(status) = state.jobs.get_mut(&job_id) {
                let (stage, text, phase) = match operation {
                    RuntimeOperation::Segment { .. } => {
                        ("segmenting", "Separating model parts.", "separation")
                    }
                    RuntimeOperation::Refine { .. } => {
                        ("refining", "Creating a new version.", "refinement")
                    }
                    RuntimeOperation::Generate { .. } => {
                        ("preparing", "Preparing creation.", "preparing")
                    }
                };
                status.stage = stage.to_string();
                status.progress_text = text.to_string();
                status.phase = Some(phase.to_string());
            }
            (
                job_id,
                operation,
                request_fingerprint,
                deadline_at_ms,
                recovered,
            )
        };
        std::thread::spawn(move || run_runtime_operation(next.0, next.1, next.2, next.3, next.4));
    }
}

#[cfg(test)]
mod tests;
