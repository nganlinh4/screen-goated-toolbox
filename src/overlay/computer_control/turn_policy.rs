//! Capability-derived lifecycle for Computer Control.
//!
//! User text and model prose never grant, deny, or rewrite tool calls. The full
//! catalog is available on every turn; structural checks live beside the action
//! they protect. This module only identifies whether a selected tool changes
//! state.

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

pub(super) fn is_mutating_tool(tool: &str) -> bool {
    is_mutating_tool_with_integration_metadata(tool, super::mcp::declared_tool_is_read_only(tool))
}

/// Whether the acting model needs a newly captured frame after dispatch.
///
/// A capability result that completely represents a read does not justify a
/// second desktop capture. View transforms and waits are different: their
/// useful result is the newly visible state. Unknown future tools remain
/// conservative because they are classified as mutating above.
pub(super) fn requires_post_dispatch_grounding(tool: &str) -> bool {
    is_mutating_tool(tool)
        || matches!(
            tool,
            "zoom"
                | "reset_view"
                | "see_whole_screen"
                | "map_targets"
                | "wait"
                | "browser_wait_for"
        )
}

/// Some effects have an exact structural receipt but no meaningful visual
/// postcondition. Capturing the desktop would add unrelated state rather than
/// strengthen the result.
pub(super) fn has_nonvisual_structured_receipt(tool: &str, result: &Value) -> bool {
    let declared_nonvisual = result
        .pointer("/structured_receipt/operation_complete")
        .and_then(Value::as_bool)
        == Some(true)
        && result
            .pointer("/structured_receipt/desktop_grounding")
            .and_then(Value::as_str)
            == Some("not_applicable");
    declared_nonvisual
        || (tool == "run_command"
            && result.get("process_completed").and_then(Value::as_bool) == Some(true))
}

fn is_mutating_tool_with_integration_metadata(
    tool: &str,
    integration_read_only: Option<bool>,
) -> bool {
    if tool.starts_with("mcp__") {
        return integration_read_only != Some(true);
    }
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
            | "read_text_file"
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
            | "extract_artifact"
            | "done"
    )
}

/// Whether a successful result gives the replacement session fresh external
/// state after an interrupted mutation. Pure delay, connection health, and the
/// terminal claim are intentionally not reconciliation evidence.
pub(super) fn provides_reconciliation_evidence(tool: &str) -> bool {
    !is_mutating_tool(tool)
        && !matches!(
            tool,
            "wait"
                | "browser_status"
                | "list_app_integrations"
                | "app_integration_status"
                | "read_app_integration_docs"
                | "done"
        )
}

pub(super) fn task_class_from_tools(tools: &[String]) -> &'static str {
    if tools.iter().any(|tool| is_mutating_tool(tool)) {
        "action"
    } else if tools.iter().any(|tool| tool == "research_web") {
        "research"
    } else if tools.iter().all(|tool| tool == "done") {
        "conversation"
    } else {
        "observation"
    }
}

#[cfg(test)]
#[path = "turn_policy_tests.rs"]
mod tests;
