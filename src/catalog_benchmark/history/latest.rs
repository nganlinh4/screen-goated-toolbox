use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Serialize;

use super::{
    HistoryPolicy, RUN_METADATA_VERSION, RunMetadata, catalog_latency_eligible,
    fixture_fingerprint, write_bytes,
};
use crate::catalog_benchmark::manifest::Manifest;
use crate::catalog_benchmark::report::{Attempt, mean, percentile, read_attempts};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct GroupKey {
    suite: String,
    model_id: String,
    provider: String,
    api_model: String,
    reasoning_policy: String,
}

struct GroupSample {
    run_id: String,
    completed_at: String,
    model_name: String,
    attempts: Vec<Attempt>,
}

struct StoredRun {
    metadata: RunMetadata,
    attempts: Vec<Attempt>,
}

type ExpectedCases = BTreeMap<String, BTreeMap<u8, String>>;

fn expected_cases(manifest: &Manifest) -> ExpectedCases {
    BTreeMap::from([
        (
            "text".to_string(),
            manifest
                .text_cases
                .iter()
                .map(|case| (case.difficulty, case.id.clone()))
                .collect(),
        ),
        (
            "coordinate".to_string(),
            manifest
                .coordinate_cases
                .iter()
                .map(|case| (case.difficulty, case.id.clone()))
                .collect(),
        ),
        (
            "ocr".to_string(),
            manifest
                .ocr_cases
                .iter()
                .map(|case| (case.difficulty, case.id.clone()))
                .collect(),
        ),
    ])
}

pub(super) fn has_complete_group(
    metadata: RunMetadata,
    attempts: Vec<Attempt>,
    manifest: &Manifest,
) -> bool {
    !complete_groups(metadata, attempts, &expected_cases(manifest)).is_empty()
}

fn complete_groups(
    metadata: RunMetadata,
    attempts: Vec<Attempt>,
    expected: &ExpectedCases,
) -> Vec<(GroupKey, GroupSample)> {
    let selected_suites = metadata
        .suites
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let identities: BTreeMap<_, _> = metadata
        .models
        .iter()
        .map(|model| {
            (
                (
                    model.id.as_str(),
                    model.provider.as_str(),
                    model.reasoning_policy.as_str(),
                ),
                model.api_model.as_str(),
            )
        })
        .collect();
    let mut groups: BTreeMap<(String, String, String, String), Vec<Attempt>> = BTreeMap::new();
    for attempt in attempts {
        groups
            .entry((
                attempt.suite.clone(),
                attempt.model_id.clone(),
                attempt.provider.clone(),
                attempt.reasoning_policy.clone(),
            ))
            .or_default()
            .push(attempt);
    }
    groups
        .into_iter()
        .filter_map(
            |((suite, model_id, provider, reasoning_policy), attempts)| {
                selected_suites.contains(suite.as_str()).then_some(())?;
                let cases = expected.get(&suite)?;
                let api_model = identities.get(&(
                    model_id.as_str(),
                    provider.as_str(),
                    reasoning_policy.as_str(),
                ))?;
                let complete = attempts.len() == cases.len()
                    && attempts
                        .iter()
                        .all(|attempt| attempt.model_name == *api_model)
                    && attempts.iter().all(|attempt| {
                        attempt.round == attempt.difficulty
                            && cases.get(&attempt.difficulty) == Some(&attempt.case_id)
                    })
                    && attempts
                        .iter()
                        .map(|attempt| (attempt.round, attempt.case_id.as_str()))
                        .collect::<BTreeSet<_>>()
                        .len()
                        == cases.len();
                complete.then(|| {
                    let model_name = attempts[0].model_name.clone();
                    (
                        GroupKey {
                            suite,
                            model_id,
                            provider,
                            api_model: (*api_model).to_string(),
                            reasoning_policy,
                        },
                        GroupSample {
                            run_id: metadata.run_id.clone(),
                            completed_at: metadata.completed_at.clone(),
                            model_name,
                            attempts,
                        },
                    )
                })
            },
        )
        .collect()
}

#[derive(Debug, Serialize)]
pub(super) struct LatestReport {
    generated_at: String,
    fixture_fingerprint: String,
    policy: HistoryPolicy,
    eligible_compatible_runs: usize,
    rows: Vec<LatestRow>,
}

impl LatestReport {
    pub(super) fn ready_row_count(&self) -> usize {
        self.rows.iter().filter(|row| row.decision_ready).count()
    }
}

