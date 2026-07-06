//! Code-owned turn policy for Computer Control.
//!
//! This keeps recurrent reliability rules out of the system prompt: classify the
//! user's current intent, detect weak tool choices, and provide compact recovery
//! instructions or deterministic reroutes.

use serde_json::{Value, json};

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
    if any(
        &text,
        &[
            "click", "type", "open", "close", "move", "drag", "play", "pause",
        ],
    ) {
        return TaskClass::DesktopAction;
    }
    if any(&text, &["this page", "web page", "tab", "browser"]) {
        return TaskClass::BrowserAction;
    }
    if any(
        &text,
        &["screen", "image", "post", "read", "what does it say"],
    ) {
        return TaskClass::ScreenAnswer;
    }
    if any(&text, &["why", "how", "what do you think", "do you think"]) {
        return TaskClass::ChatOnly;
    }
    TaskClass::Unclear
}

pub(super) fn auto_research_args(
    user_text: &str,
    task: &str,
    intent: &str,
    tool: &str,
    args: &Value,
) -> Option<Value> {
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

pub(super) fn stall_nudge(user_text: &str, intent: &str, stall_count: u32) -> String {
    let class = classify(user_text, intent);
    let guidance = match class {
        TaskClass::WebResearch => {
            "The user asked for verification/search. Call research_web now; do not ask whether to search."
        }
        TaskClass::ScreenAnswer => {
            "If the answer is already spoken, call done with the evidence. Otherwise call look with a specific question."
        }
        TaskClass::BrowserAction => {
            "Use browser_read_page/browser tools if connected; avoid visual browsing loops."
        }
        TaskClass::Setup => {
            "Check the typed setup status tool and stop with a blocker if setup is not converging."
        }
        _ => {
            "If you have answered, call done. If more evidence is needed, take the next tool action now."
        }
    };
    format!(
        "(Harness recovery after no-action turn #{stall_count}; not a new user request.) {guidance}"
    )
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
}

fn any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn explicit_google_request_is_web_research() {
        assert_eq!(
            classify("please search Google for the exact definition", ""),
            TaskClass::WebResearch
        );
    }

    #[test]
    fn definition_with_accuracy_request_is_web_research() {
        assert_eq!(
            classify("can you explain these terms accurately? I am not sure", ""),
            TaskClass::WebResearch
        );
    }

    #[test]
    fn weak_visual_tool_reroutes_to_research() {
        let args = auto_research_args(
            "If you are not sure, please search Google.",
            "verify the technical explanation",
            "",
            "observe",
            &json!({}),
        )
        .expect("research route");

        assert_eq!(
            args.get("rerouted_from").and_then(|v| v.as_str()),
            Some("observe")
        );
        assert_eq!(
            args.get("source_policy").and_then(|v| v.as_str()),
            Some("best_available")
        );
    }

    #[test]
    fn screen_reading_does_not_reroute_look() {
        assert!(
            auto_research_args(
                "read the post on screen",
                "read visible post",
                "",
                "look",
                &json!({"question": "read the post"})
            )
            .is_none()
        );
    }
}
