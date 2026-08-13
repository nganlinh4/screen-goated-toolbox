use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Attempt {
    pub suite: String,
    pub round: u8,
    pub difficulty: u8,
    pub case_id: String,
    pub model_id: String,
    pub model_name: String,
    pub provider: String,
    /// Effective ordinary-request reasoning policy used by the production path.
    #[serde(default)]
    pub reasoning_policy: String,
    pub status: String,
    /// End-to-end request time through the final response.
    pub latency_ms: u128,
    /// Request start through the first callback that contributes to the final
    /// user-visible response. Thinking placeholders are excluded.
    #[serde(default)]
    pub time_to_first_output_ms: Option<u128>,
    /// First user-visible output through request completion.
    #[serde(default)]
    pub generation_duration_ms: Option<u128>,
    #[serde(default)]
    pub output_chars: Option<usize>,
    #[serde(default)]
    pub end_to_end_chars_per_second: Option<f64>,
    #[serde(default)]
    pub generation_chars_per_second: Option<f64>,
    pub score: Option<f64>,
    pub strict_pass: Option<bool>,
    pub response: Option<String>,
    pub error: Option<String>,
    pub details: serde_json::Value,
    pub reference: Option<String>,
    pub rubric: Vec<String>,
    pub manual_review_required: bool,
}

pub type AttemptKey = (String, String, u8, String);

pub fn successful_attempt_keys(inputs: &[PathBuf]) -> Result<HashSet<AttemptKey>> {
    let mut keys = HashSet::new();
    for path in inputs {
        for attempt in read_attempts(path)? {
            if attempt.status == "success" {
                keys.insert(attempt_key(&attempt));
            }
        }
    }
    Ok(keys)
}

/// Merge one or more benchmark JSONL files into a fresh report. Later inputs
/// replace earlier results for the same model/suite/case/round, which lets a
/// focused recovery run fill unavailable cells without rerunning every provider.
pub fn merge_reports(inputs: &[PathBuf], output_dir: &Path) -> Result<()> {
    anyhow::ensure!(
        !inputs.is_empty(),
        "no benchmark reports supplied for merge"
    );
    let mut attempts = BTreeMap::new();
    for path in inputs {
        for attempt in read_attempts(path)? {
            attempts.insert(attempt_key(&attempt), attempt);
        }
    }

    let mut recorder = Recorder::new(output_dir)?;
    for attempt in attempts.into_values() {
        recorder.push(attempt)?;
    }
    recorder.finish()
}

pub(super) fn read_attempts(path: &Path) -> Result<Vec<Attempt>> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    BufReader::new(file)
        .lines()
        .enumerate()
        .filter_map(|(index, line)| match line {
            Ok(line) if line.trim().is_empty() => None,
            line => Some((index, line)),
        })
        .map(|(index, line)| {
            let line = line.with_context(|| format!("read {}", path.display()))?;
            serde_json::from_str(&line).with_context(|| {
                format!("parse {} line {}", path.display(), index.saturating_add(1))
            })
        })
        .collect()
}

fn attempt_key(attempt: &Attempt) -> AttemptKey {
    (
        attempt.suite.clone(),
        attempt.model_id.clone(),
        attempt.round,
        attempt.case_id.clone(),
    )
}

#[derive(Debug, Serialize)]
struct Summary {
    generated_at: String,
    attempts: usize,
    vision_representative_max_edge_px: u32,
    models: Vec<ModelSummary>,
}

