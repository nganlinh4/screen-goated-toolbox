use super::*;

fn browser_window() -> super::super::controller::world::BrowserWindowIdentity {
    super::super::controller::world::BrowserWindowIdentity {
        browser_window_id: 2,
        hwnd: 3,
        pid: 4,
        generation: 5,
    }
}

#[test]
fn new_turn_clears_task_local_recovery_state() {
    let mut brain = Brain::new(None);
    brain.begin_job(11, None);
    brain.trail.push("observe=ok".to_string());
    brain.completion_evidence.record(
        "future_query_tool",
        &json!({"ok": true, "result": [{"ordinal": 3, "metric": 71}]}),
        EvidenceProvenance::CapabilityResult,
    );
    brain.recent_actions.push("observe|{}".to_string());
    brain.advice_latches.push("observe|state".to_string());
    brain.prev_state_sig = Some("old surface".to_string());
    brain.wait_accum = 5.0;
    brain.last_click = Some((10, 20));
    brain.click_before = Some(vec![1, 2, 3]);
    brain.active_action = Some(super::super::telemetry::ActionTrace {
        action_id: 9,
        turn_id: 11,
    });
    brain.controlled_tab_id = Some(73);
    brain.controlled_document_id = Some("document-before".into());
    brain.next_anchor_id = 42;
    brain.zoomed = true;
    brain.whole_screen = true;
    brain.show_coarse_grid = true;
    brain.view = View {
        x: -10,
        y: -20,
        w: 30,
        h: 40,
    };
    brain.anchors.push(ClickAnchor {
        id: 41,
        x: 10,
        y: 20,
        note: None,
        verify_description: None,
        source: AnchorSource::Detector,
        score: None,
        bounds: None,
        frame_id: 1,
        view: brain.view,
        surface: (1, 1, 1),
    });

    brain.begin_job(11, None);
    assert_eq!(brain.trail, ["observe=ok"]);
    assert!(brain.completion_evidence.context().contains("ordinal"));
    assert_eq!(brain.anchors.len(), 1);
    assert!(brain.zoomed);
    assert!(brain.whole_screen);
    assert!(brain.show_coarse_grid);
    assert_eq!(brain.controlled_tab_id, Some(73));
    assert_eq!(
        brain.controlled_document_id.as_deref(),
        Some("document-before")
    );

    let expected_view = window_view(brain.target.as_deref(), false);
    brain.begin_job(12, None);
    assert_eq!(brain.current_turn_id, Some(12));
    assert!(brain.trail.is_empty());
    assert_eq!(brain.completion_evidence.context(), "none");
    assert!(brain.recent_actions.is_empty());
    assert!(brain.advice_latches.is_empty());
    assert!(brain.prev_state_sig.is_none());
    assert_eq!(brain.wait_accum, 0.0);
    assert!(brain.last_click.is_none());
    assert!(brain.click_before.is_none());
    assert!(brain.active_action.is_none());
    assert!(brain.controlled_tab_id.is_none());
    assert!(brain.controlled_document_id.is_none());
    assert!(brain.anchors.is_empty());
    assert_eq!(brain.next_anchor_id, 42);
    assert!(!brain.zoomed);
    assert!(!brain.whole_screen);
    assert!(!brain.show_coarse_grid);
    assert!(same_view(brain.view, expected_view));
}

#[test]
fn done_verifier_context_uses_only_its_bounded_evidence_ledger() {
    let mut brain = Brain::new(None);
    brain.begin_job(29, None);
    brain.trail.push("normal-context-only-marker".to_string());
    brain.completion_evidence.record(
        "future_query_tool",
        &json!({"ok": true, "result": [{"key": "verifier-ledger-marker"}]}),
        EvidenceProvenance::CapabilityResult,
    );

    let context = brain.done_verifier_context("opaque-goal-marker");
    assert!(context.contains("opaque-goal-marker"));
    assert!(context.contains("verifier-ledger-marker"));
    assert!(!context.contains("normal-context-only-marker"));
}

