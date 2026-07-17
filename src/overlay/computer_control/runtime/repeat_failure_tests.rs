use super::*;

fn browser_window() -> crate::overlay::computer_control::controller::world::BrowserWindowIdentity {
    crate::overlay::computer_control::controller::world::BrowserWindowIdentity {
        browser_window_id: 2,
        hwnd: 3,
        pid: 4,
        generation: 5,
    }
}

fn process_result(stdout: &str) -> Value {
    json!({
        "ok": true,
        "process_completed": true,
        "effect_verified": false,
        "effect_may_have_occurred": true,
        "exit_code": 0,
        "stdout": stdout,
        "stderr": "",
    })
}

#[test]
fn equivalent_failure_allows_one_retry_then_blocks() {
    let mut guard = RepeatFailureGuard::default();
    let args = json!({"target": 7, "options": {"mode": "x"}});
    let failure = json!({"ok": false, "code": "ERR_TEMPORARY"});

    assert!(
        guard
            .blocked_result(4, "future_operation", &args, None)
            .is_none()
    );
    assert!(!guard.observe(4, "future_operation", &args, None, &failure));
    assert!(
        guard
            .blocked_result(4, "future_operation", &args, None)
            .is_none()
    );
    assert!(guard.observe(4, "future_operation", &args, None, &failure));
    assert_eq!(
        guard
            .blocked_result(4, "future_operation", &args, None)
            .unwrap()["code"],
        "ERR_EQUIVALENT_FAILURE_LIMIT"
    );
}

#[test]
fn stale_indexed_targets_share_one_surface_budget_across_ids_and_verbs() {
    let mut guard = RepeatFailureGuard::default();
    let surface = SurfaceIdentity::Browser {
        tab_id: 17,
        document_id: "document-a".to_string(),
        window: browser_window(),
    };
    let stale = json!({"ok": false, "code": "ERR_BROWSER_STALE_TARGET"});

    assert!(!guard.observe(
        4,
        "act",
        &json!({"id": 2, "verb": "click"}),
        Some(&surface),
        &stale
    ));
    assert!(guard.observe(
        4,
        "do_steps",
        &json!({"steps": [{"id": 19, "verb": "activate"}]}),
        Some(&surface),
        &stale
    ));
    let blocked = guard
        .blocked_result(
            4,
            "act",
            &json!({"id": 81, "verb": "click"}),
            Some(&surface),
        )
        .unwrap();
    assert_eq!(blocked["code"], "ERR_STALE_SURFACE_RETRY_LIMIT");
    assert!(
        guard
            .blocked_result(
                4,
                "click_target",
                &json!({"description": "current target"}),
                Some(&surface)
            )
            .is_none()
    );

    let changed = SurfaceIdentity::Browser {
        tab_id: 17,
        document_id: "document-b".to_string(),
        window: browser_window(),
    };
    assert!(
        guard
            .blocked_result(4, "act", &json!({"id": 1, "verb": "click"}), Some(&changed))
            .is_none()
    );
}

#[test]
fn identical_unverified_process_result_is_bounded() {
    let mut guard = RepeatFailureGuard::default();
    let args = json!({"program": "future-check", "args": ["verify"]});
    let result = process_result("");
    for reached in [false, false, true] {
        assert_eq!(
            guard.observe(12, "run_command", &args, None, &result),
            reached
        );
    }
    assert_eq!(
        guard
            .blocked_result(12, "run_command", &args, None)
            .unwrap()["code"],
        "ERR_EQUIVALENT_UNVERIFIED_RESULT_LIMIT"
    );
}

