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
        .find("super::region::update(host);")
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

// Pins kept above: the WebView host wiring in child.rs has no observable
// surface without a live compositor window. The control-scene drag lifecycle
// (region invalidation, hiding, release, settlement, batching and teardown)
// runs in button_scene_runtime.test.cjs, geometry_runtime.test.cjs,
// scene_runtime.test.cjs, resize_runtime.test.cjs and the composed document
// probe (card_document.rs); native settlement is in gesture_tests.rs.
