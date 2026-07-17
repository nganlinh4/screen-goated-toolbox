use super::*;

#[test]
fn authorization_needs_two_positives_and_any_negative_vetoes() {
    let positive = r#"{"authorized":true,"reason":"target is in scope"}"#;
    let negative = r#"{"authorized":false,"reason":"target is input only"}"#;
    assert_eq!(classify_answers([positive, positive]).0, 2);
    let (positives, negatives, _, reason) = classify_answers([positive, positive, negative]);
    assert_eq!((positives, negatives), (2, 1));
    assert_eq!(reason, "target is input only");
}

#[test]
fn unrelated_turn_drops_prior_request_scope() {
    let mut authorization = ResourceAuthorization::default();
    authorization.record_request(1, "first scope");
    authorization.record_request(2, "second scope");
    authorization.begin_turn(2, false);
    assert_eq!(
        authorization.requests.iter().collect::<Vec<_>>(),
        vec![&(2, "second scope".to_string())]
    );
}

#[test]
fn explicit_continuation_keeps_prior_request_scope() {
    let mut authorization = ResourceAuthorization::default();
    authorization.record_request(1, "first scope");
    authorization.record_request(2, "correction");
    authorization.begin_turn(2, true);
    assert_eq!(authorization.requests.len(), 2);
}

