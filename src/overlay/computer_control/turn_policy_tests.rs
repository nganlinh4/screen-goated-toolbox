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
fn visible_search_control_can_be_refined_to_an_action() {
    let user = "Can you search for the requested item here?";
    let intent = "Type into the current search field and submit";
    assert_eq!(turn_mode(user, intent), TurnMode::Action);
    assert!(auto_research_args(user, intent, intent, "look", &json!({})).is_none());
}

#[test]
fn generic_internet_search_stays_read_only() {
    let user = "Search the internet to find out what happened";
    let intent = "Open a search page and inspect the results";
    assert_eq!(turn_mode(user, intent), TurnMode::ReadOnly);
}

#[test]
fn evidence_research_stays_read_only() {
    let user = "Find sources and verify this definition";
    assert_eq!(
        turn_mode(user, "open the current search field"),
        TurnMode::ReadOnly
    );
    assert!(auto_research_args(user, user, "", "look", &json!({})).is_some());
}

#[test]
fn research_navigation_does_not_authorize_general_mutation() {
    let request = "Search the internet and explain the answer";
    assert_eq!(
        call_access(
            TurnMode::ReadOnly,
            false,
            "open_url",
            &json!({"url": "https://example.com"}),
            request,
        ),
        ToolAccess::Allow
    );
    assert_eq!(
        call_access(
            TurnMode::ReadOnly,
            false,
            "act",
            &json!({"id": 1, "verb": "click"}),
            request,
        ),
        ToolAccess::BlockedReadOnly
    );
    assert_eq!(
        call_access(
            TurnMode::ReadOnly,
            false,
            "open_url",
            &json!({"url": "https://example.com"}),
            "Search the internet but do not open anything",
        ),
        ToolAccess::BlockedReadOnly
    );
}

#[test]
fn authorized_action_is_not_rerouted_by_quoted_research_text() {
    let user = "Open the selected result";
    let intent = "Continue the action after the earlier phrase about searching";
    assert!(auto_research_args(user, user, intent, "look", &json!({})).is_none());
}

#[test]
fn typing_search_words_into_a_field_is_an_action_not_research() {
    let user = "Type a sentence about searching into the current input box";
    assert_eq!(classify(user, ""), TaskClass::DesktopAction);
    assert_eq!(turn_mode(user, ""), TurnMode::Action);
    assert!(auto_research_args(user, user, "", "observe", &json!({})).is_none());
}

#[test]
fn behavior_questions_do_not_resume_computer_actions() {
    let user = "Why did you click and type without asking?";
    assert_eq!(classify(user, ""), TaskClass::ChatOnly);
    assert_eq!(turn_mode(user, "continue clicking"), TurnMode::Conversation);
}

