//! Code-owned turn policy for Computer Control.
//!
//! This keeps recurrent reliability rules out of the system prompt: classify the
//! user's current intent, detect weak tool choices, and provide compact recovery
//! instructions or deterministic reroutes.

use serde_json::{Value, json};

#[path = "turn_policy_phrases.rs"]
mod phrases;
#[path = "turn_policy_scope.rs"]
mod scope;
use phrases::{ACTION_TERMS, READ_ONLY_PHRASES, STOP_PHRASES};
pub(super) use scope::{
    call_access, is_affirmative_followup, request_authorizes_submission, request_is_edit_only,
    substantive_turn_allows_action_refinement,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TaskClass {
    ScreenAnswer,
    WebResearch,
    DesktopAction,
    BrowserAction,
    Setup,
    ChatOnly,
    Unclear,
}

/// Code-owned authorization for the current user turn. This is deliberately
/// separate from `TaskClass`: a screen-reading task and a general question need
/// different tools, but neither authorizes keyboard/mouse/application mutation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnMode {
    Action,
    ReadOnly,
    #[default]
    Conversation,
    Stopped,
}

impl TurnMode {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Action => "action",
            Self::ReadOnly => "read_only",
            Self::Conversation => "conversation",
            Self::Stopped => "stopped",
        }
    }

    /// Only action turns remain open until a visible/action postcondition is met.
    /// Spoken answers are complete when the model turn completes.
    pub(super) fn needs_action_completion(self) -> bool {
        self == Self::Action
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolAccess {
    Allow,
    BlockedConversation,
    BlockedSubmissionScope,
    BlockedReadOnly,
    BlockedStopped,
    BlockedRevoked,
}

impl ToolAccess {
    pub(super) fn reason(self) -> &'static str {
        match self {
            Self::Allow => "allowed",
            Self::BlockedConversation => {
                "the current turn is conversational and authorizes no mutating computer tools"
            }
            Self::BlockedSubmissionScope => {
                "the user authorized editing/typing but did not authorize submitting or sending"
            }
            Self::BlockedReadOnly => "the current turn did not authorize computer changes",
            Self::BlockedStopped => "the user stopped the current task",
            Self::BlockedRevoked => "computer-control permission remains revoked",
        }
    }
}

impl TaskClass {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            TaskClass::ScreenAnswer => "screen_answer",
            TaskClass::WebResearch => "web_research",
            TaskClass::DesktopAction => "desktop_action",
            TaskClass::BrowserAction => "browser_action",
            TaskClass::Setup => "setup",
            TaskClass::ChatOnly => "chat_only",
            TaskClass::Unclear => "unclear",
        }
    }
}

pub(super) fn classify(user_text: &str, intent: &str) -> TaskClass {
    let text = normalize(&format!("{user_text} {intent}"));
    if feedback_or_behavior_question(&text) {
        return TaskClass::ChatOnly;
    }
    if direct_manipulation_request(&text) {
        return TaskClass::DesktopAction;
    }
    if any(
        &text,
        &[
            "search",
            "google",
            "verify",
            "source",
            "double check",
            "look up",
        ],
    ) {
        return TaskClass::WebResearch;
    }
    if any(
        &text,
        &[
            "what is",
            "what are",
            "explain",
            "define",
            "definition",
            "terms",
        ],
    ) && any(
        &text,
        &[
            "not sure",
            "unsure",
            "uncertain",
            "exact",
            "accurate",
            "confirm",
        ],
    ) {
        return TaskClass::WebResearch;
    }
    if any(&text, &["set up", "setup", "enable", "turn on"])
        && any(&text, &["browser", "integration", "extension", "control"])
    {
        return TaskClass::Setup;
    }
    if any(&text, ACTION_TERMS) {
        return TaskClass::DesktopAction;
    }
    if any(&text, &["this page", "web page", "tab", "browser"])
        && any(
            &text,
            &[
                "navigate",
                "go to",
                "switch",
                "reload",
                "refresh",
                "upload",
                "download",
                "go back",
                "go forward",
                "new tab",
            ],
        )
    {
        return TaskClass::BrowserAction;
    }
    if any(
        &text,
        &[
            "screen",
            "image",
            "post",
            "read",
            "what does it say",
            "this page",
            "web page",
            "this tab",
        ],
    ) {
        return TaskClass::ScreenAnswer;
    }
    if any(&text, &["why", "how", "what do you think", "do you think"]) {
        return TaskClass::ChatOnly;
    }
    TaskClass::Unclear
}

