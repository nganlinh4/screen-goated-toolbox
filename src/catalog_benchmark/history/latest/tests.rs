use std::collections::BTreeMap;

use super::super::{HistoryPolicy, ModelIdentity, RunKind, RunMetadata};
use super::{ExpectedCases, GroupKey, GroupSample, StoredRun, build_rows, summarize_latest};
use crate::catalog_benchmark::report::Attempt;
use crate::catalog_benchmark::review::ReviewState;

fn policy() -> HistoryPolicy {
    HistoryPolicy {
        version: 3,
        benchmark_protocol_version: 3,
        selection: "latest_complete_run_per_model_suite".to_string(),
        vision_representative_max_edge_px: 1024,
        minimum_representative_cases_per_vision_suite: 4,
        latency_statistic: "latest_run_median".to_string(),
        accuracy_statistic: "human_review_for_manual_suites_else_latest_run_scores".to_string(),
        reliability_statistic: "latest_run_success_rate_including_errors".to_string(),
    }
}

fn vision_attempt(round: u8, latency_ms: u128, edge_px: u64) -> Attempt {
    Attempt {
        suite: "ocr".to_string(),
        round,
        difficulty: round,
        case_id: format!("vision-{round}"),
        model_id: "model-vision".to_string(),
        model_name: "api/vision".to_string(),
        provider: "provider".to_string(),
        reasoning_policy: "none".to_string(),
        status: "success".to_string(),
        latency_ms,
        output_chars: None,
        end_to_end_chars_per_second: None,
        score: Some(1.0),
        strict_pass: Some(true),
        response: None,
        error: None,
        details: serde_json::json!({
            "input_image_width": edge_px,
            "input_image_height": edge_px
        }),
        reference: None,
        rubric: Vec::new(),
        manual_review_required: false,
    }
}

fn expected() -> ExpectedCases {
    BTreeMap::from([(
        "text".to_string(),
        (1..=10)
            .map(|round| (round, format!("case-{round}")))
            .collect(),
    )])
}

fn stored_run(
    id: &str,
    day: u8,
    reasoning_policy: &str,
    latency_ms: u128,
    attempts: u8,
    failed_round: Option<u8>,
) -> StoredRun {
    let completed_at = format!("2026-07-{day:02}T12:00:00+00:00");
    let identity = ModelIdentity {
        id: "model-text".to_string(),
        provider: "provider".to_string(),
        api_model: "api/model".to_string(),
        reasoning_policy: reasoning_policy.to_string(),
    };
    StoredRun {
        metadata: RunMetadata {
            version: 1,
            benchmark_protocol_version: 3,
            kind: RunKind::Live,
            run_id: id.to_string(),
            started_at: completed_at.clone(),
            completed_at,
            manifest_version: 2,
            rounds: 10,
            fixture_fingerprint: "fixture".to_string(),
            catalog_fingerprint: "catalog".to_string(),
            suites: vec!["text".to_string()],
            models: vec![identity],
        },
        attempts: (1..=attempts)
            .map(|round| attempt(round, reasoning_policy, latency_ms, failed_round))
            .collect(),
        reviews: ReviewState::default(),
    }
}

fn attempt(
    round: u8,
    reasoning_policy: &str,
    latency_ms: u128,
    failed_round: Option<u8>,
) -> Attempt {
    let failed = failed_round == Some(round);
    Attempt {
        suite: "text".to_string(),
        round,
        difficulty: round,
        case_id: format!("case-{round}"),
        model_id: "model-text".to_string(),
        model_name: "api/model".to_string(),
        provider: "provider".to_string(),
        reasoning_policy: reasoning_policy.to_string(),
        status: if failed { "http-503" } else { "success" }.to_string(),
        latency_ms,
        output_chars: None,
        end_to_end_chars_per_second: None,
        score: (!failed).then_some(1.0),
        strict_pass: (!failed).then_some(true),
        response: None,
        error: failed.then(|| "overloaded".to_string()),
        details: serde_json::Value::Null,
        reference: None,
        rubric: Vec::new(),
        manual_review_required: false,
    }
}

#[test]
fn newest_complete_run_wins_and_counts_its_failures() {
    let runs = vec![
        stored_run("old", 1, "none", 10_000, 10, None),
        stored_run("newest-complete", 4, "none", 100, 10, Some(5)),
        stored_run("newer-incomplete", 5, "none", 1, 9, None),
        stored_run("different-reasoning", 6, "low", 80, 10, None),
    ];
    let rows = build_rows(runs, &expected(), &policy());
    assert_eq!(rows.len(), 2);
    let row = rows
        .iter()
        .find(|row| row.reasoning_policy == "none")
        .expect("ordinary reasoning row");
    assert_eq!(row.run_id, "newest-complete");
    assert!(row.decision_ready);
    assert_eq!(row.attempts, 10);
    assert_eq!(row.successes, 9);
    assert_eq!(row.success_rate, 0.9);
    assert_eq!(row.catalog_latency_ms, Some(100));
    assert_eq!(row.errors.get("http-503"), Some(&1));
}

#[test]
fn one_complete_run_is_catalog_ready() {
    let rows = build_rows(
        vec![stored_run("one", 1, "none", 100, 10, None)],
        &expected(),
        &policy(),
    );
    assert_eq!(rows.len(), 1);
    assert!(rows[0].decision_ready);
    assert_eq!(rows[0].catalog_latency_ms, Some(100));
}

#[test]
fn recorded_endpoint_must_match_run_identity() {
    let mut run = stored_run("one", 1, "none", 100, 10, None);
    run.attempts[0].model_name = "different/api-model".to_string();
    assert!(build_rows(vec![run], &expected(), &policy()).is_empty());
}

#[test]
fn representative_ocr_latency_excludes_large_stress_inputs() {
    let sample = GroupSample {
        run_id: "latest".to_string(),
        completed_at: "2026-07-26T12:00:00+00:00".to_string(),
        model_name: "api/vision".to_string(),
        attempts: vec![
            vision_attempt(1, 100, 600),
            vision_attempt(2, 110, 700),
            vision_attempt(3, 120, 800),
            vision_attempt(4, 130, 900),
            vision_attempt(5, 10_000, 2_560),
        ],
        human_scores: Vec::new(),
        reviewed_attempts: 0,
        review_required_attempts: 0,
    };
    let row = summarize_latest(
        GroupKey {
            suite: "ocr".to_string(),
            model_id: "model-vision".to_string(),
            provider: "provider".to_string(),
            api_model: "api/vision".to_string(),
            reasoning_policy: "none".to_string(),
        },
        sample,
        &policy(),
    );
    assert!(row.decision_ready);
    assert_eq!(row.catalog_latency_attempts, 4);
    assert_eq!(row.catalog_latency_ms, Some(115));
    assert_eq!(row.all_case_median_latency_ms, Some(120.0));
}

#[test]
fn missing_human_reviews_block_decision_readiness() {
    let sample = GroupSample {
        run_id: "latest".to_string(),
        completed_at: "2026-07-26T12:00:00+00:00".to_string(),
        model_name: "api/model".to_string(),
        attempts: vec![attempt(1, "none", 100, None)],
        human_scores: Vec::new(),
        reviewed_attempts: 9,
        review_required_attempts: 10,
    };
    let row = summarize_latest(
        GroupKey {
            suite: "text".to_string(),
            model_id: "model-text".to_string(),
            provider: "provider".to_string(),
            api_model: "api/model".to_string(),
            reasoning_policy: "none".to_string(),
        },
        sample,
        &policy(),
    );
    assert!(!row.decision_ready);
    assert_eq!(row.human_reviews, "9/10");
}
