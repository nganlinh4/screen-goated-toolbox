use super::*;
#[path = "exact_edit_guard_tests/fixture.rs"]
mod fixture;
use fixture::{Fixture, receipt, record};

#[test]
fn retained_identity_works_with_the_actual_audited_text_transaction() {
    let fixture = Fixture::new();
    let target = fixture.args("result.csv");
    let mut guard = ExactEditGuard::default();
    record(
        &mut guard,
        "edit_text_file",
        &target,
        &mut json!({"ok":false}),
    );
    let edit = json!({"path":target["path"],"expected_sha256":receipt(&target,false)["sha256"],
        "replacements":[{"old_text":"Pending","new_text":"Ready","expected_count":1}]});
    assert!(guard.before_action("edit_text_file", &edit).is_none());
    let mut result = super::super::super::executor::execute_ex(
        "edit_text_file",
        &edit,
        &super::super::super::human_input::HumanProfile::instant(),
        &std::sync::atomic::AtomicBool::new(false),
    );
    assert_eq!(result["ok"], true, "actual transaction failed: {result}");
    guard.record_result("edit_text_file", &edit, &mut result);
    assert_eq!(result["resource_boundary_reconciled"], true);
    assert!(!guard.needs_scope_check("run_command"));
    assert_eq!(
        std::fs::read(fixture.path("result.csv")).unwrap(),
        b"Label,Ready\n"
    );
}

#[test]
fn case_alias_of_a_missing_destination_requires_native_policy_and_entry_proof() {
    let fixture = Fixture::new();
    let upper = json!({"path":fixture.path("PLANNED.csv")});
    let lower = json!({"path":fixture.path("planned.csv")});
    let mut guard = ExactEditGuard::default();
    record(
        &mut guard,
        "save_artifact",
        &upper,
        &mut json!({"ok":false}),
    );
    let known_insensitive = guard.protected_resources()[0]
        .pointer("/recorded_identity/entry/case_sensitive")
        .and_then(Value::as_bool)
        == Some(false);
    assert!(guard.before_action("save_artifact", &lower).is_none());
    fixture.file("planned.csv", b"Label,Created\n");
    let native_alias = Snapshot::capture(&fixture.path("PLANNED.csv"))
        .unwrap()
        .same_file(&Snapshot::capture(&fixture.path("planned.csv")).unwrap());
    guard.record_result("save_artifact", &lower, &mut receipt(&lower, true));
    assert_eq!(
        guard.needs_scope_check("run_command"),
        !(known_insensitive && native_alias)
    );
}

#[test]
fn hard_link_aliases_share_miss_budget_and_a_verified_read_reconciles_both() {
    let fixture = Fixture::new();
    let first = fixture.args("first.csv");
    let alias = fixture.link("first.csv", "alias.csv");
    let mut guard = ExactEditGuard::default();
    for args in [&first, &alias] {
        record(
            &mut guard,
            "edit_text_file",
            args,
            &mut json!({"ok":false,"code":"ERR_TEXT_FILE_MATCH_MISSING"}),
        );
    }
    for args in [&first, &alias] {
        assert_eq!(
            guard.before_action("edit_text_file", args).unwrap()["code"],
            "ERR_TEXT_FILE_REREAD_REQUIRED"
        );
    }
    record(
        &mut guard,
        "read_text_file",
        &alias,
        &mut receipt(&alias, false),
    );
    for args in [&first, &alias] {
        assert!(guard.before_action("edit_text_file", args).is_none());
    }
}

#[test]
fn hard_link_aliases_cannot_reset_the_structural_rejection_limit() {
    let fixture = Fixture::new();
    let first = fixture.args("first.csv");
    let alias = fixture.link("first.csv", "alias.csv");
    let mut guard = ExactEditGuard::default();
    record(
        &mut guard,
        "edit_text_file_structure",
        &first,
        &mut json!({"ok":false,"code":"ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED"}),
    );
    record(
        &mut guard,
        "edit_text_file",
        &alias,
        &mut json!({"ok":false,"code":"ERR_TEXT_FILE_STRUCTURE_CHANGE"}),
    );
    let mut last = json!({"ok":false,"code":"ERR_TEXT_FILE_FORMULA_MIXED_EDIT"});
    record(&mut guard, "edit_text_file", &first, &mut last);
    assert_eq!(last["code"], "ERR_TEXT_FILE_STRUCTURE_REJECTION_LIMIT");
}

