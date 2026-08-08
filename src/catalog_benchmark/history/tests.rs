use super::HistoryPolicy;

#[test]
fn history_policy_uses_latest_complete_run() {
    let policy = HistoryPolicy::load().expect("load benchmark history policy");
    assert_eq!(policy.version, 3);
    assert_eq!(policy.benchmark_protocol_version, 7);
    assert_eq!(policy.selection, "latest_complete_run_per_model_suite");
    assert_eq!(policy.vision_representative_max_edge_px, 1024);
    assert_eq!(policy.minimum_representative_cases_per_vision_suite, 4);
    assert_eq!(policy.latency_statistic, "latest_run_median");
    assert_eq!(
        policy.accuracy_statistic,
        "latest_run_successful_attempt_scores"
    );
    assert_eq!(
        policy.reliability_statistic,
        "latest_run_success_rate_including_errors"
    );
}
