use std::collections::HashMap;

use crate::model_config::ModelConfig;

use super::models::localized_model_label;

pub(super) struct TranslationDiagnostics {
    job_id: String,
    started_at: std::time::Instant,
    attempt_count: usize,
    success_count: usize,
    retry_success_count: usize,
    split_count: usize,
    failure_counts: HashMap<String, usize>,
    model_attempt_counts: HashMap<String, usize>,
}

pub(super) struct TranslationStartLog<'a> {
    pub(super) item_count: usize,
    pub(super) chunk_count: usize,
    pub(super) chunk_mode: Option<&'a str>,
    pub(super) target_language: &'a str,
    pub(super) instructions: Option<&'a str>,
    pub(super) selected_model_id: &'a str,
    pub(super) gtx_prioritized: bool,
    pub(super) smart_fallback: bool,
    pub(super) candidate_models: &'a [ModelConfig],
    pub(super) ui_language: &'a str,
}

impl TranslationDiagnostics {
    pub(super) fn new(job_id: &str) -> Self {
        Self {
            job_id: job_id.to_string(),
            started_at: std::time::Instant::now(),
            attempt_count: 0,
            success_count: 0,
            retry_success_count: 0,
            split_count: 0,
            failure_counts: HashMap::new(),
            model_attempt_counts: HashMap::new(),
        }
    }

    pub(super) fn log_start(&self, log: TranslationStartLog<'_>) {
        let TranslationStartLog {
            item_count,
            chunk_count,
            chunk_mode,
            target_language,
            instructions,
            selected_model_id,
            gtx_prioritized,
            smart_fallback,
            candidate_models,
            ui_language,
        } = log;
        let model_chain = candidate_models
            .iter()
            .map(|model| {
                format!(
                    "{}:{}",
                    model.provider,
                    localized_model_label(model, ui_language)
                )
            })
            .collect::<Vec<_>>()
            .join(" > ");
        eprintln!(
            "[SubtitleTranslation][job={}] start items={} initial_chunks={} chunk_mode={} target=\"{}\" instructions={} prioritized_model=\"{}\" gtx_prioritized={} smart_fallback={} fallback_chain=\"{}\"",
            self.job_id,
            item_count,
            chunk_count,
            chunk_mode.unwrap_or("auto"),
            target_language,
            instructions
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
            selected_model_id,
            gtx_prioritized,
            smart_fallback,
            if model_chain.is_empty() {
                "none"
            } else {
                &model_chain
            }
        );
    }

    pub(super) fn record_success(&mut self, model_id: &str, chunk_items: usize, attempts: usize) {
        self.success_count += 1;
        if attempts > 1 {
            self.retry_success_count += 1;
            eprintln!(
                "[SubtitleTranslation][job={}] retry-success model={} chunk_items={} attempts={}",
                self.job_id, model_id, chunk_items, attempts
            );
        }
    }

    pub(super) fn record_failure(
        &mut self,
        model_id: &str,
        model_label: &str,
        attempt: usize,
        chunk_items: usize,
        history_turns: usize,
        error: &str,
    ) {
        self.attempt_count += 1;
        *self
            .model_attempt_counts
            .entry(model_id.to_string())
            .or_default() += 1;
        let category = classify_translation_error(error);
        let key = format!("{model_id}:{category}");
        let category_count = self.failure_counts.entry(key).or_default();
        *category_count += 1;

        if *category_count == 1 || *category_count == 5 || (*category_count).is_multiple_of(20) {
            eprintln!(
                "[SubtitleTranslation][job={}] failure model=\"{}\" category={} count={} attempt={} chunk_items={} history_turns={} detail=\"{}\"",
                self.job_id,
                model_label,
                category,
                category_count,
                attempt,
                chunk_items,
                history_turns,
                truncate_log_detail(error, 220)
            );
        }
    }

    pub(super) fn record_split(&mut self, left_items: usize, right_items: usize, last_error: &str) {
        self.split_count += 1;
        eprintln!(
            "[SubtitleTranslation][job={}] split #{} left_items={} right_items={} reason_category={} reason=\"{}\"",
            self.job_id,
            self.split_count,
            left_items,
            right_items,
            classify_translation_error(last_error),
            truncate_log_detail(last_error, 180)
        );
    }

    pub(super) fn log_finish(
        &self,
        state: &str,
        translated_items: usize,
        requested_items: usize,
        completed_groups: usize,
        total_groups: usize,
    ) {
        let failures = summarize_count_map(&self.failure_counts);
        let model_attempts = summarize_count_map(&self.model_attempt_counts);
        eprintln!(
            "[SubtitleTranslation][job={}] finish state={} translated_items={}/{} groups={}/{} attempts={} successes={} retry_successes={} splits={} elapsed_ms={} failures=\"{}\" model_attempts=\"{}\"",
            self.job_id,
            state,
            translated_items,
            requested_items,
            completed_groups,
            total_groups,
            self.attempt_count,
            self.success_count,
            self.retry_success_count,
            self.split_count,
            self.started_at.elapsed().as_millis(),
            failures,
            model_attempts
        );
    }
}

fn classify_translation_error(error: &str) -> &'static str {
    let lower = error.to_lowercase();
    if lower.contains("gtx") {
        "gtx"
    } else if lower.contains("parse structured translation json") || lower.contains("json") {
        "json"
    } else if lower.contains("returned")
        && (lower.contains("item") || lower.contains("id") || lower.contains("empty text"))
    {
        "schema"
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "timeout"
    } else if lower.contains("rate limit") || lower.contains("429") {
        "rate-limit"
    } else if lower.contains("401") || lower.contains("403") || lower.contains("api key") {
        "auth"
    } else if lower.contains("network")
        || lower.contains("connection")
        || lower.contains("dns")
        || lower.contains("http")
        || lower.contains("request")
    {
        "transport"
    } else if lower.contains("no content") || lower.contains("empty") {
        "empty"
    } else {
        "other"
    }
}

fn truncate_log_detail(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    let mut truncated = compact.chars().take(max_chars).collect::<String>();
    truncated.push('…');
    truncated
}

fn summarize_count_map(counts: &HashMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    let mut entries = counts
        .iter()
        .map(|(key, count)| (key.as_str(), *count))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    entries
        .into_iter()
        .map(|(key, count)| format!("{key}={count}"))
        .collect::<Vec<_>>()
        .join(",")
}