#[test]
fn atomic_replacement_retires_only_the_selected_entry_not_its_old_hard_link() {
    let fixture = Fixture::new();
    let first = fixture.args("first.csv");
    let alias = fixture.link("first.csv", "alias.csv");
    let mut guard = ExactEditGuard::default();
    for args in [&first, &alias] {
        record(&mut guard, "edit_text_file", args, &mut json!({"ok":false}));
    }
    let before = Snapshot::capture(&fixture.path("first.csv")).unwrap();
    assert!(guard.before_action("edit_text_file", &first).is_none());
    let replacement = fixture.file("replacement.csv", b"Label,Ready\n");
    std::fs::rename(replacement, fixture.path("first.csv")).unwrap();
    let after = Snapshot::capture(&fixture.path("first.csv")).unwrap();
    assert!(before.same_entry(&after));
    assert!(!before.same_file(&after));
    let mut success = receipt(&first, true);
    success["atomic"] = json!(true);
    guard.record_result("edit_text_file", &first, &mut success);
    assert_eq!(success["resource_boundary_reconciled"], true);
    let resources = guard.protected_resources();
    assert_eq!(resources.as_array().unwrap().len(), 1);
    assert_eq!(resources[0]["requested_path"], alias["path"]);
    assert_eq!(
        std::fs::read(fixture.path("alias.csv")).unwrap(),
        b"Label,Pending\n"
    );
}

#[test]
fn verified_in_place_write_reconciles_only_current_aliases_of_that_object() {
    let fixture = Fixture::new();
    let first = fixture.args("first.csv");
    let alias = fixture.link("first.csv", "alias.csv");
    let unrelated = fixture.args("other.csv");
    let mut guard = ExactEditGuard::default();
    for args in [&first, &alias, &unrelated] {
        record(&mut guard, "save_artifact", args, &mut json!({"ok":false}));
    }
    assert!(guard.before_action("save_artifact", &alias).is_none());
    std::fs::write(fixture.path("alias.csv"), b"Label,Ready\n").unwrap();
    guard.record_result("save_artifact", &alias, &mut receipt(&alias, true));
    let resources = guard.protected_resources();
    assert_eq!(resources.as_array().unwrap().len(), 1);
    assert_eq!(resources[0]["requested_path"], unrelated["path"]);
}

#[test]
fn in_place_receipt_cannot_credit_a_replacement_file_object() {
    let fixture = Fixture::new();
    let args = fixture.args("result.csv");
    let mut guard = ExactEditGuard::default();
    record(&mut guard, "save_artifact", &args, &mut json!({"ok":false}));
    assert!(guard.before_action("save_artifact", &args).is_none());
    let replacement = fixture.file("replacement.csv", b"Label,Replacement\n");
    std::fs::rename(replacement, fixture.path("result.csv")).unwrap();
    let mut changed = receipt(&args, true);
    changed["atomic"] = json!(false);
    guard.record_result("save_artifact", &args, &mut changed);
    assert!(guard.needs_scope_check("run_command"));
}

#[test]
fn changed_bytes_or_absent_digest_cannot_credit_a_read_or_mutation() {
    let fixture = Fixture::new();
    let args = fixture.args("result.csv");
    let mut guard = ExactEditGuard::default();
    for _ in 0..2 {
        record(
            &mut guard,
            "edit_text_file",
            &args,
            &mut json!({"ok":false,"code":"ERR_TEXT_FILE_MATCH_MISSING"}),
        );
    }
    assert!(guard.before_action("read_text_file", &args).is_none());
    let mut stale_read = receipt(&args, false);
    std::fs::write(fixture.path("result.csv"), b"Label,Changed\n").unwrap();
    guard.record_result("read_text_file", &args, &mut stale_read);
    assert_eq!(stale_read["resource_boundary_reconciled"], false);
    assert!(guard.before_action("edit_text_file", &args).is_some());
    record(
        &mut guard,
        "read_text_file",
        &args,
        &mut receipt(&args, false),
    );
    assert!(guard.before_action("edit_text_file", &args).is_none());
    let mut unproven = json!({"ok":true,"effect_verified":true});
    guard.record_result("edit_text_file", &args, &mut unproven);
    assert_eq!(unproven["resource_boundary_reconciled"], false);
    assert!(guard.needs_scope_check("run_command"));
}