/// Resolve the safety/lifecycle mode for a turn. Explicit user restrictions win
/// over any action language in model reasoning.
pub(super) fn turn_mode(user_text: &str, intent: &str) -> TurnMode {
    if explicit_stop_or_revocation(user_text) {
        return TurnMode::Stopped;
    }
    if explicit_read_only(user_text) {
        return TurnMode::ReadOnly;
    }
    let user_class = classify(user_text, "");
    let class = if user_class == TaskClass::Unclear && ambiguous_action_directive(user_text) {
        classify(user_text, intent)
    } else if user_class == TaskClass::WebResearch
        && search_may_target_current_surface(user_text)
        && intent_describes_surface_action(intent)
    {
        TaskClass::BrowserAction
    } else {
        user_class
    };
    match class {
        TaskClass::DesktopAction | TaskClass::BrowserAction | TaskClass::Setup => TurnMode::Action,
        TaskClass::ScreenAnswer | TaskClass::WebResearch => TurnMode::ReadOnly,
        TaskClass::ChatOnly | TaskClass::Unclear => TurnMode::Conversation,
    }
}

/// A revocation latch is cleared only by a new, explicit action request in the
/// user's own transcript. Model reasoning alone may clarify a normal turn, but it
/// must never silently restore permission the user withdrew.
pub(super) fn explicitly_authorizes_control(user_text: &str) -> bool {
    !explicit_stop_or_revocation(user_text)
        && !explicit_read_only(user_text)
        && matches!(
            classify(user_text, ""),
            TaskClass::DesktopAction | TaskClass::BrowserAction | TaskClass::Setup
        )
}

/// Model intent may clarify the target/route only after the user's own words
/// contain an action directive. A merely ambiguous transcript never grants
/// control: it might be an ASR error and must be clarified conversationally.
pub(super) fn intent_may_authorize_control(user_text: &str) -> bool {
    !explicit_stop_or_revocation(user_text)
        && !explicit_read_only(user_text)
        && (classify(user_text, "") == TaskClass::Unclear && ambiguous_action_directive(user_text)
            || search_may_target_current_surface(user_text))
}

/// Gate a model-selected tool before it reaches the executor. Unknown tools are
/// mutating by default because installed integrations can expose arbitrary writes.
pub(super) fn tool_access(mode: TurnMode, control_revoked: bool, tool: &str) -> ToolAccess {
    if tool == "done" {
        return ToolAccess::Allow;
    }
    if mode == TurnMode::Stopped {
        return ToolAccess::BlockedStopped;
    }
    if !is_mutating_tool(tool) {
        return ToolAccess::Allow;
    }
    if mode == TurnMode::Conversation {
        return ToolAccess::BlockedConversation;
    }
    if control_revoked {
        return ToolAccess::BlockedRevoked;
    }
    if mode != TurnMode::Action {
        return ToolAccess::BlockedReadOnly;
    }
    ToolAccess::Allow
}

/// `done` is visually verified only for turns that were authorized to change the
/// computer. Answers and observations have no desktop postcondition to inspect.
pub(super) fn needs_visual_done(mode: TurnMode) -> bool {
    mode.needs_action_completion()
}

pub(super) fn is_mutating_tool(tool: &str) -> bool {
    !matches!(
        tool,
        // Local/screen observation.
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
            // Read-only memory, web, browser, integration, and artifact queries.
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
            // These only persist the user's refusal; they do not control an app.
            | "decline_browser_control"
            | "decline_app_integration"
    )
}

