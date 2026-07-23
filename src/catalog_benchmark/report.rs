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
    pub status: String,
    pub latency_ms: u128,
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

fn read_attempts(path: &Path) -> Result<Vec<Attempt>> {
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
    models: Vec<ModelSummary>,
}

#[derive(Debug, Serialize)]
struct ModelSummary {
    suite: String,
    model_id: String,
    model_name: String,
    provider: String,
    attempts: usize,
    successes: usize,
    success_rate: f64,
    mean_score: Option<f64>,
    score_stddev: Option<f64>,
    strict_pass_rate: Option<f64>,
    median_latency_ms: Option<f64>,
    p95_latency_ms: Option<f64>,
    latency_cv: Option<f64>,
    errors: BTreeMap<String, usize>,
}

pub struct Recorder {
    output_dir: std::path::PathBuf,
    jsonl: BufWriter<File>,
    attempts: Vec<Attempt>,
}

impl Recorder {
    pub fn new(output_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(output_dir)
            .with_context(|| format!("create {}", output_dir.display()))?;
        let jsonl = File::create(output_dir.join("attempts.jsonl"))?;
        Ok(Self {
            output_dir: output_dir.to_path_buf(),
            jsonl: BufWriter::new(jsonl),
            attempts: Vec::new(),
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
        let summary = summarize(&self.attempts);
        std::fs::write(
            self.output_dir.join("summary.json"),
            serde_json::to_vec_pretty(&summary)?,
        )?;
        std::fs::write(self.output_dir.join("summary.md"), markdown(&summary))?;
        println!("Catalog benchmark report: {}", self.output_dir.display());
        Ok(())
    }
}

fn summarize(attempts: &[Attempt]) -> Summary {
    let mut groups: BTreeMap<(&str, &str, &str, &str), Vec<&Attempt>> = BTreeMap::new();
    for attempt in attempts {
        groups
            .entry((
                &attempt.suite,
                &attempt.model_id,
                &attempt.model_name,
                &attempt.provider,
            ))
            .or_default()
            .push(attempt);
    }
    let models = groups
        .into_iter()
        .map(|((suite, model_id, model_name, provider), group)| {
            summarize_group(suite, model_id, model_name, provider, &group)
        })
        .collect();
    Summary {
        generated_at: chrono::Utc::now().to_rfc3339(),
        attempts: attempts.len(),
        models,
    }
}

fn summarize_group(
    suite: &str,
    model_id: &str,
    model_name: &str,
    provider: &str,
    attempts: &[&Attempt],
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
        attempts: attempts.len(),
        successes,
        success_rate: successes as f64 / attempts.len() as f64,
        mean_score: mean(&scores),
        score_stddev: stddev(&scores),
        strict_pass_rate: (!strict.is_empty())
            .then(|| strict.iter().filter(|value| **value).count() as f64 / strict.len() as f64),
        median_latency_ms: percentile(&latencies, 0.5),
        p95_latency_ms: percentile(&latencies, 0.95),
        latency_cv: match (mean_latency, stddev(&latencies)) {
            (Some(mean), Some(deviation)) if mean > 0.0 => Some(deviation / mean),
            _ => None,
        },
        errors,
    }
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn stddev(values: &[f64]) -> Option<f64> {
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

fn percentile(values: &[f64], percentile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let index = ((values.len() - 1) as f64 * percentile).ceil() as usize;
    values.get(index).copied()
}

fn markdown(summary: &Summary) -> String {
    let mut output = format!(
        "# Catalog benchmark report\n\nGenerated: {}  \nAttempts: {}\n\n",
        summary.generated_at, summary.attempts
    );
    output.push_str("| Suite | Model | Provider | Success | Mean accuracy | Strict pass | Median ms | P95 ms | Latency CV |\n");
    output.push_str("| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for model in &summary.models {
        output.push_str(&format!(
            "| {} | {} | {} | {}/{} | {} | {} | {} | {} | {} |\n",
            model.suite,
            model.model_id,
            model.provider,
            model.successes,
            model.attempts,
            format_optional(model.mean_score),
            format_optional(model.strict_pass_rate),
            format_optional(model.median_latency_ms),
            format_optional(model.p95_latency_ms),
            format_optional(model.latency_cv),
        ));
    }
    output.push_str("\nTranslation accuracy is an automatic reference-similarity aid; inspect `attempts.jsonl` against each rubric before ranking models.\n");
    output
}

fn format_optional(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_string(), |value| format!("{value:.3}"))
}

#[cfg(test)]
mod tests {
    use super::percentile;

    #[test]
    fn percentile_uses_nearest_rank_without_mutating_input() {
        assert_eq!(percentile(&[10.0, 20.0, 30.0, 40.0, 50.0], 0.5), Some(30.0));
        assert_eq!(
            percentile(&[10.0, 20.0, 30.0, 40.0, 50.0], 0.95),
            Some(50.0)
        );
    }
}
