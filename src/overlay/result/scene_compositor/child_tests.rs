#[test]
fn authored_html_acceptance_is_captured_from_the_shared_compositor() {
    let source = include_str!("child.rs");

    assert!(source.contains("phase == \"interactive_document_alive\""));
    assert!(source.contains("acceptance_capture::capture_for_card(webview, *id)"));
}

#[test]
fn drag_expands_native_region_before_parent_notification() {
    let source = include_str!("child.rs");
    let branch = source
        .split("RendererInput::EventAndRefresh(event) =>")
        .nth(1)
        .expect("event and refresh branch must exist")
        .split("return;")
        .next()
        .expect("event and refresh branch must return");
    let expand = branch
        .find("super::region::update(host, true);")
        .expect("drag start must expand the native region");
    let notify = branch
        .find("emit_event(event);")
        .expect("drag start must notify the parent");
    let webview = branch
        .find("window.__SGT_BUTTON_SCENE__?.setDragActive(true,{gesture_id});")
        .expect("drag start must hide compositor controls");

    assert!(expand < webview);
    assert!(expand < notify);
}

#[test]
fn drag_region_clear_invalidates_proximity_cache_before_settlement() {
    let controls = include_str!("button_scene_runtime.js");
    let runtime = crate::overlay::result::scene_compositor::control_surface::document_script();
    let clear = controls
        .split("function clearClickableRegions()")
        .nth(1)
        .expect("button scene must clear native regions")
        .split("function hideControlsForDrag()")
        .next()
        .expect("region clear must precede drag hiding");
    let invalidate = clear
        .find("window.invalidateButtonRegions?.();")
        .expect("native region clear must invalidate its visibility cache");
    let notify = clear
        .find("window.ipc.postMessage")
        .expect("native region clear must notify the compositor");

    assert!(invalidate < notify);
    assert!(runtime.contains("window.invalidateButtonRegions = () =>"));
    assert!(runtime.contains("lastVisibleState.clear();"));
    assert!(runtime.contains("lastSentRegions.clear();"));
}

#[test]
fn drag_settlement_forces_one_acknowledged_button_region_snapshot() {
    let controls = include_str!("button_scene_runtime.js");
    let runtime = crate::overlay::result::scene_compositor::control_surface::document_script();
    let native_input = include_str!("button_input.rs");
    let settled = controls
        .split("function setDragActive(active, gestureId)")
        .nth(1)
        .expect("button scene must expose drag settlement")
        .split("function releaseDragPreview")
        .next()
        .expect("settlement must precede pointer release handling");

    assert!(settled.contains("rebuild(id)"));
    assert!(runtime.contains("window.restoreButtonRegionsAfterDrag = gestureId =>"));
    assert!(runtime.contains("? \"restore_clickable_regions\""));
    assert!(native_input.contains("if action == \"restore_clickable_regions\""));
    let restore = native_input
        .split("if action == \"restore_clickable_regions\"")
        .nth(1)
        .unwrap()
        .split("if action == \"update_clickable_regions\"")
        .next()
        .unwrap();
    let settle = restore.find("settle_drag(gesture_id(&message));").unwrap();
    let regions = restore.find("update_regions(&message);").unwrap();
    assert!(settle < regions);
}

#[test]
fn drag_hides_controls_until_release_then_hands_preview_to_committed_geometry() {
    let child = include_str!("child.rs");
    let child_commands = include_str!("child_commands.rs");
    let controls = include_str!("button_scene_runtime.js");
    let pointer = crate::overlay::result::scene_compositor::control_surface::document_script();
    let resize = include_str!("resize_runtime.js");

    assert!(!child.contains("ChildEvent::DragFinished { .. } =>"));
    assert!(!pointer.contains("activeResultDragPreview = null;\n    setResultDraggingCursor(false);\n    window.__SGT_BUTTON_SCENE__?.setDragActive(false)"));
    assert!(
        !resize.contains("active = null;\n    window.__SGT_BUTTON_SCENE__?.setDragActive(false)")
    );
    let hiding = controls
        .split("function hideControlsForDrag()")
        .nth(1)
        .unwrap()
        .split("function rebuild(")
        .next()
        .unwrap();
    assert!(hiding.contains("clearClickableRegions()"));
    assert!(hiding.contains("style.visibility = 'hidden'"));
    assert!(pointer.contains("group.style.translate = offset"));
    assert!(pointer.contains("window.__SGT_BUTTON_SCENE__?.releaseDragPreview("));
    let released = controls
        .split("function releaseDragPreview(pointerX, pointerY, gestureId)")
        .nth(1)
        .unwrap();
    assert!(released.contains("awaitingDragSettle = id"));
    assert!(!released.contains("style.visibility = ''"));
    assert!(released.contains("window.updateCursorPosition?.(pointerX, pointerY)"));
    assert!(!released.contains("clearResultDragControlPreview"));
    let rebuild = controls
        .split("function rebuild(")
        .nth(1)
        .unwrap()
        .split("function apply(command)")
        .next()
        .unwrap();
    assert!(rebuild.contains("restoreControlsAfterLayout"));
    assert!(rebuild.contains("style.visibility = ''"));
    let host_commands = include_str!("host_command_runtime.js");
    assert!(host_commands.contains("hasReleasedDragPreview?.() === true"));
    assert!(host_commands.contains("if (!preservePreview)"));
    let scene = include_str!("scene_runtime.js");
    assert!(scene.contains("const preservePosition = window.shouldPreserveResultDragGeometry?."));
    assert!(scene.contains("if (!preservePosition)"));
    assert!(pointer.contains("settlingResultDragTargets = new Set(drag.targets)"));
    assert!(controls.contains("window.releaseResultDragGeometryLock?.()"));
    assert!(child_commands.contains("super::button_input::settle_drag(*gesture_id)"));
    let settled = controls.find("command.type === 'drag_settled'").unwrap();
    let merge = controls[settled..].find("mergeCard(card)").unwrap();
    let reveal = controls[settled..]
        .find("setDragActive(false, gestureId)")
        .unwrap();
    assert!(merge < reveal);
    assert!(controls[settled..].contains("matchesExternal"));
}

#[test]
fn gesture_teardown_and_lost_release_have_native_recovery_paths() {
    let child = include_str!("child.rs");
    let commands = include_str!("child_commands.rs");
    let input = include_str!("button_input.rs");
    let teardown = include_str!("scene_command_helpers.js");
    let resize = include_str!("resize_runtime.js");
    let pointer = crate::overlay::result::scene_compositor::control_surface::document_script();

    assert!(child.contains("reconcile_released_pointer()"));
    assert!(input.contains("pub(super) fn captures_desktop_input()"));
    assert!(input.contains("pub(super) fn cancel_removed_card"));
    assert!(input.contains("pub(super) fn cancel_missing_cards"));
    assert!(commands.contains("cancel_removed_card(*id)"));
    assert!(commands.contains("cancel_missing_cards(&cards)"));
    assert!(teardown.contains("window.cancelResultDragForCard?.(key)"));
    assert!(resize.contains("setDragActive(false, gestureId)"));
    assert!(pointer.contains("gesture_id: drag.gestureId"));
}

#[test]
fn batched_card_creation_also_populates_the_control_scene() {
    let controls = include_str!("button_scene_runtime.js");
    let branch = controls
        .find("command.type === 'upsert_batch'")
        .expect("button scene must handle batched card creation");
    let next_branch = controls[branch..]
        .find("command.type === 'stream'")
        .map(|offset| branch + offset)
        .expect("stream branch must follow batched card creation");

    assert!(controls[branch..next_branch].contains("mergeCard(card)"));
}