fn explicit_stop_or_revocation(user_text: &str) -> bool {
    let text = normalize(user_text);
    if any(&text, STOP_PHRASES) {
        return true;
    }

    // Catch polite/bare forms without turning commands such as "press the stop
    // button" into a controller revocation.
    let mut saw_stop = false;
    for word in text.split_whitespace() {
        if word == "stop" {
            saw_stop = true;
        } else if !matches!(
            word,
            "can"
                | "could"
                | "would"
                | "will"
                | "you"
                | "please"
                | "just"
                | "now"
                | "okay"
                | "ok"
                | "thanks"
                | "thank"
                | "it"
                | "that"
                | "doing"
                | "for"
                | "me"
        ) {
            return false;
        }
    }
    saw_stop
}

fn explicit_read_only(user_text: &str) -> bool {
    let text = normalize(user_text);
    any(&text, READ_ONLY_PHRASES)
}

pub(super) fn auto_research_args(
    user_text: &str,
    task: &str,
    intent: &str,
    tool: &str,
    args: &Value,
) -> Option<Value> {
    if turn_mode(user_text, intent) == TurnMode::Action {
        return None;
    }
    let class = classify(user_text, intent);
    if class != TaskClass::WebResearch || !is_weak_for_research(tool) {
        return None;
    }
    let query = extract_query(user_text, task, intent, args);
    Some(json!({
        "query": query,
        "purpose": "Answer the user's verification/explanation request using web sources before speaking.",
        "source_policy": "best_available",
        "max_sources": 3,
        "rerouted_from": tool,
    }))
}

/// Build the immutable completion contract from the user's request, with model
/// intent and the model's final claim retained as secondary evidence. This keeps
/// a fluent internal plan from silently replacing the requested outcome.
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

fn search_may_target_current_surface(user_text: &str) -> bool {
    let text = normalize(user_text);
    any(&text, &["search", "find"])
        && any(
            &text,
            &[
                "here",
                "this page",
                "this site",
                "this app",
                "current page",
                "current site",
                "search box",
                "search field",
                "input box",
            ],
        )
}

fn ambiguous_action_directive(user_text: &str) -> bool {
    let text = normalize(user_text);
    ["do ", "use ", "continue ", "proceed ", "apply "]
        .iter()
        .any(|prefix| text.starts_with(prefix))
}

fn feedback_or_behavior_question(text: &str) -> bool {
    any(
        text,
        &[
            "why did you",
            "what did you",
            "what are you doing",
            "how did you",
            "who told you",
        ],
    )
}

fn direct_manipulation_request(text: &str) -> bool {
    any(
        text,
        &[
            "click", "type", "fill", "paste", "press", "select", "drag", "scroll", "focus", "클릭",
            "입력", "붙여", "눌러", "gõ", "điền", "dán", "nhấp", "nhấn",
        ],
    )
}

fn intent_describes_surface_action(intent: &str) -> bool {
    let text = normalize(intent);
    any(&text, ACTION_TERMS) || any(&text, &["fill", "use the current", "current search field"])
}

fn is_weak_for_research(tool: &str) -> bool {
    matches!(
        tool,
        "observe"
            | "look"
            | "wait"
            | "click_at"
            | "click_target"
            | "map_targets"
            | "read_clipboard"
            | "list_windows"
    )
}

fn extract_query(user_text: &str, task: &str, intent: &str, args: &Value) -> String {
    let q = args
        .get("query")
        .and_then(Value::as_str)
        .or_else(|| args.get("question").and_then(Value::as_str))
        .unwrap_or("");
    for candidate in [q, user_text, task, intent] {
        let candidate = candidate.trim();
        if candidate.chars().count() >= 8 {
            return candidate.chars().take(240).collect();
        }
    }
    "verify the requested information".to_string()
}

fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| {
        text == *needle
            || text.starts_with(&format!("{needle} "))
            || text.ends_with(&format!(" {needle}"))
            || text.contains(&format!(" {needle} "))
    })
}

#[cfg(test)]
#[path = "turn_policy_tests.rs"]
mod tests;