#[test]
fn changed_process_result_and_verified_progress_refresh_the_budget() {
    let mut guard = RepeatFailureGuard::default();
    let args = json!({"program": "future-check", "args": ["verify"]});
    guard.observe(13, "run_command", &args, None, &process_result("old"));
    guard.observe(13, "run_command", &args, None, &process_result("old"));
    assert!(!guard.observe(13, "run_command", &args, None, &process_result("new")));
    assert!(
        guard
            .blocked_result(13, "run_command", &args, None)
            .is_none()
    );
    guard.observe(13, "run_command", &args, None, &process_result("new"));
    assert!(guard.observe(13, "run_command", &args, None, &process_result("new")));
    assert!(guard.clear_after_verified_progress(13));
    assert!(
        guard
            .blocked_result(13, "run_command", &args, None)
            .is_none()
    );
}

#[test]
fn ordinary_success_is_not_treated_as_a_process_loop() {
    let mut guard = RepeatFailureGuard::default();
    let args = json!({"slot": 3});
    for _ in 0..8 {
        assert!(!guard.observe(
            2,
            "future_operation",
            &args,
            None,
            &json!({"ok": true, "value": "same"})
        ));
    }
    assert!(
        guard
            .blocked_result(2, "future_operation", &args, None)
            .is_none()
    );
}

#[test]
fn different_arguments_error_classes_and_turns_remain_available() {
    let mut guard = RepeatFailureGuard::default();
    let first = json!({"slot": 1});
    let second = json!({"slot": 2});
    let class_a = json!({"ok": false, "error": {"class": "A"}});
    let class_b = json!({"ok": false, "error": {"class": "B"}});

    guard.observe(9, "future_operation", &first, None, &class_a);
    assert!(guard.observe(9, "future_operation", &first, None, &class_a));
    assert!(
        guard
            .blocked_result(9, "future_operation", &second, None)
            .is_none()
    );
    assert!(!guard.observe(9, "future_operation", &second, None, &class_b));
    assert!(
        guard
            .blocked_result(10, "future_operation", &first, None)
            .is_none()
    );
    guard.observe(10, "future_operation", &first, None, &class_a);
    guard.observe(10, "future_operation", &first, None, &class_b);
    assert!(
        guard
            .blocked_result(10, "future_operation", &first, None)
            .is_none()
    );
}

#[test]
fn a_success_clears_the_equivalent_failure_streak() {
    let mut guard = RepeatFailureGuard::default();
    let args = json!({"slot": 3});
    let failure = json!({"ok": false, "error": "details are not retained"});
    guard.observe(2, "unknown_future_tool", &args, None, &failure);
    guard.observe(2, "unknown_future_tool", &args, None, &failure);
    assert!(
        guard
            .blocked_result(2, "unknown_future_tool", &args, None)
            .is_some()
    );

    assert!(!guard.observe(
        2,
        "unknown_future_tool",
        &args,
        None,
        &json!({"ok": true, "error": {"code": "advisory_metadata"}})
    ));
    assert!(
        guard
            .blocked_result(2, "unknown_future_tool", &args, None)
            .is_none()
    );
}

#[test]
fn a_verified_world_change_refreshes_failed_diagnostic_budget() {
    let mut guard = RepeatFailureGuard::default();
    let args = json!({"program": "future-check", "args": ["verify"]});
    let failure = json!({"ok": false, "code": "ERR_CHECK_FAILED"});
    guard.observe(2, "run_command", &args, None, &failure);
    guard.observe(2, "run_command", &args, None, &failure);
    assert!(
        guard
            .blocked_result(2, "run_command", &args, None)
            .is_some()
    );

    assert!(guard.clear_after_verified_progress(2));
    assert!(
        guard
            .blocked_result(2, "run_command", &args, None)
            .is_none()
    );
    assert!(!guard.clear_after_verified_progress(2));
}

