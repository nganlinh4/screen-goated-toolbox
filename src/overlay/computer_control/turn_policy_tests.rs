use serde_json::json;

use super::*;

#[test]
fn lifecycle_is_derived_from_capabilities_not_user_words() {
    assert!(!is_mutating_tool("browser_read_page"));
    assert!(!is_mutating_tool("extract_artifact"));
    assert!(!is_mutating_tool("read_text_file"));
    assert!(!is_mutating_tool("done"));
    assert!(is_mutating_tool("edit_text_file"));
    assert!(is_mutating_tool("edit_text_file_structure"));
    assert!(is_mutating_tool("browser_navigate"));
}

#[test]
fn unknown_future_tools_remain_action_capabilities() {
    assert!(is_mutating_tool("future_dynamic_tool"));
    assert!(requires_post_dispatch_grounding("future_dynamic_tool"));
}

#[test]
fn complete_observation_results_skip_redundant_desktop_grounding() {
    for tool in [
        "read_text_file",
        "research_web",
        "browser_extract_page",
        "extract_artifact",
        "browser_tabs",
        "system_query",
    ] {
        assert!(!requires_post_dispatch_grounding(tool), "{tool}");
    }
    for tool in [
        "zoom",
        "reset_view",
        "see_whole_screen",
        "wait",
        "browser_wait_for",
        "browser_navigate",
    ] {
        assert!(requires_post_dispatch_grounding(tool), "{tool}");
    }
}

#[test]
fn completed_process_receipts_skip_unrelated_desktop_grounding() {
    let completed = json!({
        "evidence_kind": "exact_process_invocation",
        "process_completed": true,
        "exit_code": 1,
    });
    assert!(has_nonvisual_structured_receipt("run_command", &completed));
    assert!(has_nonvisual_structured_receipt(
        "run_command",
        &json!({"process_completed": true, "exit_code": 0})
    ));
    assert!(!has_nonvisual_structured_receipt(
        "run_command",
        &json!({
            "evidence_kind": "exact_process_invocation",
            "process_completed": false,
        })
    ));
    assert!(!has_nonvisual_structured_receipt("future_tool", &completed));
    assert!(has_nonvisual_structured_receipt(
        "future_tool",
        &json!({
            "structured_receipt": {
                "operation_complete": true,
                "desktop_grounding": "not_applicable",
            }
        })
    ));
}

#[test]
fn integration_effect_uses_structured_annotations_and_defaults_conservative() {
    assert!(!is_mutating_tool_with_integration_metadata(
        "mcp__future__read",
        Some(true)
    ));
    assert!(is_mutating_tool_with_integration_metadata(
        "mcp__future__write",
        Some(false)
    ));
    assert!(is_mutating_tool_with_integration_metadata(
        "mcp__future__unknown",
        None
    ));
}

#[test]
fn reconciliation_requires_external_state_evidence() {
    assert!(provides_reconciliation_evidence("observe"));
    assert!(provides_reconciliation_evidence("read_text_file"));
    assert!(provides_reconciliation_evidence("browser_read_page"));
    assert!(provides_reconciliation_evidence("extract_artifact"));
    assert!(provides_reconciliation_evidence("browser_wait_for"));
    assert!(!provides_reconciliation_evidence("wait"));
    assert!(!provides_reconciliation_evidence("browser_status"));
    assert!(!provides_reconciliation_evidence("done"));
    assert!(!provides_reconciliation_evidence("future_dynamic_tool"));
}

#[test]
fn task_summary_uses_executed_capabilities() {
    assert_eq!(task_class_from_tools(&[]), "conversation");
    assert_eq!(task_class_from_tools(&["done".to_string()]), "conversation");
    assert_eq!(
        task_class_from_tools(&["browser_read_page".to_string()]),
        "observation"
    );
    assert_eq!(
        task_class_from_tools(&["research_web".to_string()]),
        "research"
    );
    assert_eq!(
        task_class_from_tools(&["click_target".to_string()]),
        "action"
    );
}
