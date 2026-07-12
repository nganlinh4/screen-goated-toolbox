//! Per-call action scope and bounded follow-up approval.

use super::*;

pub(in crate::overlay::computer_control) fn call_access(
    mode: TurnMode,
    control_revoked: bool,
    tool: &str,
    args: &Value,
    user_text: &str,
) -> ToolAccess {
    if mode == TurnMode::ReadOnly
        && !control_revoked
        && !explicit_read_only(user_text)
        && classify(user_text, "") == TaskClass::WebResearch
        && matches!(tool, "open_url" | "browser_navigate")
    {
        return ToolAccess::Allow;
    }
    let access = tool_access(mode, control_revoked, tool);
    if access == ToolAccess::Allow
        && tool_requests_submission(tool, args)
        && !request_authorizes_submission(user_text)
    {
        ToolAccess::BlockedSubmissionScope
    } else {
        access
    }
}

pub(in crate::overlay::computer_control) fn request_authorizes_submission(user_text: &str) -> bool {
    let text = normalize(user_text);
    let explicit_submit = any(
        &text,
        &[
            "submit",
            "send",
            "post",
            "publish",
            "press enter",
            "click submit",
            "click send",
            "제출",
            "전송",
            "보내",
            "gửi",
            "đăng",
        ],
    );
    let typing_only = any(
        &text,
        &["type", "fill", "paste", "입력", "붙여", "gõ", "điền", "dán"],
    );
    explicit_submit || (any(&text, &["search", "look up"]) && !typing_only)
}

pub(in crate::overlay::computer_control) fn request_is_edit_only(user_text: &str) -> bool {
    let text = normalize(user_text);
    any(
        &text,
        &["type", "fill", "paste", "입력", "붙여", "gõ", "điền", "dán"],
    ) && !request_authorizes_submission(user_text)
}

pub(in crate::overlay::computer_control) fn is_affirmative_followup(user_text: &str) -> bool {
    let text = normalize(user_text);
    text.chars().count() <= 32
        && any(
            &text,
            &[
                "ok",
                "okay",
                "yes",
                "sure",
                "go ahead",
                "do it",
                "được",
                "đồng ý",
                "luôn",
                "ừ",
                "네",
                "응",
                "좋아",
                "동의",
            ],
        )
}

pub(in crate::overlay::computer_control) fn substantive_turn_allows_action_refinement(
    user_text: &str,
    tool: &str,
) -> bool {
    if explicit_stop_or_revocation(user_text)
        || explicit_read_only(user_text)
        || !is_mutating_tool(tool)
    {
        return false;
    }
    let normalized = normalize(user_text);
    normalized
        .chars()
        .filter(|char| char.is_alphanumeric())
        .count()
        >= 16
}

fn tool_requests_submission(tool: &str, args: &Value) -> bool {
    match tool {
        "type_text" => args.get("press_enter").and_then(Value::as_bool) == Some(true),
        "key_combination" => args
            .get("keys")
            .and_then(Value::as_str)
            .is_some_and(|keys| normalize(keys).split_whitespace().any(|key| key == "enter")),
        "act" => args.get("verb").and_then(Value::as_str) == Some("submit"),
        "do_steps" => args
            .get("steps")
            .and_then(Value::as_array)
            .is_some_and(|steps| {
                steps
                    .iter()
                    .any(|step| step.get("verb").and_then(Value::as_str) == Some("submit"))
            }),
        _ => false,
    }
}
