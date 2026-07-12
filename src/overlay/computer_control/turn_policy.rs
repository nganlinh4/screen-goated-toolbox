//! Capability-derived lifecycle for Computer Control.
//!
//! User text and model prose never grant, deny, or rewrite tool calls. The full
//! catalog is available on every turn; structural checks live beside the action
//! they protect. This module only identifies whether a selected tool changes
//! state and builds the independent completion contract.

use serde_json::Value;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnMode {
    Action,
    #[default]
    Conversation,
}

impl TurnMode {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Action => "action",
            Self::Conversation => "conversation",
        }
    }
}

pub(super) fn needs_visual_done(mode: TurnMode) -> bool {
    mode == TurnMode::Action
}

pub(super) fn is_mutating_tool(tool: &str) -> bool {
    !matches!(
        tool,
        "observe"
            | "look"
            | "zoom"
            | "reset_view"
            | "see_whole_screen"
            | "map_targets"
            | "system_query"
            | "list_files"
            | "list_windows"
            | "read_clipboard"
            | "wait"
            | "search_memory"
            | "open_memory"
            | "research_web"
            | "browser_status"
            | "browser_read_page"
            | "browser_extract_page"
            | "browser_wait_for"
            | "browser_tabs"
            | "browser_network"
            | "browser_console"
            | "list_app_integrations"
            | "app_integration_status"
            | "read_app_integration_docs"
            | "artifact_info"
            | "decline_browser_control"
            | "decline_app_integration"
    )
}

pub(super) fn verification_goal(user_text: &str, intent: &str, done_args: &Value) -> String {
    let user_goal = user_text.trim();
    let model_intent = intent.trim();
    let claim = done_args
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    format!(
        "USER GOAL (authoritative): {}\nMODEL INTENT (secondary): {}\nCOMPLETION CLAIM (must be evidenced): {}",
        if user_goal.is_empty() {
            "(unavailable)"
        } else {
            user_goal
        },
        if model_intent.is_empty() {
            "(unavailable)"
        } else {
            model_intent
        },
        if claim.is_empty() {
            "(none supplied)"
        } else {
            claim
        },
    )
}

pub(super) fn task_class_from_tools(tools: &[String]) -> &'static str {
    if tools.iter().any(|tool| is_mutating_tool(tool)) {
        "action"
    } else if tools.iter().any(|tool| tool == "research_web") {
        "research"
    } else if tools.is_empty() {
        "conversation"
    } else {
        "observation"
    }
}

#[cfg(test)]
#[path = "turn_policy_tests.rs"]
mod tests;