#[derive(Debug, Serialize)]
struct ModelSummary {
    suite: String,
    model_id: String,
    model_name: String,
    provider: String,
    reasoning_policy: String,
    attempts: usize,
    successes: usize,
    success_rate: f64,
    mean_score: Option<f64>,
    score_stddev: Option<f64>,
    strict_pass_rate: Option<f64>,
    catalog_latency_attempts: usize,
    catalog_median_latency_ms: Option<f64>,
    catalog_p95_latency_ms: Option<f64>,
    catalog_median_time_to_first_output_ms: Option<f64>,
    catalog_median_generation_duration_ms: Option<f64>,
    median_latency_ms: Option<f64>,
    warm_median_latency_ms: Option<f64>,
    median_time_to_first_output_ms: Option<f64>,
    median_generation_duration_ms: Option<f64>,
    median_end_to_end_chars_per_second: Option<f64>,
    median_generation_chars_per_second: Option<f64>,
    p95_latency_ms: Option<f64>,
    latency_cv: Option<f64>,
    errors: BTreeMap<String, usize>,
}

pub struct Recorder {
    output_dir: std::path::PathBuf,
    jsonl: BufWriter<File>,
    attempts: Vec<Attempt>,
    pending_run: Option<super::history::PendingRun>,
}

impl Recorder {
    pub fn new(output_dir: &Path) -> Result<Self> {
        Self::open(output_dir, None)
    }

    pub(super) fn new_live(
        output_dir: &Path,
        pending_run: super::history::PendingRun,
    ) -> Result<Self> {
        Self::open(output_dir, Some(pending_run))
    }

    fn open(output_dir: &Path, pending_run: Option<super::history::PendingRun>) -> Result<Self> {
        std::fs::create_dir_all(output_dir)
            .with_context(|| format!("create {}", output_dir.display()))?;
        let jsonl = File::create(output_dir.join("attempts.jsonl"))?;
        Ok(Self {
            output_dir: output_dir.to_path_buf(),
            jsonl: BufWriter::new(jsonl),
            attempts: Vec::new(),
            pending_run,
        })
    }

    pub fn push(&mut self, attempt: Attempt) -> Result<()> {
        serde_json::to_writer(&mut self.jsonl, &attempt)?;
        writeln!(self.jsonl)?;
        self.jsonl.flush()?;
        println!(
            "BENCH_RESULT suite={} round={} model={} status={} latency_ms={} score={:?}",
            attempt.suite,
            attempt.round,
            attempt.model_id,
            attempt.status,
            attempt.latency_ms,
            attempt.score
        );
        self.attempts.push(attempt);
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.jsonl.flush()?;
        let latency_policy = super::history::vision_latency_policy()?;
        let summary = summarize(&self.attempts, latency_policy.max_edge_px);
        std::fs::write(
            self.output_dir.join("summary.json"),
            serde_json::to_vec_pretty(&summary)?,
        )?;
        std::fs::write(self.output_dir.join("summary.md"), markdown(&summary))?;
        if let Some(pending_run) = self.pending_run.take() {
            super::history::complete_live_run(&self.output_dir, pending_run)?;
        }
        println!("Catalog benchmark report: {}", self.output_dir.display());
        Ok(())
    }
}

fn summarize(attempts: &[Attempt], vision_representative_max_edge_px: u32) -> Summary {
    let mut groups: BTreeMap<(&str, &str, &str, &str, &str), Vec<&Attempt>> = BTreeMap::new();
    for attempt in attempts {
        groups
            .entry((
                &attempt.suite,
                &attempt.model_id,
                &attempt.model_name,
                &attempt.provider,
                &attempt.reasoning_policy,
            ))
            .or_default()
            .push(attempt);
    }
    let models = groups
        .into_iter()
        .map(
            |((suite, model_id, model_name, provider, reasoning_policy), group)| {
                summarize_group(
                    suite,
                    model_id,
                    model_name,
                    provider,
                    reasoning_policy,
                    &group,
                    vision_representative_max_edge_px,
                )
            },
        )
        .collect();
    Summary {
        generated_at: chrono::Utc::now().to_rfc3339(),
        attempts: attempts.len(),
        vision_representative_max_edge_px,
        models,
    }
}