#[test]
fn ambiguous_error_metadata_neither_counts_nor_clears() {
    let mut guard = RepeatFailureGuard::default();
    let args = json!({"slot": 4});
    let failure = json!({"ok": false, "code": "ERR_TYPED"});
    assert!(!guard.observe(3, "future_operation", &args, None, &failure));
    assert!(!guard.observe(
        3,
        "future_operation",
        &args,
        None,
        &json!({"error": {"code": "ERR_TYPED"}}),
    ));
    assert!(guard.observe(3, "future_operation", &args, None, &failure));

    let other = json!({"slot": 5});
    guard.observe(
        3,
        "future_operation",
        &other,
        None,
        &json!({"error": "unclassified"}),
    );
    guard.observe(
        3,
        "future_operation",
        &other,
        None,
        &json!({"error": "unclassified"}),
    );
    assert!(
        guard
            .blocked_result(3, "future_operation", &other, None)
            .is_none()
    );
}

#[test]
fn a_changed_document_or_window_generation_gets_a_fresh_retry_budget() {
    let mut guard = RepeatFailureGuard::default();
    let args = json!({"slot": 6});
    let failure = json!({"ok": false, "code": "ERR_STATE"});
    let first_document = SurfaceIdentity::Browser {
        tab_id: 17,
        document_id: "document-a".to_string(),
        window: browser_window(),
    };
    let next_document = SurfaceIdentity::Browser {
        tab_id: 17,
        document_id: "document-b".to_string(),
        window: browser_window(),
    };
    guard.observe(
        5,
        "future_operation",
        &args,
        Some(&first_document),
        &failure,
    );
    guard.observe(
        5,
        "future_operation",
        &args,
        Some(&first_document),
        &failure,
    );
    assert!(
        guard
            .blocked_result(5, "future_operation", &args, Some(&first_document))
            .is_some()
    );
    assert!(
        guard
            .blocked_result(5, "future_operation", &args, Some(&next_document))
            .is_none()
    );

    let first_window = SurfaceIdentity::Native {
        hwnd: 31,
        pid: 41,
        generation: 1,
    };
    let next_window = SurfaceIdentity::Native {
        hwnd: 31,
        pid: 41,
        generation: 2,
    };
    guard.observe(5, "future_operation", &args, Some(&first_window), &failure);
    guard.observe(5, "future_operation", &args, Some(&first_window), &failure);
    assert!(
        guard
            .blocked_result(5, "future_operation", &args, Some(&first_window))
            .is_some()
    );
    assert!(
        guard
            .blocked_result(5, "future_operation", &args, Some(&next_window))
            .is_none()
    );
}

#[test]
fn typed_retryability_is_part_of_the_failure_class() {
    let mut guard = RepeatFailureGuard::default();
    let args = json!({"slot": 7});
    let retryable = json!({"ok": false, "code": "ERR_STATE", "retryable": true});
    let final_failure = json!({"ok": false, "code": "ERR_STATE", "retryable": false});
    guard.observe(7, "future_operation", &args, None, &retryable);
    guard.observe(7, "future_operation", &args, None, &final_failure);
    assert!(
        guard
            .blocked_result(7, "future_operation", &args, None)
            .is_none()
    );
    assert!(guard.observe(7, "future_operation", &args, None, &retryable));
    assert!(
        guard
            .blocked_result(7, "future_operation", &args, None)
            .is_some()
    );
}

#[test]
fn fingerprints_do_not_retain_argument_or_result_content() {
    let mut guard = RepeatFailureGuard::default();
    let hidden_argument = "argument-content-must-not-survive";
    let hidden_class = "error-content-must-not-survive";
    let hidden_output = "process-output-must-not-survive";
    guard.observe(
        8,
        "future_operation",
        &json!({"value": hidden_argument}),
        None,
        &json!({"ok": false, "error": {"class": hidden_class}}),
    );
    guard.observe(
        8,
        "run_command",
        &json!({"slot": 9}),
        None,
        &process_result(hidden_output),
    );

    let debug = format!("{guard:?}");
    assert!(!debug.contains(hidden_argument));
    assert!(!debug.contains(hidden_class));
    assert!(!debug.contains(hidden_output));
}
