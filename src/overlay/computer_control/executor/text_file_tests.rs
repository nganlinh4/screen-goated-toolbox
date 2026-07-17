use super::*;
use crate::overlay::computer_control::effect_receipt::EffectStatus;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

struct Fixture {
    dir: PathBuf,
    path: PathBuf,
}

impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        Self::named("fixture.txt", bytes)
    }

    fn named(name: &str, bytes: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let dir = std::env::temp_dir().join(format!(
            "sgt-text-file-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        Self { dir, path }
    }

    fn hash(&self) -> String {
        sha256_hex(&fs::read(&self.path).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn edit(fixture: &Fixture, replacements: Value) -> Value {
    edit_text_file(&json!({
        "path": fixture.path,
        "expected_sha256": fixture.hash(),
        "replacements": replacements,
    }))
}

#[test]
fn reads_bounded_content_with_exact_metadata() {
    let fixture = Fixture::new(b"alpha\r\nbeta\r\n");
    let result = read_text_file(&json!({"path": fixture.path, "max_chars": 5}));
    assert_eq!(result["ok"], true);
    assert_eq!(result["content"], "alpha");
    assert_eq!(result["byte_count"], 13);
    assert_eq!(result["char_count"], 13);
    assert_eq!(result["line_count"], 2);
    assert_eq!(result["encoding"], "utf-8");
    assert_eq!(result["truncated"], true);
    assert_eq!(result["completion_proof"]["partial"], json!(["/content"]));
    assert_eq!(result["sha256"], fixture.hash());
}

#[test]
fn local_text_paths_must_be_absolute_before_read_or_edit() {
    let relative = PathBuf::from("working-directory-file.txt");
    let read = read_text_file(&json!({"path": relative}));
    assert_eq!(read["code"], "ERR_TEXT_FILE_PATH_NOT_ABSOLUTE");
    assert_eq!(read["original_unchanged"], true);

    let edit = edit_text_file(&json!({
        "path": "working-directory-file.txt",
        "expected_sha256": "0".repeat(64),
        "replacements": [{
            "old_text": "old",
            "new_text": "new",
            "expected_count": 1
        }]
    }));
    assert_eq!(edit["code"], "ERR_TEXT_FILE_PATH_NOT_ABSOLUTE");
    assert_eq!(edit["original_unchanged"], true);
}

#[test]
fn exact_edit_preserves_bom_crlf_and_shell_metacharacters() {
    let mut bytes = UTF8_BOM.to_vec();
    bytes.extend_from_slice(b"cost=$39.88\r\ncode=`literal`; braces={x}\r\nkeep\r\n");
    let fixture = Fixture::new(&bytes);
    let result = edit(
        &fixture,
        json!([{
            "old_text": "cost=$39.88\r\ncode=`literal`; braces={x}",
            "new_text": "cost=$59.99\r\ncode=`still-literal`; braces={y}",
            "expected_count": 1
        }]),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["effect_verified"], true);
    let actual = fs::read(&fixture.path).unwrap();
    assert!(actual.starts_with(UTF8_BOM));
    assert_eq!(
        &actual[UTF8_BOM.len()..],
        b"cost=$59.99\r\ncode=`still-literal`; braces={y}\r\nkeep\r\n"
    );
}

#[test]
fn multiple_replacements_commit_as_one_verified_effect() {
    let fixture = Fixture::new(b"one two one\nuntouched\nthree\n");
    let result = edit(
        &fixture,
        json!([
            {"old_text": "one", "new_text": "1", "expected_count": 2},
            {"old_text": "three", "new_text": "3", "expected_count": 1}
        ]),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["replacements_applied"], 3);
    assert_eq!(fs::read(&fixture.path).unwrap(), b"1 two 1\nuntouched\n3\n");
}

#[test]
fn delimited_edit_rejects_lost_field_and_formula_before_writing() {
    let original = b"name,value,eligible\r\nalpha,1,\"=A2>0\"\r\n";
    let fixture = Fixture::named("fixture.csv", original);
    let result = edit(
        &fixture,
        json!([{
            "old_text": "alpha,1,\"=A2>0\"",
            "new_text": "alpha,2",
            "expected_count": 1
        }]),
    );
    assert_eq!(
        result["code"], "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL",
        "{result}"
    );
    assert_eq!(result["before_formula_count"], 1);
    assert_eq!(result["after_formula_count"], 0);
    assert_eq!(result["original_unchanged"], true);
    assert_eq!(fs::read(&fixture.path).unwrap(), original);
}

#[test]
fn delimited_edit_preserves_shape_and_formula_evidence() {
    let fixture = Fixture::named("fixture.csv", b"name,value,eligible\nalpha,1,\"=A2>0\"\n");
    let result = edit(
        &fixture,
        json!([{"old_text": "alpha,1,", "new_text": "alpha,2,", "expected_count": 1}]),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["structure"]["record_shape_preserved"], true);
    assert_eq!(result["structure"]["formulas_preserved"], true);
    assert_eq!(result["structure"]["structural_change_confirmed"], false);
}

#[test]
fn ordinary_row_update_auto_preserves_opaque_formula_bytes() {
    let original = b"name,status,total\r\nalpha,Unknown,\"=B2*12\"\r\n";
    let fixture = Fixture::named("fixture.csv", original);
    let result = edit(
        &fixture,
        json!([{
            "old_text": "alpha,Unknown,\"=B2*12\"",
            "new_text": "alpha,Ready,\"=99\"",
            "expected_count": 1
        }]),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["formula_cells_auto_preserved"], 1);
    assert_eq!(result["formula_replacement_groups_rewritten"], 1);
    assert_eq!(result["structure"]["formulas_preserved"], true);
    assert_eq!(
        fs::read(&fixture.path).unwrap(),
        b"name,status,total\r\nalpha,Ready,\"=B2*12\"\r\n"
    );
}

#[test]
fn ordinary_row_update_repairs_an_unquoted_unchanged_formula_tail() {
    let original = b"name,status,eligible\r\nalpha,Unknown,\"=AND(B2>0,C2=\"\"Yes\"\")\"\r\n";
    let fixture = Fixture::named("fixture.csv", original);
    let result = edit(
        &fixture,
        json!([{
            "old_text": "alpha,Unknown,\"=AND(B2>0,C2=\"\"Yes\"\")\"",
            "new_text": "alpha,Ready,=AND(B2>0,C2=\"Yes\")",
            "expected_count": 1
        }]),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["formula_cells_auto_preserved"], 1);
    assert_eq!(result["formula_replacement_groups_rewritten"], 1);
    assert_eq!(result["structure"]["record_shape_preserved"], true);
    assert_eq!(result["structure"]["formulas_preserved"], true);
    assert_eq!(
        fs::read(&fixture.path).unwrap(),
        b"name,status,eligible\r\nalpha,Ready,\"=AND(B2>0,C2=\"\"Yes\"\")\"\r\n"
    );
}

#[test]
fn ordinary_data_row_drops_only_redundant_trailing_empty_fields() {
    let original = b"Label,Pending\r\n";
    let fixture = Fixture::named("fixture.csv", original);
    let result = edit(
        &fixture,
        json!([{
            "old_text": "Label,Pending",
            "new_text": "Label,Ready,,",
            "expected_count": 1
        }]),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["trailing_empty_fields_omitted"], 2);
    assert_eq!(result["structure"]["record_shape_preserved"], true);
    assert_eq!(fs::read(&fixture.path).unwrap(), b"Label,Ready\r\n");
}

#[test]
fn ordinary_data_row_serializes_commas_in_the_changed_trailing_value() {
    let original = b"Field,Value\r\nRationale,Unknown\r\n";
    let fixture = Fixture::named("fixture.csv", original);
    let result = edit(
        &fixture,
        json!([{
            "old_text": "Rationale,Unknown",
            "new_text": "Rationale,Supports families, shared vaults, and recovery",
            "expected_count": 1
        }]),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["trailing_value_fields_repaired"], 2);
    assert_eq!(result["structure"]["record_shape_preserved"], true);
    assert_eq!(
        fs::read(&fixture.path).unwrap(),
        b"Field,Value\r\nRationale,\"Supports families, shared vaults, and recovery\"\r\n"
    );
}

#[test]
fn ordinary_data_row_does_not_fold_a_preserved_last_field_into_a_new_column() {
    let original = b"Field,Value\r\nRationale,Unknown\r\n";
    let fixture = Fixture::named("fixture.csv", original);
    let result = edit(
        &fixture,
        json!([{
            "old_text": "Rationale,Unknown",
            "new_text": "Rationale,Unknown,extra",
            "expected_count": 1
        }]),
    );
    assert_eq!(
        result["code"], "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL",
        "{result}"
    );
    assert_eq!(fs::read(&fixture.path).unwrap(), original);
}

#[test]
fn ordinary_multi_row_edit_repairs_formula_quoting_and_empty_suffixes_atomically() {
    let original =
        b"name,status,eligible\r\nalpha,Unknown,\"=AND(B2>0,C2=\"\"Yes\"\")\"\r\nLabel,Pending\r\n";
    let fixture = Fixture::named("fixture.csv", original);
    let result = edit(
        &fixture,
        json!([
            {
                "old_text": "alpha,Unknown,\"=AND(B2>0,C2=\"\"Yes\"\")\"",
                "new_text": "alpha,Ready,=AND(B2>0,C2=\"Yes\")",
                "expected_count": 1
            },
            {
                "old_text": "Label,Pending",
                "new_text": "Label,Ready,,",
                "expected_count": 1
            }
        ]),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["formula_cells_auto_preserved"], 1);
    assert_eq!(result["trailing_empty_fields_omitted"], 2);
    assert_eq!(result["structure"]["record_shape_preserved"], true);
    assert_eq!(result["structure"]["formulas_preserved"], true);
    assert_eq!(
        fs::read(&fixture.path).unwrap(),
        b"name,status,eligible\r\nalpha,Ready,\"=AND(B2>0,C2=\"\"Yes\"\")\"\r\nLabel,Ready\r\n"
    );
}

#[test]
fn ordinary_table_work_omits_a_separate_formula_replacement_group() {
    let original = b"name\tstatus\ttotal\nalpha\tUnknown\t=B2*12\n";
    let fixture = Fixture::named("fixture.tsv", original);
    let result = edit(
        &fixture,
        json!([
            {"old_text": "Unknown", "new_text": "Ready", "expected_count": 1},
            {"old_text": "=B2*12", "new_text": "=99", "expected_count": 1}
        ]),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["requested_replacement_groups"], 2);
    assert_eq!(result["replacement_groups"], 1);
    assert_eq!(result["formula_only_groups_omitted"], 1);
    assert_eq!(result["formula_cells_auto_preserved"], 1);
    assert_eq!(
        fs::read(&fixture.path).unwrap(),
        b"name\tstatus\ttotal\nalpha\tReady\t=B2*12\n"
    );
}

#[test]
fn ambiguous_text_shared_by_data_and_formula_still_fails_closed() {
    let original = b"name,status,rule\nalpha,Unknown,\"=IF(A1=\"\"Unknown\"\",1,0)\"\n";
    let fixture = Fixture::named("fixture.csv", original);
    let result = edit(
        &fixture,
        json!([{"old_text": "Unknown", "new_text": "Ready", "expected_count": 2}]),
    );
    assert_eq!(
        result["code"], "ERR_TEXT_FILE_FORMULA_MIXED_EDIT",
        "{result}"
    );
    assert_eq!(result["original_unchanged"], true);
    assert_eq!(fs::read(&fixture.path).unwrap(), original);
}

#[test]
fn intentional_delimited_structure_change_requires_separate_preflight() {
    let fixture = Fixture::named("fixture.tsv", b"name\tvalue\nalpha\t1\n");
    let args = json!({
        "path": fixture.path,
        "expected_sha256": fixture.hash(),
        "replacements": [{
            "old_text": "alpha\t1",
            "new_text": "alpha\t1\textra",
            "expected_count": 1
        }]
    });
    let ordinary_rejected = edit_text_file(&args);
    assert_eq!(
        ordinary_rejected["code"],
        "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL"
    );
    assert!(ordinary_rejected.get("structural_change_token").is_none());
    assert_eq!(fs::read(&fixture.path).unwrap(), b"name\tvalue\nalpha\t1\n");

    let rejected = edit_text_file_structure(&args);
    assert_eq!(rejected["code"], "ERR_TEXT_FILE_STRUCTURE_CHANGE");
    assert_eq!(rejected["original_unchanged"], true);
    let token = rejected["structural_change_token"].as_str().unwrap();
    let mut confirmed_args = args;
    confirmed_args["structural_change_token"] = json!(token);
    let still_preflight = edit_text_file_structure(&confirmed_args);
    assert_eq!(
        still_preflight["code"], "ERR_TEXT_FILE_STRUCTURE_CHANGE",
        "{still_preflight}"
    );
    assert_eq!(still_preflight["original_unchanged"], true);
    assert_eq!(fs::read(&fixture.path).unwrap(), b"name\tvalue\nalpha\t1\n");

    let result = commit_text_file_structure(&confirmed_args);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["edit_scope"], "structure");
    assert_eq!(result["structure"]["record_shape_preserved"], false);
    assert_eq!(result["structure"]["structural_change_confirmed"], true);
}

#[test]
fn ordinary_formula_only_retry_never_receives_executable_authorization() {
    let original = b"name,value,total\nalpha,1,=B2*12\n";
    let fixture = Fixture::named("fixture.csv", original);
    let args = json!({
        "path": fixture.path,
        "expected_sha256": fixture.hash(),
        "replacements": [{
            "old_text": "=B2*12",
            "new_text": "=99",
            "expected_count": 1
        }]
    });
    let result = edit_text_file(&args);
    assert_eq!(
        result["code"],
        "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL"
    );
    assert!(result.get("structural_change_token").is_none());
    assert_eq!(fs::read(&fixture.path).unwrap(), original);

    let mut smuggled = args;
    smuggled["structural_change_token"] = json!("invented");
    let result = edit_text_file(&smuggled);
    assert_eq!(result["code"], "ERR_TEXT_FILE_BAD_ARGUMENT");
    assert_eq!(fs::read(&fixture.path).unwrap(), original);
}

#[test]
fn explicit_structure_tool_rejects_shape_preserving_data_work() {
    let original = b"name,value,total\nalpha,1,=B2*12\n";
    let fixture = Fixture::named("fixture.csv", original);
    let result = edit_text_file_structure(&json!({
        "path": fixture.path,
        "expected_sha256": fixture.hash(),
        "replacements": [{"old_text": "alpha,1,", "new_text": "alpha,2,", "expected_count": 1}]
    }));
    assert_eq!(result["code"], "ERR_TEXT_FILE_STRUCTURE_NOT_CHANGED");
    assert!(result.get("structural_change_token").is_none());
    assert_eq!(fs::read(&fixture.path).unwrap(), original);
}

#[test]
fn explicit_structure_tool_cannot_bypass_normal_text_editing() {
    let original = b"before\n";
    let fixture = Fixture::named("fixture.txt", original);
    let result = edit_text_file_structure(&json!({
        "path": fixture.path,
        "expected_sha256": fixture.hash(),
        "replacements": [{"old_text": "before", "new_text": "after", "expected_count": 1}]
    }));
    assert_eq!(result["code"], "ERR_TEXT_FILE_STRUCTURE_UNSUPPORTED");
    assert_eq!(fs::read(&fixture.path).unwrap(), original);
}

#[test]
fn stale_hash_leaves_original_unchanged() {
    let fixture = Fixture::new(b"before");
    let before = fs::read(&fixture.path).unwrap();
    let result = edit_text_file(&json!({
        "path": fixture.path,
        "expected_sha256": "0".repeat(64),
        "replacements": [{"old_text": "before", "new_text": "after", "expected_count": 1}]
    }));
    assert_eq!(result["code"], "ERR_TEXT_FILE_STALE");
    assert_eq!(result["original_unchanged"], true);
    assert_eq!(result["tool_mutated_file"], false);
    assert_eq!(result["external_change_detected"], true);
    assert_eq!(fs::read(&fixture.path).unwrap(), before);
}

#[test]
fn missing_and_ambiguous_matches_leave_original_unchanged() {
    let fixture = Fixture::new(b"same same");
    let before = fs::read(&fixture.path).unwrap();
    for (old_text, code) in [
        ("absent", "ERR_TEXT_FILE_MATCH_MISSING"),
        ("same", "ERR_TEXT_FILE_MATCH_AMBIGUOUS"),
    ] {
        let result = edit(
            &fixture,
            json!([{"old_text": old_text, "new_text": "new", "expected_count": 1}]),
        );
        assert_eq!(result["code"], code);
        assert_eq!(result["original_unchanged"], true);
        assert_eq!(fs::read(&fixture.path).unwrap(), before);
    }
}

#[test]
fn non_contiguous_exact_block_reports_a_split_repair_without_mutation() {
    let original = b"first,1\nmiddle,2\nlast,3\n";
    let fixture = Fixture::named("fixture.csv", original);
    let result = edit(
        &fixture,
        json!([{
            "old_text": "first,1\nlast,3",
            "new_text": "first,4\nlast,5",
            "expected_count": 1
        }]),
    );
    assert_eq!(result["code"], "ERR_TEXT_FILE_MATCH_NONCONTIGUOUS");
    assert_eq!(
        result["repair_strategy"],
        "split_into_independent_exact_replacements"
    );
    assert_eq!(result["original_unchanged"], true);
    assert_eq!(fs::read(&fixture.path).unwrap(), original);
}

#[test]
fn overlapping_groups_fail_before_writing() {
    let fixture = Fixture::new(b"prefix-middle-suffix");
    let before = fs::read(&fixture.path).unwrap();
    let result = edit(
        &fixture,
        json!([
            {"old_text": "prefix-middle", "new_text": "left", "expected_count": 1},
            {"old_text": "middle-suffix", "new_text": "right", "expected_count": 1}
        ]),
    );
    assert_eq!(result["code"], "ERR_TEXT_FILE_OVERLAPPING_REPLACEMENTS");
    assert_eq!(fs::read(&fixture.path).unwrap(), before);
}

#[test]
fn invalid_utf8_is_rejected_without_mutation() {
    let fixture = Fixture::new(&[0xff, 0xfe, b'x', 0]);
    let before = fs::read(&fixture.path).unwrap();
    let result = edit(
        &fixture,
        json!([{"old_text": "x", "new_text": "y", "expected_count": 1}]),
    );
    assert_eq!(result["code"], "ERR_TEXT_FILE_UNSUPPORTED_ENCODING");
    assert_eq!(result["original_unchanged"], true);
    assert_eq!(fs::read(&fixture.path).unwrap(), before);
}

#[test]
fn oversized_file_is_rejected_before_content_is_read() {
    let fixture = Fixture::new(b"seed");
    let file = OpenOptions::new().write(true).open(&fixture.path).unwrap();
    file.set_len(MAX_FILE_BYTES + 1).unwrap();
    drop(file);
    let before_len = fs::metadata(&fixture.path).unwrap().len();
    let result = read_text_file(&json!({"path": fixture.path}));
    assert_eq!(result["code"], "ERR_TEXT_FILE_TOO_LARGE");
    assert_eq!(result["original_unchanged"], true);
    assert_eq!(fs::metadata(&fixture.path).unwrap().len(), before_len);
}

#[test]
fn no_op_and_bad_contracts_do_not_touch_the_file() {
    let fixture = Fixture::new(b"stable");
    let before = fs::read(&fixture.path).unwrap();
    let no_op = edit(
        &fixture,
        json!([{"old_text": "stable", "new_text": "stable", "expected_count": 1}]),
    );
    assert_eq!(no_op["code"], "ERR_TEXT_FILE_NO_CHANGE");
    let missing_hash = edit_text_file(&json!({
        "path": fixture.path,
        "replacements": [{"old_text": "stable", "new_text": "changed", "expected_count": 1}]
    }));
    assert_eq!(missing_hash["code"], "ERR_TEXT_FILE_BAD_ARGUMENT");
    assert!(
        missing_hash["error"]
            .as_str()
            .is_some_and(|error| error.contains("call read_text_file"))
    );
    assert_eq!(fs::read(&fixture.path).unwrap(), before);
}

#[test]
fn edited_output_over_limit_is_rejected_before_staging() {
    let fixture = Fixture::new(b"x");
    let result = edit(
        &fixture,
        json!([{
            "old_text": "x",
            "new_text": "y".repeat(MAX_FILE_BYTES as usize + 1),
            "expected_count": 1
        }]),
    );
    assert_eq!(result["code"], "ERR_TEXT_FILE_TOO_LARGE");
    assert_eq!(result["proposed_byte_count"], MAX_FILE_BYTES + 1);
    assert_eq!(result["tool_mutated_file"], false);
    assert_eq!(fs::read(&fixture.path).unwrap(), b"x");
    assert!(fs::read_dir(&fixture.dir).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("sgt-edit")
    }));
}

#[test]
fn effect_receipts_distinguish_precommit_rejection_from_ambiguous_commit() {
    let path = Path::new("receipt.txt");
    for result in [
        failure("ERR_TEST", Some(path), "rejected", true),
        concurrent_change(path, &"0".repeat(64), None),
    ] {
        assert_eq!(result["effect_verified"], false);
        assert_eq!(result["effect_may_have_occurred"], false);
        assert_eq!(result["executed"], false);
        assert_eq!(
            EffectStatus::after_dispatch(&result, true),
            EffectStatus::ProvenNoEffect
        );
    }
    let ambiguous = ambiguous_commit(path, "unprovable", false, true, None, None);
    assert_eq!(ambiguous["tool_mutated_file"], false);
    assert_eq!(ambiguous["effect_may_have_occurred"], true);
    assert_eq!(
        EffectStatus::after_dispatch(&ambiguous, true),
        EffectStatus::MayHaveOccurred
    );
}

#[path = "text_file_transaction_tests.rs"]
mod transaction_tests;

#[path = "text_file_contract_tests.rs"]
mod contract_tests;
