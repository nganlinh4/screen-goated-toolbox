use super::navigation::{
    NavigationDispatchMethod, NavigationVerification, VerificationFailure,
    navigation_dispatch_method, navigation_dispatch_receipt, navigation_success,
    navigation_verification_failure, tab_navigation_before_state,
};
use super::navigation_state::{MainFrameState, NavigationOutcome};
use super::*;

#[test]
fn exact_target_is_added_to_success_and_error_results() {
    assert_eq!(
        tag_target(json!({"ok": true}), Some(41))["target_tab_id"],
        41
    );
    assert_eq!(
        tag_target(json!({"ok": false, "error": "x"}), Some(42))["target_tab_id"],
        42
    );
    assert!(
        tag_target(json!({"ok": true}), None)
            .get("target_tab_id")
            .is_none()
    );
    assert_eq!(
        with_effect_verified(json!({"ok": false}), false)["effect_verified"],
        false
    );
}

#[test]
fn verified_navigation_keeps_dispatch_and_effect_evidence_distinct() {
    let before = MainFrameState {
        url: "https://example.invalid/before".to_string(),
        unreachable_url: None,
        loader_id: Some("before-loader".to_string()),
    };
    let verified = NavigationVerification {
        outcome: NavigationOutcome::Redirect,
        state: MainFrameState {
            url: "https://example.invalid/after".to_string(),
            unreachable_url: None,
            loader_id: Some("after-loader".to_string()),
        },
        attempts: 2,
        elapsed_ms: 25,
    };
    let result = navigation_success(
        "http://example.invalid/requested",
        9,
        Some(&before),
        json!({"status": "accepted", "loader_id": "after-loader"}),
        verified,
    );

    assert_eq!(result["ok"], true);
    assert_eq!(result["effect_verified"], true);
    assert_eq!(result["dispatch"]["status"], "accepted");
    assert_eq!(result["verification"]["status"], "committed");
    assert_eq!(result["verification"]["outcome"], "redirect");
    assert_eq!(result["target_tab_id"], 9);
}

#[test]
fn exact_tab_rpc_is_used_only_when_cdp_cannot_inspect_the_source() {
    assert_eq!(
        navigation_dispatch_method(false, true),
        NavigationDispatchMethod::ExactTabRpc
    );
    assert_eq!(
        navigation_dispatch_method(true, true),
        NavigationDispatchMethod::Cdp
    );
    assert_eq!(
        navigation_dispatch_method(false, false),
        NavigationDispatchMethod::Cdp
    );
}

#[test]
fn exact_tab_rpc_preserves_pre_navigation_identity_for_verification() {
    let response = json!({
        "id": 73,
        "beforeUrl": "restricted-surface://control/",
        "url": "restricted-surface://control/",
        "pendingUrl": "https://example.invalid/destination",
    });
    let before = tab_navigation_before_state(&response).expect("before URL");
    assert_eq!(before.committed_url(), "restricted-surface://control/");

    let (receipt, loader_id) =
        navigation_dispatch_receipt(&Ok(response), NavigationDispatchMethod::ExactTabRpc);
    assert_eq!(receipt["status"], "accepted");
    assert_eq!(receipt["method"], "tabs.navigate");
    assert_eq!(receipt["tab_id"], 73);
    assert_eq!(loader_id, None);
}

#[test]
fn cancelled_navigation_reports_unknown_effect_after_dispatch() {
    let failure = VerificationFailure {
        last_state: None,
        last_error: None,
        attempts: 1,
        transition_seen: false,
        cancelled: true,
    };
    let result = navigation_verification_failure(
        "https://example.invalid/after",
        12,
        None,
        json!({"status": "accepted"}),
        failure,
        10,
    );

    assert_eq!(result["cancelled"], true);
    assert_eq!(result["effect_may_have_occurred"], true);
    assert_eq!(result["verification"]["status"], "cancelled");
}

#[test]
fn cancelled_selector_wait_is_reported_as_nonmutating() {
    let result = cancelled_result("browser_wait_for", false);

    assert_eq!(result["cancelled"], true);
    assert_eq!(result["effect_may_have_occurred"], false);
}