#[test]
fn direct_and_grounded_evidence_survive_advisory_churn() {
    let mut evidence = CompletionEvidence::default();
    for index in 0..5 {
        evidence.record(
            "future_capability",
            &json!({"ok": true, "direct_marker": format!("direct-{index}")}),
            EvidenceProvenance::CapabilityResult,
        );
    }
    evidence.record_grounded_surface(
        "grounded-marker",
        "https://example.invalid/grounded-marker",
        &super::super::controller::world::SurfaceIdentity::Browser {
            tab_id: 17,
            document_id: "grounded-document".into(),
            window: browser_window(),
        },
    );
    for index in 0..40 {
        let provenance = if index % 2 == 0 {
            EvidenceProvenance::ModelInference
        } else {
            EvidenceProvenance::ModelMediatedEffect
        };
        evidence.record(
            "future_advice",
            &json!({"ok": true, "advisory_marker": index}),
            provenance,
        );
    }

    let context = evidence.context();
    for index in 0..5 {
        assert!(context.contains(&format!("direct-{index}")));
    }
    assert!(context.contains("grounded-marker"));
    assert!(context.contains("grounded-document"));
}

#[test]
fn grounding_churn_preserves_direct_facts_and_recent_postconditions() {
    let mut evidence = CompletionEvidence::default();
    for index in 0..5 {
        evidence.record(
            "future_capability",
            &json!({"ok": true, "direct_marker": format!("direct-{index}")}),
            EvidenceProvenance::CapabilityResult,
        );
    }
    for index in 0..24 {
        evidence.record_grounded_surface(
            &format!("grounded-{index}"),
            &format!("https://example.invalid/grounded-{index}"),
            &super::super::controller::world::SurfaceIdentity::Browser {
                tab_id: 17,
                document_id: format!("document-{index}"),
                window: browser_window(),
            },
        );
    }

    let context = evidence.context();
    for index in 0..5 {
        assert!(context.contains(&format!("direct-{index}")));
    }
    assert!(context.contains("grounded-22"));
    assert!(context.contains("grounded-23"));
}

#[test]
fn model_authored_dispatch_keeps_lineage_and_receipt_but_not_claimed_output() {
    let mut evidence = CompletionEvidence::default();
    let request = json!({"command": "Write-Output fabricated-completion-marker"});
    let result = json!({
        "ok": true,
        "exit_code": 0,
        "stdout": "fabricated-completion-marker",
        "stderr": "",
    });
    evidence.record_dispatch(
        "run_command",
        &request,
        &result,
        EvidenceProvenance::for_dispatch("run_command"),
    );

    let context = evidence.context();
    assert!(context.contains("request_lineage"));
    assert!(context.contains("execution_receipt"));
    assert!(context.contains(r#""exit_code":0"#));
    assert!(!context.contains("fabricated-completion-marker"));
}

#[test]
fn new_turn_binds_browser_tools_to_the_source_frame_tab() {
    let mut brain = Brain::new(None);
    let source = FrameSource {
        frame_id: 81,
        surface: super::super::controller::world::SurfaceIdentity::Browser {
            tab_id: 37,
            document_id: "document-81".into(),
            window: browser_window(),
        },
    };

    brain.begin_job(44, Some(source.clone()));
    assert_eq!(brain.source_frame, Some(source));
    assert_eq!(brain.controlled_tab_id, Some(37));
    assert_eq!(brain.controlled_document_id.as_deref(), Some("document-81"));
    let source_evidence = brain.completion_evidence.context();
    assert!(source_evidence.contains(r#""provenance":"job_source""#));
    assert!(source_evidence.contains(r#""provenance":"provider_source""#));
    assert!(source_evidence.contains("document-81"));

    let native_source = FrameSource::native(82, (5, 6, 7));
    brain.begin_job(44, Some(native_source.clone()));
    assert_eq!(brain.source_frame, Some(native_source));
    assert_eq!(brain.controlled_tab_id, Some(37));
    assert_eq!(brain.controlled_document_id.as_deref(), Some("document-81"));
    let later_evidence = brain.completion_evidence.context();
    assert!(later_evidence.contains("document-81"));
    assert!(later_evidence.contains(r#""hwnd":5"#));
    assert_eq!(
        later_evidence
            .matches(r#""provenance":"provider_source""#)
            .count(),
        2
    );
}