fn summarize_group(
    suite: &str,
    model_id: &str,
    model_name: &str,
    provider: &str,
    reasoning_policy: &str,
    attempts: &[&Attempt],
    vision_representative_max_edge_px: u32,
) -> ModelSummary {
    let successes = attempts
        .iter()
        .filter(|attempt| attempt.status == "success")
        .count();
    let scores: Vec<f64> = attempts
        .iter()
        .filter_map(|attempt| attempt.score)
        .collect();
    let strict: Vec<bool> = attempts
        .iter()
        .filter_map(|attempt| attempt.strict_pass)
        .collect();
    let mut latencies: Vec<f64> = attempts
        .iter()
        .filter(|attempt| attempt.status == "success")
        .map(|attempt| attempt.latency_ms as f64)
        .collect();
    latencies.sort_by(f64::total_cmp);
    let catalog_attempts = attempts
        .iter()
        .filter(|attempt| {
            attempt.status == "success"
                && super::history::catalog_latency_eligible(
                    attempt,
                    vision_representative_max_edge_px,
                )
        })
        .copied()
        .collect::<Vec<_>>();
    let mut catalog_latencies = catalog_attempts
        .iter()
        .map(|attempt| attempt.latency_ms as f64)
        .collect::<Vec<_>>();
    let mut catalog_first_output = catalog_attempts
        .iter()
        .filter_map(|attempt| attempt.time_to_first_output_ms.map(|value| value as f64))
        .collect::<Vec<_>>();
    let mut catalog_generation_duration = catalog_attempts
        .iter()
        .filter_map(|attempt| attempt.generation_duration_ms.map(|value| value as f64))
        .collect::<Vec<_>>();
    for values in [
        &mut catalog_latencies,
        &mut catalog_first_output,
        &mut catalog_generation_duration,
    ] {
        values.sort_by(f64::total_cmp);
    }
    let mut warm_latencies: Vec<f64> = attempts
        .iter()
        .filter(|attempt| attempt.status == "success" && attempt.round > 1)
        .map(|attempt| attempt.latency_ms as f64)
        .collect();
    warm_latencies.sort_by(f64::total_cmp);
    let mut first_output: Vec<f64> = successful_values(attempts, |attempt| {
        attempt.time_to_first_output_ms.map(|value| value as f64)
    });
    let mut generation_duration: Vec<f64> = successful_values(attempts, |attempt| {
        attempt.generation_duration_ms.map(|value| value as f64)
    });
    let mut end_to_end_rate =
        successful_values(attempts, |attempt| attempt.end_to_end_chars_per_second);
    let mut generation_rate =
        successful_values(attempts, |attempt| attempt.generation_chars_per_second);
    for values in [
        &mut first_output,
        &mut generation_duration,
        &mut end_to_end_rate,
        &mut generation_rate,
    ] {
        values.sort_by(f64::total_cmp);
    }
    let mut errors = BTreeMap::new();
    for attempt in attempts
        .iter()
        .filter(|attempt| attempt.status != "success")
    {
        *errors.entry(attempt.status.clone()).or_insert(0) += 1;
    }
    let mean_latency = mean(&latencies);
    ModelSummary {
        suite: suite.to_string(),
        model_id: model_id.to_string(),
        model_name: model_name.to_string(),
        provider: provider.to_string(),
        reasoning_policy: reasoning_policy.to_string(),
        attempts: attempts.len(),
        successes,
        success_rate: successes as f64 / attempts.len() as f64,
        mean_score: mean(&scores),
        score_stddev: stddev(&scores),
        strict_pass_rate: (!strict.is_empty())
            .then(|| strict.iter().filter(|value| **value).count() as f64 / strict.len() as f64),
        catalog_latency_attempts: catalog_latencies.len(),
        catalog_median_latency_ms: percentile(&catalog_latencies, 0.5),
        catalog_p95_latency_ms: percentile(&catalog_latencies, 0.95),
        catalog_median_time_to_first_output_ms: percentile(&catalog_first_output, 0.5),
        catalog_median_generation_duration_ms: percentile(&catalog_generation_duration, 0.5),
        median_latency_ms: percentile(&latencies, 0.5),
        warm_median_latency_ms: percentile(&warm_latencies, 0.5),
        median_time_to_first_output_ms: percentile(&first_output, 0.5),
        median_generation_duration_ms: percentile(&generation_duration, 0.5),
        median_end_to_end_chars_per_second: percentile(&end_to_end_rate, 0.5),
        median_generation_chars_per_second: percentile(&generation_rate, 0.5),
        p95_latency_ms: percentile(&latencies, 0.95),
        latency_cv: match (mean_latency, stddev(&latencies)) {
            (Some(mean), Some(deviation)) if mean > 0.0 => Some(deviation / mean),
            _ => None,
        },
        errors,
    }
}