#[test]
fn changed_parent_identity_cannot_retire_the_original_entry() {
    let fixture = Fixture::new();
    fixture.directory("parent");
    let args = fixture.args("parent/result.csv");
    let mut guard = ExactEditGuard::default();
    record(
        &mut guard,
        "edit_text_file",
        &args,
        &mut json!({"ok":false}),
    );
    assert!(guard.before_action("edit_text_file", &args).is_none());
    fixture.directory("other");
    fixture.file("other/result.csv", b"Label,Other\n");
    assert!(
        !Snapshot::capture(&fixture.path("parent/result.csv"))
            .unwrap()
            .same_entry(&Snapshot::capture(&fixture.path("other/result.csv")).unwrap())
    );
    if let Err(error) = fixture.rename_directory("parent", "moved", &["result.csv"]) {
        assert!(
            matches!(error.raw_os_error(), Some(5 | 32 | 33)),
            "unexpected directory move error: {error}"
        );
        // Some native filesystems keep an open directory busy despite delete
        // sharing. Explicit retirement must release that ownership completely.
        guard.reset();
        fixture
            .rename_directory("parent", "moved", &["result.csv"])
            .unwrap();
    }
    fixture.directory("parent");
    fixture.file("parent/result.csv", b"Label,Replacement\n");
    if !guard.needs_scope_check("run_command") {
        record(
            &mut guard,
            "edit_text_file",
            &args,
            &mut json!({"ok":false}),
        );
    }
    guard.record_result("edit_text_file", &args, &mut receipt(&args, true));
    assert!(guard.needs_scope_check("run_command"));
}

#[test]
fn newly_created_file_keeps_its_previous_destination_entry_identity() {
    let fixture = Fixture::new();
    let missing = Snapshot::capture(&fixture.path("future.csv")).unwrap();
    fixture.file("future.csv", b"Label,Created\n");
    let existing = Snapshot::capture(&fixture.path("future.csv")).unwrap();
    assert!(missing.same_entry(&existing));
    assert!(!missing.same_file(&existing));
}

#[test]
fn only_the_matching_in_flight_action_can_credit_a_receipt() {
    let fixture = Fixture::new();
    let args = fixture.args("result.csv");
    let other = fixture.args("other.csv");
    let mut guard = ExactEditGuard::default();
    record(
        &mut guard,
        "edit_text_file",
        &args,
        &mut json!({"ok":false}),
    );
    assert!(guard.before_action("edit_text_file", &other).is_none());
    guard.record_result("edit_text_file", &args, &mut receipt(&args, true));
    assert!(guard.needs_scope_check("run_command"));
    guard.before_action("edit_text_file", &args);
    guard.before_action("observe", &json!({}));
    guard.record_result("edit_text_file", &args, &mut receipt(&args, true));
    assert!(guard.needs_scope_check("run_command"));
}

#[test]
fn rejected_resource_checks_every_alternate_mutation_without_inspecting_arguments() {
    let fixture = Fixture::new();
    let mut guard = ExactEditGuard::default();
    let edit = fixture.args("result.csv");
    record(
        &mut guard,
        "edit_text_file",
        &edit,
        &mut json!({"ok":false,"code":"ERR_PRECONDITION"}),
    );
    let contract: Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/phone-control/exact-resource-boundary.json"
    ))
    .unwrap();
    assert_eq!(contract["schemaVersion"], 1);
    for case in contract["cases"].as_array().unwrap() {
        let tool = case["tool"].as_str().unwrap();
        assert_eq!(
            guard.needs_scope_check(tool),
            case["needsAssessment"].as_bool().unwrap(),
            "{}",
            case["id"]
        );
        assert!(guard.before_action(tool, &case["arguments"]).is_none());
    }
    record(
        &mut guard,
        "edit_text_file",
        &edit,
        &mut json!({"ok":true,"effect_verified":true}),
    );
    assert!(!guard.needs_scope_check("run_command"));
}

#[test]
fn successful_exact_transaction_clears_only_its_own_resource_boundary() {
    let fixture = Fixture::new();
    let mut guard = ExactEditGuard::default();
    let first = fixture.args("first.csv");
    let second = fixture.args("second.csv");
    record(
        &mut guard,
        "edit_text_file",
        &first,
        &mut json!({"ok":false}),
    );
    record(
        &mut guard,
        "edit_text_file",
        &second,
        &mut json!({"ok":false}),
    );
    record(
        &mut guard,
        "edit_text_file",
        &first,
        &mut json!({"ok":true,"effect_verified":true}),
    );
    let resources = guard.protected_resources();
    assert_eq!(resources.as_array().unwrap().len(), 1);
    assert_eq!(resources[0]["requested_path"], second["path"]);
    assert!(guard.needs_scope_check("run_command"));
}