#[test]
fn typing_permission_does_not_include_submission() {
    let user = "Type the requested text into the field";
    assert_eq!(
        call_access(
            TurnMode::Action,
            false,
            "type_text",
            &json!({"text": "draft", "press_enter": true}),
            user,
        ),
        ToolAccess::BlockedSubmissionScope
    );
    assert!(!request_authorizes_submission(user));
    assert!(request_is_edit_only(user));
    assert!(request_authorizes_submission(
        "Type the draft and press Enter"
    ));
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

#[test]
fn generic_stop_and_revocation_phrases_latch_control() {
    for phrase in [
        "Stop.",
        "Can you stop?",
        "Please stop now.",
        "Cancel that.",
        "Never mind.",
        "Don't control my computer.",
        "Stop using the mouse.",
        "Stop scrolling.",
    ] {
        assert_eq!(
            turn_mode(phrase, "I was about to click"),
            TurnMode::Stopped,
            "phrase: {phrase}"
        );
    }
}

#[test]
fn stop_labels_inside_action_requests_do_not_revoke_control() {
    assert_eq!(turn_mode("Click the stop button", ""), TurnMode::Action);
    assert_eq!(turn_mode("Stop the media playback", ""), TurnMode::Action);
}

#[test]
fn common_action_verbs_restore_control_without_model_inference() {
    for phrase in [
        "Save this document",
        "Rename this item",
        "Mute the current audio",
        "Upload this file",
        "이 문서를 저장해",
        "Lưu tài liệu này",
    ] {
        assert_eq!(turn_mode(phrase, ""), TurnMode::Action, "phrase: {phrase}");
        assert!(explicitly_authorizes_control(phrase), "phrase: {phrase}");
    }
}

#[test]
fn bundled_languages_can_revoke_and_request_read_only_control() {
    for phrase in ["멈춰", "컴퓨터 제어를 멈춰", "Dừng lại", "Ngừng điều khiển"]
    {
        assert_eq!(turn_mode(phrase, "I will click"), TurnMode::Stopped);
    }
    for phrase in ["설명만 해줘", "클릭하지 마", "Chỉ giải thích", "Đừng nhấp"]
    {
        assert_eq!(turn_mode(phrase, "I will click"), TurnMode::ReadOnly);
    }
}

#[test]
fn explicit_advice_and_read_only_language_overrides_model_action_intent() {
    for phrase in [
        "Just tell me what to do.",
        "Explain it without clicking.",
        "Don't type anything; only explain.",
        "How do I open this setting?",
        "Read this panel.",
        "Don't open that window.",
        "What happens if I click this?",
    ] {
        assert_eq!(
            turn_mode(phrase, "I will click and type to demonstrate"),
            TurnMode::ReadOnly,
            "phrase: {phrase}"
        );
    }
}

#[test]
fn mutation_requires_an_action_mode_and_an_open_latch() {
    assert_eq!(
        tool_access(TurnMode::Action, false, "click_target"),
        ToolAccess::Allow
    );
    assert_eq!(
        tool_access(TurnMode::ReadOnly, false, "click_target"),
        ToolAccess::BlockedReadOnly
    );
    assert_eq!(
        tool_access(TurnMode::Action, true, "click_target"),
        ToolAccess::BlockedRevoked
    );
    assert_eq!(
        tool_access(TurnMode::Conversation, false, "dynamic_write_tool"),
        ToolAccess::BlockedConversation
    );
    assert_eq!(
        tool_access(TurnMode::ReadOnly, true, "look"),
        ToolAccess::Allow
    );
    assert_eq!(
        tool_access(TurnMode::Stopped, true, "look"),
        ToolAccess::BlockedStopped
    );
}

#[test]
fn revocation_is_restored_only_by_explicit_user_action() {
    assert!(explicitly_authorizes_control("Open the preferences window"));
    assert!(!explicitly_authorizes_control(
        "What do you think I should do?"
    ));
    assert!(!explicitly_authorizes_control("Don't click; explain it"));
}

#[test]
fn model_intent_may_clarify_a_reference_but_not_a_question() {
    assert!(intent_may_authorize_control("Do that one"));
    assert!(!intent_may_authorize_control("Right there"));
    assert!(!intent_may_authorize_control("I said hello there"));
    assert!(!intent_may_authorize_control("Is this safe?"));
    assert_eq!(
        turn_mode("What is this page?", "I should click around to inspect it"),
        TurnMode::ReadOnly
    );
}

#[test]
fn ambiguous_transcript_cannot_be_promoted_into_an_action() {
    assert_eq!(
        turn_mode("Right there", "click at the current cursor"),
        TurnMode::Conversation
    );
    assert_eq!(
        tool_access(TurnMode::Conversation, false, "click_here"),
        ToolAccess::BlockedConversation
    );
    assert_eq!(
        tool_access(TurnMode::Conversation, false, "observe"),
        ToolAccess::Allow
    );
}

#[test]
fn conversation_can_research_but_cannot_mutate() {
    assert_eq!(
        tool_access(TurnMode::Conversation, false, "research_web"),
        ToolAccess::Allow
    );
    assert_eq!(
        tool_access(TurnMode::Conversation, false, "browser_setup"),
        ToolAccess::BlockedConversation
    );
}

#[test]
fn short_multilingual_affirmations_are_recognized() {
    for text in ["Yes", "OK", "Được", "Ok luôn", "동의", "네"] {
        assert!(is_affirmative_followup(text), "text: {text}");
    }
    assert!(!is_affirmative_followup(
        "Okay, but first explain what this changes and why"
    ));
}

#[test]
fn only_action_turns_need_visual_done_verification() {
    assert!(needs_visual_done(TurnMode::Action));
    assert!(!needs_visual_done(TurnMode::ReadOnly));
    assert!(!needs_visual_done(TurnMode::Conversation));
    assert!(!needs_visual_done(TurnMode::Stopped));
}