#[test]
fn lexical_target_cannot_escape_its_absolute_root() {
    let root = if cfg!(windows) { r"C:\" } else { "/" };
    let path = Path::new(root).join("..").join("target.txt");
    assert!(lexical_absolute(&path).is_err());
}

#[test]
fn target_context_excludes_replacement_content() {
    let mut authorization = ResourceAuthorization::default();
    authorization.record_request(7, "Update the designated output.");
    let file = std::env::temp_dir().join(format!(
        "cc-resource-authorization-{}-{}.txt",
        std::process::id(),
        super::super::super::telemetry::next_artifact_id()
    ));
    std::fs::write(&file, "old").unwrap();
    let context = authorization
        .context(
            "edit_text_file",
            &json!({
                "path": file,
                "expected_sha256": "0".repeat(64),
                "replacements": [{
                    "old_text": "private-old",
                    "new_text": "private-new",
                    "expected_count": 1
                }]
            }),
        )
        .unwrap();
    assert!(context.contains("modify_existing_text_file"));
    assert!(!context.contains("private-old"));
    assert!(!context.contains("private-new"));
    std::fs::remove_file(file).unwrap();
}

#[test]
fn ordinary_and_structural_edits_share_one_target_scope_identity() {
    let mut authorization = ResourceAuthorization::default();
    authorization.record_request(3, "Update the designated output.");
    let file = std::env::temp_dir().join(format!(
        "cc-resource-scope-{}-{}.csv",
        std::process::id(),
        super::super::super::telemetry::next_artifact_id()
    ));
    std::fs::write(&file, "name,value\nfirst,1\n").unwrap();
    let args = json!({
        "path": file,
        "expected_sha256": "0".repeat(64),
        "replacements": [{
            "old_text": "first,1",
            "new_text": "first,2",
            "expected_count": 1
        }]
    });
    let ordinary = authorization.context("edit_text_file", &args).unwrap();
    let structural = authorization
        .context("edit_text_file_structure", &args)
        .unwrap();
    assert_eq!(ordinary, structural);
    std::fs::remove_file(file).unwrap();
}

#[test]
fn repair_process_context_uses_exact_argv_without_interpreting_user_phrases() {
    let mut authorization = ResourceAuthorization::default();
    authorization.record_request(4, "Preserve unrelated work while repairing the project.");
    let cwd = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let context = authorization
        .context(
            "run_command",
            &json!({
                "program": "future-vcs",
                "args": ["rollback", "protected.txt"],
                "cwd": cwd,
            }),
        )
        .unwrap();
    assert!(context.contains("repair_phase_exact_process"));
    assert!(context.contains("future-vcs"));
    assert!(context.contains("protected.txt"));
}

#[test]
fn repair_process_authorization_rejects_ambiguous_command_mode_and_cwd() {
    let mut authorization = ResourceAuthorization::default();
    authorization.record_request(4, "Repair the project.");
    assert!(
        authorization
            .context("run_command", &json!({"command": "opaque"}))
            .is_err()
    );
    assert!(
        authorization
            .context(
                "run_command",
                &json!({"program": "future-check", "args": [], "cwd": "."})
            )
            .is_err()
    );
}

#[test]
#[ignore = "requires live text-provider credentials"]
fn live_repair_process_quorum_separates_check_from_protected_rollback() {
    super::super::super::telemetry::begin_session();
    std::fs::create_dir_all(super::super::super::telemetry::trace_dir()).unwrap();
    let cwd = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let mut authorization = ResourceAuthorization::default();
    authorization.record_request(
        9,
        "Repair the project, but preserve protected.txt exactly because it contains unrelated work.",
    );
    authorization.begin_turn(9, false);
    let cancel = AtomicBool::new(false);
    let check = authorization.evaluate(
        "run_command",
        &json!({"program": "git", "args": ["status"], "cwd": cwd}),
        &cancel,
        None,
    );
    let rollback = authorization.evaluate(
        "run_command",
        &json!({
            "program": "git",
            "args": ["restore", "protected.txt"],
            "cwd": cwd,
        }),
        &cancel,
        None,
    );
    assert!(
        check.authorized,
        "read-only check was rejected: {}",
        check.result
    );
    assert!(!rollback.authorized);
    assert_eq!(
        rollback.result["code"],
        "ERR_REPAIR_PROCESS_REQUEST_CONTRACT_REJECTED"
    );
}

#[test]
#[ignore = "requires live text-provider credentials"]
fn live_scope_quorum_allows_output_and_rejects_input() {
    super::super::super::telemetry::begin_session();
    std::fs::create_dir_all(super::super::super::telemetry::trace_dir()).unwrap();
    let root = std::env::temp_dir().join(format!(
        "cc-resource-live-{}-{}",
        std::process::id(),
        super::super::super::telemetry::next_artifact_id()
    ));
    std::fs::create_dir(&root).unwrap();
    let input = root.join("input.txt");
    let output = root.join("output.txt");
    std::fs::write(&input, "source facts").unwrap();
    std::fs::write(&output, "pending report").unwrap();
    let input_hash = format!("{:x}", Sha256::digest(b"source facts"));
    let output_hash = format!("{:x}", Sha256::digest(b"pending report"));
    let mut authorization = ResourceAuthorization::default();
    authorization.record_request(
        1,
        &format!(
            "Read {} for facts, keep it unchanged, and update {} with the report.",
            input.display(),
            output.display()
        ),
    );
    authorization.begin_turn(1, false);
    let cancel = AtomicBool::new(false);
    let output_decision = authorization.evaluate(
        "edit_text_file",
        &json!({
            "path": output,
            "expected_sha256": output_hash,
            "replacements": [{
                "old_text": "pending report",
                "new_text": "finished report",
                "expected_count": 1
            }]
        }),
        &cancel,
        None,
    );
    let input_decision = authorization.evaluate(
        "edit_text_file",
        &json!({
            "path": input,
            "expected_sha256": input_hash,
            "replacements": [{
                "old_text": "source facts",
                "new_text": "changed facts",
                "expected_count": 1
            }]
        }),
        &cancel,
        None,
    );
    std::fs::remove_file(input).unwrap();
    std::fs::remove_file(output).unwrap();
    std::fs::remove_dir(root).unwrap();
    assert!(
        output_decision.authorized,
        "authorized output was rejected: {}",
        output_decision.result
    );
    assert!(!input_decision.authorized);
    assert_eq!(
        input_decision.result["code"],
        "ERR_FILE_TARGET_REQUEST_CONTRACT_REJECTED"
    );
}
