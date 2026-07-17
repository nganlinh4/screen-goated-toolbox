use super::*;

#[test]
fn route_uses_exact_tab_only_when_turn_has_a_pin() {
    assert_eq!(managed_open::tab_route(None), TabRoute::Current);
    assert_eq!(managed_open::tab_route(Some(310)), TabRoute::Exact(310));
    assert_eq!(
        managed_open::default_lifetime(),
        super::super::tab_ownership::TabLifetime::Persistent
    );
}

#[test]
fn managed_open_rejects_non_web_targets_before_dispatch() {
    for args in [
        json!({}),
        json!({"url": ""}),
        json!({"url": "relative/path"}),
        json!({"url": "C:\\local\\item.txt"}),
        json!({"url": "file:///C:/local/item.txt"}),
    ] {
        let error = managed_open::http_url(&args).unwrap_err();
        assert_eq!(error["code"], "ERR_OPEN_URL_INVALID");
        assert_eq!(error["effect_may_have_occurred"], false);
        assert_eq!(error["executed"], false);
    }
    assert_eq!(
        managed_open::http_url(&json!({"url": "https://example.invalid/path"})).unwrap(),
        "https://example.invalid/path"
    );
}

#[test]
fn turn_navigation_never_mutates_a_borrowed_or_ambient_tab() {
    assert_eq!(
        navigation_plan(
            super::super::tab_ownership::TabLifetime::Turn,
            TabRoute::Current,
            false,
        ),
        NavigationPlan::CreateTurnTab
    );
    assert_eq!(
        navigation_plan(
            super::super::tab_ownership::TabLifetime::Turn,
            TabRoute::Exact(44),
            false,
        ),
        NavigationPlan::CreateTurnTab
    );
    assert_eq!(
        navigation_plan(
            super::super::tab_ownership::TabLifetime::Turn,
            TabRoute::Exact(45),
            true,
        ),
        NavigationPlan::Navigate {
            route: TabRoute::Exact(45),
            promote_owned_lease: false,
        }
    );
}

#[test]
fn persistent_navigation_preserves_target_and_promotes_only_owned_exact_lease() {
    assert_eq!(
        navigation_plan(
            super::super::tab_ownership::TabLifetime::Persistent,
            TabRoute::Current,
            false,
        ),
        NavigationPlan::Navigate {
            route: TabRoute::Current,
            promote_owned_lease: false,
        }
    );
    assert_eq!(
        navigation_plan(
            super::super::tab_ownership::TabLifetime::Persistent,
            TabRoute::Exact(51),
            true,
        ),
        NavigationPlan::Navigate {
            route: TabRoute::Exact(51),
            promote_owned_lease: true,
        }
    );
}

#[test]
fn lease_promotion_follows_dispatch_evidence_not_a_failed_request_alone() {
    assert!(!navigation_may_have_effect(&json!({
        "ok": false,
        "executed": false,
        "effect_may_have_occurred": false,
    })));
    assert!(!navigation_may_have_effect(&json!({
        "ok": false,
        "dispatch": {"status": "not_attempted"},
    })));
    assert!(navigation_may_have_effect(&json!({
        "ok": false,
        "dispatch": {"status": "accepted"},
    })));
    assert!(navigation_may_have_effect(&json!({
        "ok": false,
        "dispatch": {"status": "unknown", "effect_may_have_occurred": true},
    })));
}

#[test]
fn successful_close_clears_only_the_matching_pin() {
    assert_eq!(pin_after_close(Some(7), 7, true), None);
    assert_eq!(pin_after_close(Some(7), 8, true), Some(7));
    assert_eq!(pin_after_close(Some(7), 7, false), Some(7));
}

#[test]
fn tab_create_failure_before_dispatch_is_a_proven_non_effect() {
    let failure = tab_open_error(anyhow::anyhow!("preflight unavailable"));
    assert_eq!(failure["effect_verified"], false);
    assert_eq!(failure["effect_may_have_occurred"], false);
    assert_eq!(failure["executed"], false);
}

#[test]
fn exact_page_tools_fail_closed_when_the_source_document_drifted() {
    let drift = validate_document_route(
        TabRoute::Exact(12),
        Some("doc-before"),
        |tab_id, document_id| {
            assert_eq!(tab_id, 12);
            assert_eq!(document_id, "doc-before");
            anyhow::bail!("document changed")
        },
    )
    .expect("drift must produce a typed failure");

    assert_eq!(drift["code"], "ERR_STALE_FRAME_SURFACE");
    assert_eq!(drift["target_tab_id"], 12);
    assert_eq!(drift["expected_document_id"], "doc-before");
    assert_eq!(drift["effect_may_have_occurred"], false);
    assert_eq!(drift["executed"], false);
}