#[test]
fn turn_reset_releases_all_resource_boundaries() {
    let fixture = Fixture::new();
    let mut guard = ExactEditGuard::default();
    record(
        &mut guard,
        "edit_text_file",
        &fixture.args("result.csv"),
        &mut json!({"ok":false}),
    );
    guard.reset();
    assert!(!guard.needs_scope_check("future_mutation"));
    assert_eq!(guard.protected_resources(), json!([]));
}

#[test]
fn rejected_artifact_target_uses_the_same_alternate_effect_checkpoint() {
    let fixture = Fixture::new();
    let mut guard = ExactEditGuard::default();
    let args = fixture.args("report.txt");
    record(
        &mut guard,
        "save_artifact",
        &args,
        &mut json!({"ok":false,"code":"ERR_FILE_TARGET_REQUEST_CONTRACT_REJECTED"}),
    );
    assert!(guard.needs_scope_check("run_command"));
    assert_eq!(
        guard.protected_resources()[0]["last_rejection_code"],
        "ERR_FILE_TARGET_REQUEST_CONTRACT_REJECTED"
    );
}

#[test]
fn malformed_or_directory_target_cannot_create_a_file_resource_boundary() {
    for path in [
        "relative.csv".to_string(),
        "C:\\bad\0.txt".to_string(),
        std::env::temp_dir().to_str().unwrap().to_string(),
    ] {
        let mut guard = ExactEditGuard::default();
        record(
            &mut guard,
            "edit_text_file",
            &json!({"path":path}),
            &mut json!({"ok":false}),
        );
        assert!(!guard.needs_scope_check("run_command"));
    }
}

#[test]
fn contract_denial_bounds_changed_argument_retries_by_resource_and_effect() {
    let fixture = Fixture::new();
    let mut guard = ExactEditGuard::default();
    let args = fixture.args("result.csv");
    let mut denied = json!({
        "ok": false,
        "code": "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED",
        "effect_may_have_occurred": false,
    });
    record(&mut guard, "edit_text_file_structure", &args, &mut denied);
    assert!(denied.get("terminal_blocker").is_none());

    let mut first_retry = json!({
        "ok": false,
        "code": "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL",
        "effect_may_have_occurred": false,
    });
    record(&mut guard, "edit_text_file", &args, &mut first_retry);
    assert!(first_retry.get("terminal_blocker").is_none());

    let mut final_retry = json!({
        "ok": false,
        "code": "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL",
        "effect_may_have_occurred": false,
    });
    record(&mut guard, "edit_text_file", &args, &mut final_retry);
    assert_eq!(
        final_retry["code"],
        "ERR_TEXT_FILE_STRUCTURE_REJECTION_LIMIT"
    );
    assert_eq!(final_retry["terminal_blocker"], true);
    assert_eq!(final_retry["effect_may_have_occurred"], false);
}

#[test]
fn corrected_content_only_success_clears_the_structural_rejection_latch() {
    let fixture = Fixture::new();
    let mut guard = ExactEditGuard::default();
    let args = fixture.args("result.csv");
    let mut denied = json!({
        "ok": false,
        "code": "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED",
    });
    record(&mut guard, "edit_text_file_structure", &args, &mut denied);
    let mut retry = json!({
        "ok": false,
        "code": "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL",
    });
    record(&mut guard, "edit_text_file", &args, &mut retry);

    let mut corrected = json!({"ok": true, "effect_verified": true});
    record(&mut guard, "edit_text_file", &args, &mut corrected);
    let mut later_denial = json!({
        "ok": false,
        "code": "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED",
    });
    record(
        &mut guard,
        "edit_text_file_structure",
        &args,
        &mut later_denial,
    );
    assert!(later_denial.get("terminal_blocker").is_none());
}

#[test]
fn two_match_misses_require_a_fresh_exact_read_before_another_edit() {
    let fixture = Fixture::new();
    let mut guard = ExactEditGuard::default();
    let args = fixture.args("result.csv");
    for _ in 0..MATCH_MISS_REREAD_LIMIT {
        record(
            &mut guard,
            "edit_text_file",
            &args,
            &mut json!({"ok": false, "code": "ERR_TEXT_FILE_MATCH_MISSING"}),
        );
    }
    assert_eq!(
        guard.before_action("edit_text_file", &args).unwrap()["code"],
        "ERR_TEXT_FILE_REREAD_REQUIRED"
    );

    record(
        &mut guard,
        "read_text_file",
        &fixture.args("other.csv"),
        &mut json!({"ok": true}),
    );
    assert!(guard.before_action("edit_text_file", &args).is_some());

    record(
        &mut guard,
        "read_text_file",
        &args,
        &mut json!({"ok": true}),
    );
    assert!(guard.before_action("edit_text_file", &args).is_none());
}