#[derive(Debug, Serialize)]
struct LatestRow {
    suite: String,
    model_id: String,
    model_name: String,
    provider: String,
    api_model: String,
    reasoning_policy: String,
    run_id: String,
    completed_at: String,
    decision_ready: bool,
    attempts: usize,
    successes: usize,
    success_rate: f64,
    mean_accuracy_score: Option<f64>,
    strict_pass_rate: Option<f64>,
    catalog_latency_attempts: usize,
    catalog_latency_ms: Option<u64>,
    catalog_p95_latency_ms: Option<f64>,
    all_case_median_latency_ms: Option<f64>,
    all_case_p95_latency_ms: Option<f64>,
    errors: BTreeMap<String, usize>,
}

pub(super) fn refresh(
    root: &Path,
    manifest: &Manifest,
    extra_output: Option<&Path>,
    policy: &HistoryPolicy,
) -> Result<LatestReport> {
    std::fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
    let fingerprint = fixture_fingerprint(manifest)?;
    let mut paths = metadata_paths(root)?;
    if let Some(path) = extra_output.map(|path| path.join("run.json"))
        && path.exists()
        && !paths.contains(&path)
    {
        paths.push(path);
    }
    let mut runs = BTreeMap::new();
    for path in paths {
        match load_stored_run(&path) {
            Ok(run)
                if run.metadata.fixture_fingerprint == fingerprint
                    && run.metadata.benchmark_protocol_version
                        == policy.benchmark_protocol_version
                    && run.metadata.manifest_version == manifest.version
                    && run.metadata.rounds == manifest.rounds =>
            {
                runs.entry(run.metadata.run_id.clone()).or_insert(run);
            }
            Ok(_) => {}
            Err(error) => eprintln!("BENCH_HISTORY_SKIP path={} error={error:#}", path.display()),
        }
    }
    let eligible_compatible_runs = runs.len();
    let expected = expected_cases(manifest);
    let rows = build_rows(runs.into_values().collect(), &expected, policy);
    let report = LatestReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        fixture_fingerprint: fingerprint,
        policy: policy.clone(),
        eligible_compatible_runs,
        rows,
    };
    write_json(&root.join("latest.json"), &report)?;
    write_bytes(&root.join("latest.md"), latest_markdown(&report).as_bytes())?;
    println!("Catalog benchmark latest report: {}", root.display());
    Ok(report)
}

fn build_rows(
    mut runs: Vec<StoredRun>,
    expected: &ExpectedCases,
    policy: &HistoryPolicy,
) -> Vec<LatestRow> {
    runs.sort_by(|left, right| {
        right
            .metadata
            .completed_at
            .cmp(&left.metadata.completed_at)
            .then_with(|| right.metadata.run_id.cmp(&left.metadata.run_id))
    });
    let mut latest = BTreeMap::new();
    for run in runs {
        for (key, sample) in complete_groups(run.metadata, run.attempts, expected) {
            latest.entry(key).or_insert(sample);
        }
    }
    latest
        .into_iter()
        .map(|(key, sample)| summarize_latest(key, sample, policy))
        .collect()
}

fn summarize_latest(key: GroupKey, sample: GroupSample, policy: &HistoryPolicy) -> LatestRow {
    let mut catalog_latencies = Vec::new();
    let mut all_case_latencies = Vec::new();
    let mut scores = Vec::new();
    let mut strict = Vec::new();
    let mut successes = 0;
    let mut errors = BTreeMap::new();
    for attempt in &sample.attempts {
        if attempt.status == "success" {
            successes += 1;
            all_case_latencies.push(attempt.latency_ms as f64);
            if catalog_latency_eligible(attempt, policy.vision_representative_max_edge_px) {
                catalog_latencies.push(attempt.latency_ms as f64);
            }
            scores.extend(attempt.score);
            strict.extend(attempt.strict_pass);
        } else {
            *errors.entry(attempt.status.clone()).or_insert(0) += 1;
        }
    }
    catalog_latencies.sort_by(f64::total_cmp);
    all_case_latencies.sort_by(f64::total_cmp);
    let required_catalog_cases = match key.suite.as_str() {
        "text" => 1,
        "coordinate" | "ocr" => policy.minimum_representative_cases_per_vision_suite,
        _ => usize::MAX,
    };
    let decision_ready = catalog_latencies.len() >= required_catalog_cases;
    let catalog_median = percentile(&catalog_latencies, 0.5);
    let attempts = sample.attempts.len();
    LatestRow {
        suite: key.suite,
        model_id: key.model_id,
        model_name: sample.model_name,
        provider: key.provider,
        api_model: key.api_model,
        reasoning_policy: key.reasoning_policy,
        run_id: sample.run_id,
        completed_at: sample.completed_at,
        decision_ready,
        attempts,
        successes,
        success_rate: if attempts == 0 {
            0.0
        } else {
            successes as f64 / attempts as f64
        },
        mean_accuracy_score: mean(&scores),
        strict_pass_rate: (!strict.is_empty())
            .then(|| strict.iter().filter(|passed| **passed).count() as f64 / strict.len() as f64),
        catalog_latency_attempts: catalog_latencies.len(),
        catalog_latency_ms: decision_ready
            .then(|| catalog_median.map(|value| value.round() as u64))
            .flatten(),
        catalog_p95_latency_ms: percentile(&catalog_latencies, 0.95),
        all_case_median_latency_ms: percentile(&all_case_latencies, 0.5),
        all_case_p95_latency_ms: percentile(&all_case_latencies, 0.95),
        errors,
    }
}