#[test]
fn successful_tab_effect_is_not_reported_without_an_exact_document_binding() {
    let (bound, document_id) =
        bind_successful_document(json!({"ok": true, "target_tab_id": 12}), 12, |_| {
            Ok(
                crate::overlay::computer_control::browser::StableDocumentIdentity {
                    document_id: "doc-after".to_string(),
                    loader_id: "loader-after".to_string(),
                    url: "https://example.invalid/after".to_string(),
                },
            )
        });
    assert_eq!(bound["document_id"], "doc-after");
    assert_eq!(bound["document_binding"]["loader_id"], "loader-after");
    assert_eq!(bound["document_binding"]["stable"], true);
    assert_eq!(document_id.as_deref(), Some("doc-after"));

    let (failed, document_id) =
        bind_successful_document(json!({"ok": true, "target_tab_id": 12}), 12, |_| {
            anyhow::bail!("execution context unavailable")
        });
    assert_eq!(failed["code"], "ERR_BROWSER_DOCUMENT_BINDING_UNAVAILABLE");
    assert_eq!(failed["effect"]["ok"], true);
    assert!(document_id.is_none());

    let preflight = document_binding_preflight_failure(12, "still loading");
    assert_eq!(
        preflight["code"],
        "ERR_BROWSER_DOCUMENT_BINDING_UNAVAILABLE"
    );
    assert_eq!(preflight["effect_may_have_occurred"], false);
    assert_eq!(preflight["executed"], false);
}

#[test]
fn legacy_blank_tab_response_requires_a_settled_document() {
    assert!(switch_requires_settled_document(&json!({
        "ok": true,
        "url": null
    })));
    assert!(switch_requires_settled_document(&json!({
        "ok": true,
        "url": "about:blank",
        "pending_url": null
    })));
    assert!(switch_requires_settled_document(&json!({
        "ok": true,
        "url": "https://old.invalid/",
        "pending_url": "https://new.invalid/"
    })));
    assert!(!switch_requires_settled_document(&json!({
        "ok": true,
        "url": "https://settled.invalid/",
        "pending_url": null
    })));
}

#[test]
fn explicit_current_route_does_not_invent_a_source_document() {
    let result = validate_document_route(TabRoute::Current, None, |_, _| {
        panic!("current route must not run an exact-document validator")
    });
    assert!(result.is_none());
    assert!(is_document_bound_tool("browser_read_page"));
    assert!(!is_document_bound_tool("browser_navigate"));
    assert!(!is_document_bound_tool("browser_history"));
    assert!(!is_document_bound_tool("browser_switch_tab"));
}

#[test]
fn observational_result_is_rejected_if_navigation_wins_the_call_race() {
    let result = postcheck_document_route(
        TabRoute::Exact(22),
        Some("doc-before"),
        |_, _| anyhow::bail!("document changed after read"),
        json!({"ok": true, "page": {"text": "wrong document"}}),
    );

    assert_eq!(result["code"], "ERR_STALE_FRAME_SURFACE");
    assert!(!result.to_string().contains("wrong document"));
    assert!(requires_document_postcheck("browser_read_page"));
    assert!(!requires_document_postcheck("browser_navigate"));
}

#[test]
fn only_observations_retire_a_stale_document_binding() {
    let mut read = json!({"code": "ERR_STALE_FRAME_SURFACE"});
    assert!(retire_refreshable_stale_binding(
        "browser_read_page",
        &mut read
    ));
    assert_eq!(read["document_binding_retired"], true);
    let mut eval = json!({"code": "ERR_STALE_FRAME_SURFACE"});
    assert!(!retire_refreshable_stale_binding("browser_eval", &mut eval));
}

#[test]
fn only_connection_management_tools_bypass_readiness_preflight() {
    for name in ["browser_setup", "browser_status", "browser_reset"] {
        assert_eq!(connection_requirement(name), Some(false));
    }
    for name in [
        "browser_read_page",
        "research_web",
        "browser_navigate",
        "browser_history",
        "browser_tabs",
        "browser_switch_tab",
        "browser_close_tab",
    ] {
        assert_eq!(connection_requirement(name), Some(true));
    }
    assert_eq!(connection_requirement("future_capability"), None);
}
