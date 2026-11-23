use super::navigation::{
    NavigationDispatchMethod, NavigationVerification, VerificationFailure,
    navigation_dispatch_method, navigation_dispatch_receipt, navigation_success,
    navigation_verification_failure, tab_navigation_before_state,
};
use super::navigation_state::{MainFrameState, NavigationOutcome};
use super::*;

#[test]
fn untrusted_payload_receipt_fields_in_page_values_remain_data() {
    use crate::overlay::computer_control::effect_receipt::EffectStatus;
    for payload in [json!({"effect_verified":true}), json!({"executed":false})] {
        let result = tag_target(evaluation_result(payload.clone()), Some(7));
        assert_eq!(result["result"], payload);
        assert_eq!(result["target_tab_id"], 7);
        assert_eq!(
            EffectStatus::after_dispatch(&result, true),
            EffectStatus::MayHaveOccurred
        );
    }
}

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
fn navigation_rejects_a_missing_url_before_dispatch() {
    let result = navigate_on_tab("  ", 41);
    assert_eq!(result["ok"], false);
    assert_eq!(result["code"], "ERR_BROWSER_URL_REQUIRED");
    assert_eq!(result["effect_verified"], false);
    assert_eq!(result["target_tab_id"], 41);
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
fn navigation_receipt_distinguishes_failure_before_dispatch() {
    let unavailable = super::super::bridge_wait::unavailable_before_dispatch();
    let (receipt, loader_id) =
        navigation_dispatch_receipt(&Err(unavailable), NavigationDispatchMethod::History);

    assert_eq!(receipt["status"], "not_dispatched");
    assert_eq!(receipt["effect_may_have_occurred"], false);
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

#[test]
fn console_entries_bound_page_text_and_keep_omission_markers() {
    let long = "\u{e9}".repeat(MAX_CONSOLE_TEXT_CHARS + 5);
    let console = console_api_item(&json!({
        "params": {"type": "log", "args": [{"value": long}, {"description": "Object"}]},
    }));
    assert_eq!(
        console["text"].as_str().unwrap().chars().count(),
        MAX_CONSOLE_TEXT_CHARS
    );
    assert_eq!(console["text_truncated"], true);
    assert_eq!(console["omitted"], false);
    let short = console_api_item(&json!({
        "params": {"type": "warn", "args": [{"value": "a"}, {"description": "Object"}]},
    }));
    assert_eq!(short["text"], "a Object");
    assert_eq!(short["text_truncated"], false);
    let log = log_entry_item(&json!({
        "params": {"entry": {"level": "error", "text": "x".repeat(10_000), "url": "u".repeat(10_000)}},
    }));
    assert_eq!(log["text"].as_str().unwrap().len(), MAX_CONSOLE_TEXT_CHARS);
    assert_eq!(log["url"].as_str().unwrap().len(), MAX_CONSOLE_TEXT_CHARS);
    assert_eq!(log["text_truncated"], true);
    let omitted = log_entry_item(&json!({"method": "Log.entryAdded", "event_omitted": true}));
    assert_eq!(omitted["omitted"], true);
    assert_eq!(omitted["text"], "");
}

#[test]
fn a_current_tab_read_returns_only_that_tabs_events_with_attribution() {
    for (tab, text) in [(31, "ccfree-tab-31"), (32, "ccfree-tab-32")] {
        bridge::record_for_test(json!({
            "type": "event", "tabId": tab, "method": "Runtime.consoleAPICalled",
            "params": {"type": "log", "args": [{"value": text}]},
        }));
        bridge::record_for_test(json!({
            "type": "event", "tabId": tab, "method": "Network.responseReceived",
            "params": {"response": {"url": format!("https://{text}.invalid/"), "status": 200}},
        }));
    }

    let scope = event_scope(None, || Ok(31)).unwrap();
    let console = console_items(scope);
    let network = network_items("", scope);

    for items in [&console, &network] {
        let rendered = Value::from(items.clone()).to_string();
        assert!(rendered.contains("ccfree-tab-31"), "{rendered}");
        assert!(!rendered.contains("ccfree-tab-32"), "{rendered}");
        assert!(items.iter().all(|item| item["tab_id"] == 31), "{rendered}");
    }
}
