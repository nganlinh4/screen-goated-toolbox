use serde_json::json;

use super::*;

#[test]
fn lifecycle_is_derived_from_capabilities_not_user_words() {
    assert!(!is_mutating_tool("browser_read_page"));
    assert!(is_mutating_tool("browser_navigate"));
}

#[test]
fn unknown_future_tools_remain_action_capabilities() {
    assert!(is_mutating_tool("future_dynamic_tool"));
}

#[test]
fn task_summary_uses_executed_capabilities() {
    assert_eq!(task_class_from_tools(&[]), "conversation");
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

#[test]
fn verifier_keeps_user_goal_authoritative() {
    let goal = verification_goal(
        "Select the requested item",
        "Inspect navigation options",
        &json!({"summary": "The item is selected"}),
    );
    assert!(goal.starts_with("USER GOAL (authoritative): Select the requested item"));
    assert!(goal.contains("MODEL INTENT (secondary): Inspect navigation options"));
}

#[test]
fn only_selected_action_lifecycle_needs_visual_done() {
    assert!(needs_visual_done(TurnMode::Action));
    assert!(!needs_visual_done(TurnMode::Conversation));
}