fn successful_values(attempts: &[&Attempt], value: impl Fn(&Attempt) -> Option<f64>) -> Vec<f64> {
    attempts
        .iter()
        .filter(|attempt| attempt.status == "success")
        .filter_map(|attempt| value(attempt))
        .collect()
}

pub(super) fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

pub(super) fn stddev(values: &[f64]) -> Option<f64> {
    let mean = mean(values)?;
    Some(
        (values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / values.len() as f64)
            .sqrt(),
    )
}

pub(super) fn percentile(values: &[f64], percentile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let position = (values.len() - 1) as f64 * percentile.clamp(0.0, 1.0);
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let fraction = position - lower as f64;
    Some(values[lower] + (values[upper] - values[lower]) * fraction)
}

fn markdown(summary: &Summary) -> String {
    let mut output = format!(
        "# Catalog benchmark report\n\nGenerated: {}  \nAttempts: {}  \nRepresentative vision latency: effective longest edge <= {} px\n\n",
        summary.generated_at, summary.attempts, summary.vision_representative_max_edge_px
    );
    output.push_str("| Suite | Model | Provider | Reasoning policy | Success | Mean accuracy | Strict pass | Catalog n | Catalog TTFO ms | Catalog total ms | Catalog generation ms | Catalog P95 ms | All-case total ms | All-case P95 ms | Warm total ms | E2E char/s | Post-first char/s | All-case latency CV |\n");
    output.push_str("| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for model in &summary.models {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {}/{} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            model.suite,
            model.model_id,
            model.provider,
            model.reasoning_policy,
            model.successes,
            model.attempts,
            format_optional(model.mean_score),
            format_optional(model.strict_pass_rate),
            model.catalog_latency_attempts,
            format_optional(model.catalog_median_time_to_first_output_ms),
            format_optional(model.catalog_median_latency_ms),
            format_optional(model.catalog_median_generation_duration_ms),
            format_optional(model.catalog_p95_latency_ms),
            format_optional(model.median_latency_ms),
            format_optional(model.p95_latency_ms),
            format_optional(model.warm_median_latency_ms),
            format_optional(model.median_end_to_end_chars_per_second),
            format_optional(model.median_generation_chars_per_second),
            format_optional(model.latency_cv),
        ));
    }
    output.push_str("\nCatalog vision latency uses only the representative small-image cohort. Every selected case still contributes to accuracy, reliability, and all-case stress diagnostics. Translation accuracy is an automatic reference-similarity aid; inspect `attempts.jsonl` against each rubric before ranking models.\n");
    output
}

fn format_optional(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_string(), |value| format!("{value:.3}"))
}

#[cfg(test)]
mod tests {
    use super::percentile;

    #[test]
    fn percentile_interpolates_even_samples_without_mutating_input() {
        assert_eq!(percentile(&[10.0, 20.0, 30.0, 40.0, 50.0], 0.5), Some(30.0));
        assert_eq!(
            percentile(&[10.0, 20.0, 30.0, 40.0, 50.0], 0.95),
            Some(48.0)
        );
        assert_eq!(percentile(&[10.0, 20.0, 30.0, 40.0], 0.5), Some(25.0));
    }
}