fn load_stored_run(metadata_path: &Path) -> Result<StoredRun> {
    let metadata: RunMetadata = serde_json::from_slice(
        &std::fs::read(metadata_path)
            .with_context(|| format!("read {}", metadata_path.display()))?,
    )
    .with_context(|| format!("parse {}", metadata_path.display()))?;
    ensure!(
        metadata.version == RUN_METADATA_VERSION,
        "unsupported run metadata version"
    );
    chrono::DateTime::parse_from_rfc3339(&metadata.completed_at)
        .context("invalid completion timestamp")?;
    let output = metadata_path
        .parent()
        .context("run metadata has no parent")?;
    Ok(StoredRun {
        metadata,
        attempts: read_attempts(&output.join("attempts.jsonl"))?,
    })
}

fn metadata_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .with_context(|| format!("read {}", directory.display()))?
        {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if entry.file_name() == "run.json" {
                found.push(entry.path());
            }
        }
    }
    found.sort();
    Ok(found)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    write_bytes(path, &serde_json::to_vec_pretty(value)?)
}

fn latest_markdown(report: &LatestReport) -> String {
    let mut output = format!(
        "# Latest catalog benchmark\n\nGenerated: {}  \nFixture: `{}`  \nCompatible recorded runs: {}  \nSelection: newest complete run per model, suite, endpoint, and reasoning policy.\n\n",
        report.generated_at, report.fixture_fingerprint, report.eligible_compatible_runs
    );
    output.push_str(&format!(
        "Representative vision latency uses effective inputs at or below {} px longest edge and needs at least {} successful representative cases. OCR owns general vision catalog timing; coordinate rows are control-task evidence.\n\n",
        report.policy.vision_representative_max_edge_px,
        report
            .policy
            .minimum_representative_cases_per_vision_suite
    ));
    output.push_str("| Suite | Model | Provider endpoint | Reasoning | Run | Ready | Success | Accuracy aid | Strict | Catalog n | Catalog median ms | Catalog P95 ms | All-case median ms | All-case P95 ms |\n");
    output.push_str(
        "| --- | --- | --- | --- | --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );
    for row in &report.rows {
        output.push_str(&format!(
            "| {} | {} | {} / {} | {} | {} | {} | {}/{} | {} | {} | {} | {} | {} | {} | {} |\n",
            row.suite,
            row.model_id,
            row.provider,
            row.api_model,
            row.reasoning_policy,
            row.run_id,
            if row.decision_ready { "yes" } else { "no" },
            row.successes,
            row.attempts,
            format_optional(row.mean_accuracy_score),
            format_optional(row.strict_pass_rate),
            row.catalog_latency_attempts,
            row.catalog_latency_ms
                .map_or_else(|| "—".to_string(), |value| value.to_string()),
            format_optional(row.catalog_p95_latency_ms),
            format_optional(row.all_case_median_latency_ms),
            format_optional(row.all_case_p95_latency_ms),
        ));
    }
    output.push_str("\nA recovery report may be registered after it has been merged into one complete logical run. Older compatible runs remain historical evidence but never contribute to current catalog values. Translation accuracy still requires rubric-based human review.\n");
    output
}

fn format_optional(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_string(), |value| format!("{value:.3}"))
}

#[cfg(test)]
mod tests;
