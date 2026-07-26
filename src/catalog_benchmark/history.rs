use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod latest;

use super::manifest::Manifest;
use super::reasoning::reasoning_policy_label;
use super::report::{Attempt, Recorder, read_attempts};
use super::setup::Suites;
use crate::model_config::ModelConfig;

const RUN_METADATA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct HistoryPolicy {
    version: u32,
    benchmark_protocol_version: u32,
    selection: String,
    vision_representative_max_edge_px: u32,
    minimum_representative_cases_per_vision_suite: usize,
    latency_statistic: String,
    accuracy_statistic: String,
    reliability_statistic: String,
}

impl HistoryPolicy {
    fn load() -> Result<Self> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/catalog-benchmark/history-policy.json");
        let policy: Self = serde_json::from_slice(
            &std::fs::read(&path).with_context(|| format!("read {}", path.display()))?,
        )
        .with_context(|| format!("parse {}", path.display()))?;
        ensure!(policy.version == 3, "unsupported history policy version");
        ensure!(
            policy.benchmark_protocol_version > 0,
            "benchmark protocol version cannot be zero"
        );
        ensure!(
            policy.selection == "latest_complete_run_per_model_suite",
            "unsupported benchmark-history selection policy"
        );
        ensure!(
            policy.vision_representative_max_edge_px > 0,
            "representative vision image edge cannot be zero"
        );
        ensure!(
            policy.minimum_representative_cases_per_vision_suite > 0,
            "representative vision suite cannot be empty"
        );
        ensure!(
            policy.latency_statistic == "latest_run_median",
            "unsupported catalog latency statistic"
        );
        ensure!(
            policy.accuracy_statistic == "latest_run_successful_attempt_scores",
            "unsupported catalog accuracy statistic"
        );
        ensure!(
            policy.reliability_statistic == "latest_run_success_rate_including_errors",
            "unsupported catalog reliability statistic"
        );
        Ok(policy)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct VisionLatencyPolicy {
    pub max_edge_px: u32,
    pub minimum_cases_per_suite: usize,
}

pub(super) fn vision_latency_policy() -> Result<VisionLatencyPolicy> {
    let policy = HistoryPolicy::load()?;
    Ok(VisionLatencyPolicy {
        max_edge_px: policy.vision_representative_max_edge_px,
        minimum_cases_per_suite: policy.minimum_representative_cases_per_vision_suite,
    })
}

pub(super) fn catalog_latency_eligible(attempt: &Attempt, max_edge_px: u32) -> bool {
    match attempt.suite.as_str() {
        "text" => true,
        "coordinate" | "ocr" => {
            let width = attempt
                .details
                .get("input_image_width")
                .and_then(serde_json::Value::as_u64);
            let height = attempt
                .details
                .get("input_image_height")
                .and_then(serde_json::Value::as_u64);
            matches!((width, height), (Some(width), Some(height))
                if width.max(height) <= u64::from(max_edge_px))
        }
        _ => false,
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum RunKind {
    Live,
    ImportedLive,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ModelIdentity {
    id: String,
    provider: String,
    api_model: String,
    reasoning_policy: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RunMetadata {
    version: u32,
    benchmark_protocol_version: u32,
    kind: RunKind,
    run_id: String,
    started_at: String,
    completed_at: String,
    manifest_version: u32,
    rounds: u8,
    fixture_fingerprint: String,
    catalog_fingerprint: String,
    suites: Vec<String>,
    models: Vec<ModelIdentity>,
}

pub(super) struct PendingRun {
    history_root: PathBuf,
    metadata: RunMetadata,
}

pub(super) fn live_recorder(
    manifest: &Manifest,
    suites: Suites,
    text_models: &[ModelConfig],
    vision_models: &[ModelConfig],
) -> Result<Recorder> {
    let policy = HistoryPolicy::load()?;
    let output = super::setup::output_dir();
    ensure!(
        !output.join("run.json").exists(),
        "benchmark output {} is already registered; choose a fresh output directory",
        output.display()
    );
    if !super::setup::resume_inputs().is_empty() {
        println!(
            "BENCH_HISTORY_SKIP output={} reason=recovery_run",
            output.display()
        );
        return Recorder::new(&output);
    }
    let now = chrono::Utc::now();
    let metadata = RunMetadata {
        version: RUN_METADATA_VERSION,
        benchmark_protocol_version: policy.benchmark_protocol_version,
        kind: RunKind::Live,
        run_id: format!(
            "live-{}-{}",
            now.format("%Y%m%dT%H%M%S%fZ"),
            std::process::id()
        ),
        started_at: now.to_rfc3339(),
        completed_at: String::new(),
        manifest_version: manifest.version,
        rounds: manifest.rounds,
        fixture_fingerprint: fixture_fingerprint(manifest)?,
        catalog_fingerprint: catalog_fingerprint()?,
        suites: suite_names(suites),
        models: selected_models(suites, text_models, vision_models),
    };
    Recorder::new_live(
        &output,
        PendingRun {
            history_root: super::setup::history_root(),
            metadata,
        },
    )
}

pub(super) fn complete_live_run(output: &Path, mut pending: PendingRun) -> Result<()> {
    pending.metadata.completed_at = chrono::Utc::now().to_rfc3339();
    write_new_json(&output.join("run.json"), &pending.metadata)?;
    let manifest = Manifest::load()?;
    let report = latest::refresh(
        &pending.history_root,
        &manifest,
        Some(output),
        &HistoryPolicy::load()?,
    )?;
    println!(
        "Catalog benchmark latest history: {} ({} row(s) ready)",
        pending.history_root.display(),
        report.ready_row_count()
    );
    Ok(())
}

pub(super) fn refresh_current_history() -> Result<()> {
    let manifest = Manifest::load()?;
    manifest.validate()?;
    let root = super::setup::history_root();
    latest::refresh(&root, &manifest, None, &HistoryPolicy::load()?)?;
    Ok(())
}

pub(super) fn register_existing_live_run(output: &Path) -> Result<()> {
    let manifest = Manifest::load()?;
    manifest.validate()?;
    let policy = HistoryPolicy::load()?;
    if output.join("run.json").exists() {
        latest::refresh(
            &super::setup::history_root(),
            &manifest,
            Some(output),
            &policy,
        )?;
        return Ok(());
    }
    let attempts = read_attempts(&output.join("attempts.jsonl"))?;
    ensure!(!attempts.is_empty(), "cannot register an empty report");
    let completed_at = summary_timestamp(output)?;
    let models = models_from_attempts(&attempts)?;
    let metadata = RunMetadata {
        version: RUN_METADATA_VERSION,
        benchmark_protocol_version: policy.benchmark_protocol_version,
        kind: RunKind::ImportedLive,
        run_id: imported_run_id(output, &completed_at),
        started_at: completed_at.clone(),
        completed_at,
        manifest_version: manifest.version,
        rounds: manifest.rounds,
        fixture_fingerprint: fixture_fingerprint(&manifest)?,
        catalog_fingerprint: catalog_fingerprint()?,
        suites: attempts
            .iter()
            .map(|attempt| attempt.suite.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        models,
    };
    ensure!(
        latest::has_complete_group(metadata.clone(), attempts, &manifest),
        "report has no complete model/suite row for the current fixtures"
    );
    write_new_json(&output.join("run.json"), &metadata)?;
    latest::refresh(
        &super::setup::history_root(),
        &manifest,
        Some(output),
        &policy,
    )?;
    Ok(())
}

fn suite_names(suites: Suites) -> Vec<String> {
    [
        ("text", suites.text),
        ("coordinate", suites.coordinate),
        ("ocr", suites.ocr),
    ]
    .into_iter()
    .filter(|(_, selected)| *selected)
    .map(|(name, _)| name.to_string())
    .collect()
}

fn selected_models(
    suites: Suites,
    text_models: &[ModelConfig],
    vision_models: &[ModelConfig],
) -> Vec<ModelIdentity> {
    let mut identities = BTreeMap::new();
    let models = text_models.iter().filter(|_| suites.text).chain(
        vision_models
            .iter()
            .filter(|_| suites.coordinate || suites.ocr),
    );
    for model in models {
        let identity = model_identity(model);
        identities.insert(
            (
                identity.id.clone(),
                identity.provider.clone(),
                identity.api_model.clone(),
                identity.reasoning_policy.clone(),
            ),
            identity,
        );
    }
    identities.into_values().collect()
}

fn model_identity(model: &ModelConfig) -> ModelIdentity {
    ModelIdentity {
        id: model.id.clone(),
        provider: model.provider.clone(),
        api_model: model.full_name.clone(),
        reasoning_policy: reasoning_policy_label(model),
    }
}

fn models_from_attempts(attempts: &[Attempt]) -> Result<Vec<ModelIdentity>> {
    let mut identities = BTreeMap::new();
    for attempt in attempts {
        ensure!(
            !attempt.model_name.trim().is_empty(),
            "imported model {} has no recorded API endpoint",
            attempt.model_id
        );
        let key = (
            attempt.model_id.clone(),
            attempt.provider.clone(),
            attempt.reasoning_policy.clone(),
        );
        let identity = ModelIdentity {
            id: attempt.model_id.clone(),
            provider: attempt.provider.clone(),
            api_model: attempt.model_name.clone(),
            reasoning_policy: attempt.reasoning_policy.clone(),
        };
        if let Some(previous) = identities.insert(key, identity) {
            ensure!(
                previous.api_model == attempt.model_name,
                "imported model {} mixes API endpoints",
                attempt.model_id
            );
        }
    }
    Ok(identities.into_values().collect())
}

fn fixture_fingerprint(manifest: &Manifest) -> Result<String> {
    let mut hasher = Sha256::new();
    let descriptor = serde_json::to_vec(&(
        manifest.version,
        manifest.rounds,
        &manifest.text_cases,
        &manifest.coordinate_cases,
        &manifest.ocr_cases,
    ))?;
    hash_piece(&mut hasher, &descriptor);
    let images = manifest
        .coordinate_cases
        .iter()
        .map(|case| case.image.as_str())
        .chain(manifest.ocr_cases.iter().map(|case| case.image.as_str()))
        .collect::<BTreeSet<_>>();
    for relative in images {
        hash_piece(&mut hasher, relative.as_bytes());
        let path = manifest.image_path(relative);
        hash_piece(
            &mut hasher,
            &std::fs::read(&path).with_context(|| format!("read {}", path.display()))?,
        );
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn catalog_fingerprint() -> Result<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("catalog/model_catalog.json");
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn hash_piece(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn summary_timestamp(output: &Path) -> Result<String> {
    #[derive(Deserialize)]
    struct Timestamp {
        generated_at: String,
    }
    let path = output.join("summary.json");
    let timestamp: Timestamp = serde_json::from_slice(
        &std::fs::read(&path).with_context(|| format!("read {}", path.display()))?,
    )?;
    chrono::DateTime::parse_from_rfc3339(&timestamp.generated_at)
        .context("invalid summary timestamp")?;
    Ok(timestamp.generated_at)
}

fn imported_run_id(output: &Path, completed_at: &str) -> String {
    let path_hash = Sha256::digest(output.to_string_lossy().as_bytes());
    let path_suffix = path_hash[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "import-{}-{}",
        completed_at.replace([':', '-', '.', '+'], ""),
        path_suffix
    )
}

fn write_new_json(path: &Path, value: &impl Serialize) -> Result<()> {
    ensure!(!path.exists(), "{} already exists", path.display());
    write_bytes(path, &serde_json::to_vec_pretty(value)?)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let temporary = path.with_extension(format!("{extension}.tmp-{}", std::process::id()));
    let mut file = std::fs::File::create(&temporary)
        .with_context(|| format!("create {}", temporary.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    if path.exists() {
        std::fs::remove_file(path).with_context(|| format!("replace {}", path.display()))?;
    }
    std::fs::rename(&temporary, path)
        .with_context(|| format!("move {} to {}", temporary.display(), path.display()))
}

#[cfg(test)]
mod tests;
