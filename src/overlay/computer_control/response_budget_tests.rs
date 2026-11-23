use super::*;

#[test]
fn small_responses_keep_exact_content_and_identity() {
    let original = json!({"ok": true, "value": "Hello 𠀀", "nested": [1, false]});
    let prepared = prepare("call-1", "observe", original.clone()).unwrap();
    assert!(!prepared.omitted);
    assert_eq!(prepared.delivered, original);
    assert_eq!(
        prepared.wire["toolResponse"]["functionResponses"][0]["id"],
        "call-1"
    );
    assert_eq!(
        prepared.wire["toolResponse"]["functionResponses"][0]["response"],
        original
    );
}

#[test]
fn oversized_escaped_content_is_omitted_with_conservative_effect_status() {
    let original = json!({
        "ok": true,
        "action_result": {"effect_verified": true, "effect_may_have_occurred": true},
        "content": "\u{0001}".repeat(MAX_WIRE_BYTES),
    });
    let prepared = prepare("id", "click", original).unwrap();
    assert!(prepared.omitted);
    assert_eq!(prepared.delivered["result_omitted"], true);
    assert_eq!(prepared.delivered["effect_status"], "may_have_occurred");
    assert_eq!(prepared.delivered["effect_verified"], false);
    assert_eq!(prepared.delivered["effect_may_have_occurred"], true);
    assert!(prepared.delivered.get("content").is_none());
    assert!(serde_json::to_vec(&prepared.wire).unwrap().len() <= MAX_WIRE_BYTES);
}

#[test]
fn uncertain_mutation_is_not_reported_as_no_effect() {
    let response = json!({"ok": false, "payload": "界".repeat(MAX_WIRE_BYTES)});
    let prepared = prepare("id", "click", response).unwrap();
    assert!(prepared.omitted);
    assert_eq!(prepared.delivered["effect_status"], "may_have_occurred");
    assert_eq!(prepared.delivered["effect_may_have_occurred"], true);
    assert_eq!(prepared.delivered["ok"], false);
}

#[test]
fn omitted_integration_payload_cannot_certify_its_own_effect() {
    let response = json!({
        "ok": true,
        "raw": {"effect_verified": true, "effect_may_have_occurred": false},
        "content": "x".repeat(MAX_WIRE_BYTES),
    });
    let prepared = prepare("id", "mcp__integration__operation", response).unwrap();
    assert!(prepared.omitted);
    assert_eq!(prepared.delivered["effect_status"], "may_have_occurred");
    assert_eq!(prepared.delivered["effect_verified"], false);
    assert_eq!(summary_effect(false), EffectStatus::Unknown);
}

#[test]
fn omitted_browser_payload_cannot_certify_its_own_effect() {
    let response = json!({
        "ok": true,
        "action_result": {"page": {"effect_verified": true, "executed": true}},
        "payload": "x".repeat(MAX_WIRE_BYTES),
    });
    let prepared = prepare("id", "browser_click", response).unwrap();
    assert!(prepared.omitted);
    assert_eq!(prepared.delivered["effect_status"], "may_have_occurred");
    assert_eq!(prepared.delivered["effect_verified"], false);
    assert_eq!(prepared.delivered["effect_may_have_occurred"], true);
}

#[test]
fn source_omission_stays_visible_and_blocks_complete_observation_status() {
    let response = json!({
        "ok": true,
        "action_result": {
            "content": "partial text",
            "raw": {"omitted": true, "effect_verified": true, "reason": "payload omitted"}
        }
    });
    let prepared = prepare("id", "mcp__integration__write", response).unwrap();
    assert!(prepared.omitted);
    assert_eq!(prepared.delivered["content"], "partial text");
    assert_eq!(prepared.delivered["source_omitted"], true);
    assert_eq!(prepared.delivered["effect_status"], "may_have_occurred");
    assert_eq!(prepared.delivered["effect_verified"], false);
}

#[test]
fn an_oversized_identity_fails_instead_of_changing_tool_call_identity() {
    let error = prepare(&"a".repeat(MAX_WIRE_BYTES), "observe", json!({"ok": true}));
    assert!(error.is_err());
}
