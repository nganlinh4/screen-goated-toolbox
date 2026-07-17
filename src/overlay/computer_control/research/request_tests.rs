use super::*;

#[test]
fn fixed_source_budget_reserves_missing_domain_slots() {
    let coverage = source_policy::Coverage {
        assessed: true,
        covered: vec!["covered.invalid".into()],
        missing: vec!["missing.invalid".into()],
    };
    assert_eq!(
        source_slot(1, 2, Some("covered.invalid"), &coverage),
        SourceSlot::Defer
    );
    assert_eq!(
        source_slot(1, 2, Some("missing.invalid"), &coverage),
        SourceSlot::Accept
    );
    assert_eq!(
        source_slot(2, 2, Some("missing.invalid"), &coverage),
        SourceSlot::Full
    );
}

#[test]
fn oversized_query_is_rejected_before_any_browser_effect() {
    let result = research_web(&json!({"query": "x".repeat(MAX_RESEARCH_QUERY_CHARS + 1)}));
    assert_eq!(result["code"], "ERR_RESEARCH_QUERY_TOO_LONG");
    assert_eq!(result["effect_may_have_occurred"], false);
    assert_eq!(result["executed"], false);
}

#[test]
fn missing_public_evidence_scope_is_rejected_before_any_browser_effect() {
    let result = research_web(&json!({"query": "short query"}));
    assert_eq!(result["code"], "ERR_RESEARCH_PURPOSE_REQUIRED");
    assert_eq!(result["effect_may_have_occurred"], false);
    assert_eq!(result["executed"], false);
}

#[test]
fn public_evidence_scope_enriches_ranking_without_polluting_provider_discovery() {
    assert_eq!(discovery_query("short query"), "short query");
    assert_eq!(
        discovery_query(&"q".repeat(MAX_RESEARCH_QUERY_CHARS))
            .chars()
            .count(),
        MAX_RESEARCH_QUERY_CHARS
    );
    assert_eq!(
        evidence_query("short query", "rank standard renewal evidence"),
        "short query rank standard renewal evidence"
    );
}

#[test]
fn repeated_source_failures_stop_only_after_usable_evidence_exists() {
    let mut diagnostics = SourceDiagnostics::default();
    let mut consecutive = 0;
    for _ in 0..6 {
        assert!(!source_failure_cutoff(
            0,
            true,
            &mut consecutive,
            &mut diagnostics
        ));
    }
    assert!(!diagnostics.source_failure_cutoff_reached);

    consecutive = 0;
    assert!(!source_failure_cutoff(
        MIN_SOURCES_BEFORE_FAILURE_CUTOFF,
        true,
        &mut consecutive,
        &mut diagnostics
    ));
    assert!(!source_failure_cutoff(
        MIN_SOURCES_BEFORE_FAILURE_CUTOFF,
        true,
        &mut consecutive,
        &mut diagnostics
    ));
    assert!(source_failure_cutoff(
        MIN_SOURCES_BEFORE_FAILURE_CUTOFF,
        true,
        &mut consecutive,
        &mut diagnostics
    ));
    assert!(diagnostics.source_failure_cutoff_reached);
    assert_eq!(
        diagnostics.consecutive_source_failures_at_cutoff,
        MAX_CONSECUTIVE_SOURCE_FAILURES_WITH_EVIDENCE
    );
}

#[test]
fn repeated_source_failures_do_not_abandon_requested_coverage() {
    let mut diagnostics = SourceDiagnostics::default();
    let mut consecutive = 0;
    for _ in 0..MAX_CONSECUTIVE_SOURCE_FAILURES_WITH_EVIDENCE + 2 {
        assert!(!source_failure_cutoff(
            MIN_SOURCES_BEFORE_FAILURE_CUTOFF,
            false,
            &mut consecutive,
            &mut diagnostics,
        ));
    }
    assert!(!diagnostics.source_failure_cutoff_reached);
}
