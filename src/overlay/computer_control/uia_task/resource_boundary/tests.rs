use super::*;

fn evidence(tool: &str) -> Value {
    json!({"tool_declaration":{"name":tool,"description":"selected capability","parameters":{}},
        "process":{"cwd":"C:/work"},"acting_model_context_untrusted":"context only"})
}

#[test]
fn every_opaque_argument_form_reaches_the_same_complete_resource_proposal() {
    let protected =
        json!([{"requested_path":"C:/work/result.csv","canonical_target":"C:/work/result.csv"}]);
    for args in [
        json!({"command":"Set-Content C:/work/result.csv changed"}),
        json!({"program":"future-editor","args":["result.csv"],"cwd":"C:/work"}),
        json!({"command":"Set-Content $env:TARGET changed"}),
        json!({"program":"future-editor","args":["RESULT~1.CSV"],"cwd":"C:/work"}),
        json!({"command":"Get-Content C:/work/result.csv"}),
    ] {
        let proposal = build_proposal(
            "run_command",
            &args,
            protected.clone(),
            evidence("run_command"),
        )
        .unwrap();
        assert_eq!(proposal["proposed_action"]["arguments"], args);
        assert_eq!(proposal["protected_resources"], protected);
        assert_eq!(proposal["evidence_untrusted"]["process"]["cwd"], "C:/work");
    }
}

#[test]
fn ui_browser_and_future_capabilities_receive_their_actual_declared_action() {
    for tool in ["act", "browser_eval", "future_capability"] {
        let args = json!({"opaque":{"id":27,"text":"context is not authority"}});
        let proposal = build_proposal(
            tool,
            &args,
            json!([{"requested_path":"C:/work/file"}]),
            evidence(tool),
        )
        .unwrap();
        assert_eq!(proposal["proposed_action"]["tool"], tool);
        assert_eq!(proposal["proposed_action"]["arguments"], args);
        assert_eq!(
            proposal["evidence_untrusted"]["tool_declaration"]["name"],
            tool
        );
    }
}

#[test]
fn absent_mismatched_or_oversized_effect_evidence_is_never_truncated_into_approval() {
    for evidence in [json!({}), evidence("different_tool")] {
        let rejected = build_proposal("run_command", &json!({}), json!([]), evidence).unwrap_err();
        assert_eq!(rejected["code"], "ERR_EXACT_EDIT_EFFECT_SCOPE_UNVERIFIED");
        assert_eq!(rejected["effect_may_have_occurred"], false);
    }
    let rejected = build_proposal(
        "run_command",
        &json!({"command":"x".repeat(MAX_PROPOSAL_BYTES)}),
        json!([]),
        evidence("run_command"),
    )
    .unwrap_err();
    assert_eq!(rejected["executed"], false);
}
