use super::DOCUMENT;

#[test]
fn ordinary_cards_use_one_shared_document_runtime() {
    assert!(DOCUMENT.contains("attachShadow({ mode: 'open' })"));
    assert!(DOCUMENT.contains("entry.mode = 'direct'"));
    assert!(DOCUMENT.contains("applyDirectContent(entry, message)"));
    assert!(!DOCUMENT.contains("frame.src = '/card.html'"));
}

#[test]
fn raw_html_and_navigation_keep_an_isolated_frame_boundary() {
    assert!(DOCUMENT.contains("function loadIsolatedDocument"));
    assert!(DOCUMENT.contains("isolatedOrigin + '/card/'"));
    assert!(DOCUMENT.contains("frame.referrerPolicy = 'no-referrer'"));
    assert!(!DOCUMENT.contains("setAttribute('sandbox'"));
    assert!(DOCUMENT.contains("event.origin !== isolatedOrigin"));
    assert!(DOCUMENT.contains("event.data.card_id"));
    assert!(DOCUMENT.contains("event.data.document_revision"));
    assert!(DOCUMENT.contains("entry.navigationDepth !== 0"));
    assert!(DOCUMENT.contains("postMessage(message, '*')"));
    assert!(!DOCUMENT.contains("sgtresult://localhost/card/"));
    assert!(DOCUMENT.contains("function navigateTo(entry, url)"));
    assert!(DOCUMENT.contains("entry.frame.src = url"));
}

#[test]
fn isolated_bridge_handshake_flushes_content_before_iframe_load_fallback() {
    assert!(DOCUMENT.contains("event.data.phase === 'bridge_ready'"));
    assert!(DOCUMENT.contains("entry.activateIsolatedBridge = activateIsolatedBridge"));
    assert!(DOCUMENT.contains("entry.activateIsolatedBridge()"));
    assert!(DOCUMENT.contains("entry.commandPort = event.ports"));
    assert!(DOCUMENT.contains("entry.commandPort.postMessage(message)"));
    assert!(DOCUMENT.contains("event.data.type === 'frame_request'"));
    assert!(DOCUMENT.contains("type: 'frame_tick'"));
    assert!(DOCUMENT.contains("if (entry.ready || entry.navigationDepth !== 0) return"));
}

#[test]
fn content_revisions_reject_stale_paint_acknowledgements() {
    assert!(DOCUMENT.contains("content_revision: ++entry.contentRevision"));
    assert!(DOCUMENT.contains("contentRevision !== entry.contentRevision"));
    assert!(DOCUMENT.contains("revision === entry.contentRevision"));
}

#[test]
fn fits_are_serialized_and_pending_updates_are_coalesced_per_card() {
    assert!(DOCUMENT.contains("const pendingFits = new Map()"));
    assert!(DOCUMENT.contains("if (activeFit || pendingFits.size === 0) return"));
    assert!(DOCUMENT.contains("pendingFits.set(key, next)"));
    assert!(DOCUMENT.contains("window.__SGT_FIT_CONTEXT__"));
    assert!(DOCUMENT.contains("fontReady: true"));
    assert!(DOCUMENT.contains("'final_fit_completed'"));
    assert!(DOCUMENT.contains("contentRevision: entry.contentRevision"));
}

#[test]
fn finalized_cards_reject_late_stream_updates() {
    assert!(DOCUMENT.contains("if (entry.contentPhase === 'finalized')"));
    assert!(DOCUMENT.contains("'stale_stream_ignored'"));
    assert!(DOCUMENT.contains("entry.contentPhase = 'finalized'"));
}

#[test]
fn renderer_readiness_is_gated_on_the_single_bundled_font() {
    let document = super::super::card_document::compositor_document("http://127.0.0.1:32123");
    assert!(document.contains("src:url('/font.ttf')"));
    assert!(!document.contains("Segoe UI"));
    assert!(document.contains("document.fonts.load"));
    assert!(document.contains("type: 'font_ready'"));
    assert!(document.contains("window.ipc.postMessage('renderer_ready')"));
}

#[test]
fn moving_a_card_uses_only_a_compositor_transform() {
    assert!(DOCUMENT.contains("entry.card.style.transform = 'translate3d('"));
    assert!(!DOCUMENT.contains("entry.card.style.left ="));
    assert!(!DOCUMENT.contains("entry.card.style.top ="));
}

#[test]
fn result_card_outline_does_not_bleed_into_the_control_gap() {
    assert!(DOCUMENT.contains("box-shadow:inset 0 0 0 1px var(--result-outline)"));
    assert!(!DOCUMENT.contains("0 8px 28px rgba(0,0,0,.22)"));
}

#[test]
fn resizing_debounces_fit_without_penalizing_position_only_dragging() {
    assert!(DOCUMENT.contains("const resized = entry.card.clientWidth !== width"));
    assert!(DOCUMENT.contains("setTimeout(function() { queueFit(entry, entry.streaming); }, 40)"));
}

#[test]
fn theme_and_interaction_updates_stay_inside_the_shared_scene() {
    assert!(DOCUMENT.contains("document.getElementById('sgt-theme-css').textContent"));
    assert!(DOCUMENT.contains("card.addEventListener('pointerdown'"));
    assert!(DOCUMENT.contains("command.type === 'raise'"));
}

#[test]
fn stale_content_commands_cannot_undo_the_latest_interaction_order() {
    assert!(DOCUMENT.contains("if (order >= current)"));
    assert!(DOCUMENT.contains("raiseCard(entry)"));
}
