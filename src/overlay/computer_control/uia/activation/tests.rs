use super::*;

#[test]
fn patterns_choose_activation_without_role_or_label_heuristics() {
    assert_eq!(
        choose_activation(Capabilities {
            invoke: true,
            selection_item: true,
            ..Capabilities::default()
        }),
        Ok(Plan::Invoke)
    );
    assert_eq!(
        choose_activation(Capabilities {
            expansion: Some(Expansion::Collapsed),
            ..Capabilities::default()
        }),
        Ok(Plan::Expand)
    );
    assert_eq!(
        choose_activation(Capabilities {
            expansion: Some(Expansion::Expanded),
            ..Capabilities::default()
        }),
        Ok(Plan::Collapse)
    );
    assert_eq!(
        choose_activation(Capabilities {
            legacy_default: true,
            ..Capabilities::default()
        }),
        Ok(Plan::LegacyDefault)
    );
}

#[test]
fn selection_never_escalates_into_activation() {
    assert_eq!(
        choose_activation(Capabilities {
            selection_item: true,
            ..Capabilities::default()
        }),
        Err(Unsupported::SelectionOnly)
    );
}

#[test]
fn no_pattern_fails_closed() {
    assert_eq!(
        choose_activation(Capabilities::default()),
        Err(Unsupported::NoDefaultAction)
    );
}

#[test]
fn final_edge_requires_both_roots_to_match_the_observed_window() {
    assert!(validate_roots(9, 9, 9).is_ok());
    let focus_drift = validate_roots(9, 10, 9).unwrap_err();
    assert_eq!(focus_drift.kind(), FailureKind::StaleTarget);
    assert!(!focus_drift.effect_may_have_occurred());
    let point_drift = validate_roots(9, 9, 10).unwrap_err();
    assert_eq!(point_drift.kind(), FailureKind::StaleTarget);
    assert!(!point_drift.effect_may_have_occurred());
}

#[test]
fn expiring_preflight_prevents_a_late_dispatch_claim() {
    let state = AtomicU8::new(DISPATCH_PENDING);
    assert!(stop_pending_dispatch(&state, DISPATCH_EXPIRED));
    assert_eq!(
        claim_dispatch(&state).unwrap_err().kind(),
        FailureKind::Timeout
    );
}

#[test]
fn cancellation_before_claim_prevents_late_dispatch() {
    let state = AtomicU8::new(DISPATCH_PENDING);
    let cancel = AtomicBool::new(true);
    let (_tx, rx) = std::sync::mpsc::channel();
    let error = wait_for_result(rx, &state, &cancel).unwrap_err();
    assert_eq!(error.kind(), FailureKind::Cancelled);
    assert!(!error.effect_may_have_occurred());
    assert_eq!(state.load(Ordering::Acquire), DISPATCH_CANCELLED);
    assert_eq!(
        claim_dispatch(&state).unwrap_err().kind(),
        FailureKind::Cancelled
    );
}

#[test]
fn cancellation_after_claim_reports_possible_effect() {
    let state = AtomicU8::new(DISPATCH_STARTED);
    let cancel = AtomicBool::new(true);
    let (_tx, rx) = std::sync::mpsc::channel();
    let error = wait_for_result(rx, &state, &cancel).unwrap_err();
    assert_eq!(error.kind(), FailureKind::Cancelled);
    assert!(error.effect_may_have_occurred());
}

#[test]
fn cancellation_wins_when_result_arrives_in_the_same_poll() {
    let state = AtomicU8::new(DISPATCH_STARTED);
    let cancel = AtomicBool::new(true);
    let result = Ok(ActivationReceipt {
        method: "uia_invoke",
        dry_run: false,
    });
    let error = received_result(result, &state, &cancel).unwrap_err();
    assert_eq!(error.kind(), FailureKind::Cancelled);
    assert!(error.effect_may_have_occurred());
}
