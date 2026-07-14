use super::*;

#[test]
fn old_extension_fallback_selects_only_the_foreground_active_tab() {
    let tabs = json!([
        {"id": 1, "title": "first", "active": true},
        {"id": 2, "title": "second", "active": true},
        {"id": 3, "title": "hidden", "active": false}
    ]);
    assert_eq!(select_active_tab(&tabs, "second - Browser"), Some(2));
    assert_eq!(select_active_tab(&tabs, "unknown - Browser"), None);
}

#[test]
fn fallback_does_not_assume_a_lone_extension_tab_is_foreground() {
    let tabs = json!([{"id": 1, "title": "other", "active": true}]);
    assert_eq!(select_active_tab(&tabs, "foreground - Browser"), None);
}

#[test]
fn dispatch_prefers_atomic_guard_then_verified_explicit_tab() {
    assert_eq!(
        select_active_dispatch_mode(true, true),
        Some(ActiveDispatchMode::ExtensionGuard)
    );
    assert_eq!(
        select_active_dispatch_mode(false, true),
        Some(ActiveDispatchMode::VerifiedExplicitTab)
    );
    assert_eq!(select_active_dispatch_mode(false, false), None);
}

#[test]
fn click_activation_is_one_atomic_mouse_gesture() {
    let (method, params) = atomic_activation(12.5, 19.0);
    assert_eq!(method, "Input.synthesizeTapGesture");
    assert_eq!(params["x"], 12.5);
    assert_eq!(params["y"], 19.0);
    assert_eq!(params["tapCount"], 1);
    assert_eq!(params["gestureSourceType"], "mouse");
}

fn classify(value: Value, require_focus: bool) -> std::result::Result<TargetSnapshot, Value> {
    classify_target_snapshot(
        &value,
        "[data-sgt-id=\"4\"]",
        73,
        "before_input",
        "document-a",
        "element-a",
        require_focus,
    )
}

#[test]
fn matching_document_element_and_focus_are_fresh() {
    let result = classify(
        json!({
            "documentId":"document-a", "elementId":"element-a", "present":true,
            "focused":true, "x":12.5, "y":19.0,
        }),
        true,
    )
    .unwrap();
    assert_eq!(result.x, 12.5);
}

#[test]
fn navigation_between_resolution_and_input_is_typed_stale() {
    let stale = classify(
        json!({
            "documentId":"document-b", "elementId":"element-a", "present":true,
            "focused":true, "x":12, "y":19,
        }),
        true,
    )
    .unwrap_err();
    assert_eq!(stale["code"], "ERR_BROWSER_STALE_TARGET");
    assert_eq!(stale["reason"], "document_changed");
    assert_eq!(stale["target_tab_id"], 73);
    assert_eq!(stale["effect_may_have_occurred"], false);
}

#[test]
fn replacement_element_or_stolen_focus_fails_before_input() {
    let replaced = classify(
        json!({"documentId":"document-a", "elementId":"element-b", "present":true}),
        false,
    )
    .unwrap_err();
    assert_eq!(replaced["reason"], "element_changed");
    let unfocused = classify(
        json!({
            "documentId":"document-a", "elementId":"element-a", "present":true,
            "focused":false,
        }),
        true,
    )
    .unwrap_err();
    assert_eq!(unfocused["reason"], "focus_changed");
}
