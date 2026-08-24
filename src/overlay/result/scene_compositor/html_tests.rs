use super::DOCUMENT;
use std::sync::LazyLock;

/// The document as the browser receives it: the shell with its scene runtime
/// inlined. The runtime moved out to `scene_runtime.js` — matching the other
/// runtimes in this directory — so assertions about runtime behaviour have to
/// read the composed output, not the shell.
static COMPOSED: LazyLock<String> =
    LazyLock::new(|| DOCUMENT.replace("__SGT_SCENE_RUNTIME__", include_str!("scene_runtime.js")));

#[test]
fn ordinary_cards_use_one_shared_document_runtime() {
    assert!(COMPOSED.contains("attachShadow({ mode: 'open' })"));
    assert!(COMPOSED.contains("entry.mode = 'direct'"));
    assert!(COMPOSED.contains("applyDirectContent(entry, message)"));
    assert!(!COMPOSED.contains("frame.src = '/card.html'"));
}

#[test]
fn raw_html_stays_isolated_and_web_navigation_leaves_the_shared_compositor() {
    assert!(COMPOSED.contains("function loadIsolatedDocument"));
    assert!(
        COMPOSED.contains(".direct-host[hidden],.result-frame[hidden]{display:none!important}")
    );
    assert!(COMPOSED.contains(".direct-host,.result-frame{position:absolute;inset:0"));
    assert!(COMPOSED.contains("entry.frame.srcdoc = documentHtml"));
    assert!(COMPOSED.contains("frame.referrerPolicy = 'no-referrer'"));
    assert!(COMPOSED.contains("frame.setAttribute('sandbox'"));
    assert!(!COMPOSED.contains("allow-same-origin"));
    assert!(COMPOSED.contains("event.source !== entry.frame.contentWindow"));
    assert!(COMPOSED.contains("event.data.card_id"));
    assert!(COMPOSED.contains("event.data.document_revision"));
    assert!(COMPOSED.contains("entry.navigationDepth !== 0"));
    assert!(COMPOSED.contains("postMessage(message, '*')"));
    assert!(COMPOSED.contains("function navigateTo(entry, url)"));
    assert!(COMPOSED.contains("type: 'navigation_request'"));
    assert!(!COMPOSED.contains("anchor.target === '_blank'"));
    assert!(COMPOSED.contains("if (model.external_navigation === true)"));
    assert!(!COMPOSED.contains("native_document"));
    assert!(!COMPOSED.contains("entry.frame.src = url"));
    assert!(COMPOSED.contains("if (nextDocument !== null)"));
    assert!(COMPOSED.contains("'document_content_committed'"));
    assert!(COMPOSED.contains("activateCard(entry, becameVisible);\n    return;"));
    let raw_branch = COMPOSED
        .split("if (nextDocument !== null)")
        .nth(1)
        .and_then(|tail| tail.split("selectSurface(entry, null)").next())
        .expect("raw-document branch");
    assert!(!raw_branch.contains("prepareSettledReveal"));
}

#[test]
fn isolated_html_has_a_rounded_mask_and_compositor_owned_resize_edges() {
    let document = super::super::card_document::compositor_document("http://127.0.0.1:32123");

    assert!(COMPOSED.contains(".result-frame{border-radius:inherit;clip-path:inset(0 round"));
    assert!(COMPOSED.contains(".resize-handle{position:absolute;z-index:4"));
    assert!(document.contains("action: 'result_resize_start'"));
    assert!(document.contains("action: 'result_resize_finish'"));
    assert!(document.contains("requestAnimationFrame(render)"));
}

#[test]
fn interactive_raw_document_acceptance_requires_a_visible_surface() {
    let settled = include_str!("settled_reveal_runtime.js");

    assert!(settled.contains("function isolatedSurfaceVisibility(entry)"));
    assert!(settled.contains("entry.frame.isConnected"));
    assert!(settled.contains("entry.frame.hidden"));
    assert!(settled.contains("rect.left < window.innerWidth"));
    assert!(COMPOSED.contains("phase === 'interactive_document_alive'"));
    assert!(COMPOSED.contains("'interactive_surface_visible'"));
}

#[test]
fn isolated_bridge_handshake_activates_the_loaded_document() {
    assert!(COMPOSED.contains("event.data.phase === 'bridge_ready'"));
    assert!(COMPOSED.contains("entry.activateIsolatedBridge = activateIsolatedBridge"));
    assert!(COMPOSED.contains("entry.activateIsolatedBridge()"));
    assert!(COMPOSED.contains("entry.commandPort = event.ports"));
    assert!(COMPOSED.contains("entry.commandPort.postMessage(message)"));
    assert!(COMPOSED.contains("event.data.type === 'frame_request'"));
    assert!(COMPOSED.contains("type: 'frame_tick'"));
    assert!(COMPOSED.contains("if (entry.ready || entry.navigationDepth !== 0) return"));
}

#[test]
fn content_revisions_reject_stale_paint_acknowledgements() {
    assert!(COMPOSED.contains("content_revision: ++entry.contentRevision"));
    assert!(COMPOSED.contains("contentRevision !== entry.contentRevision"));
    assert!(COMPOSED.contains("revision === entry.contentRevision"));
}

#[test]
fn fits_are_serialized_and_pending_updates_are_coalesced_per_card() {
    assert!(COMPOSED.contains("const pendingFits = new Map()"));
    assert!(COMPOSED.contains("if (activeFit || pendingFits.size === 0) return"));
    assert!(COMPOSED.contains("pendingFits.set(key, next)"));
    assert!(COMPOSED.contains("window.__SGT_FIT_CONTEXT__"));
    assert!(COMPOSED.contains("fontReady: true"));
    assert!(COMPOSED.contains("'final_fit_completed'"));
    assert!(COMPOSED.contains("contentRevision: entry.contentRevision"));
}

#[test]
fn finalized_cards_reject_late_stream_updates() {
    assert!(COMPOSED.contains("if (entry.contentPhase === 'finalized')"));
    assert!(COMPOSED.contains("'stale_stream_ignored'"));
    assert!(COMPOSED.contains("entry.contentPhase = 'finalized'"));
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
    assert!(COMPOSED.contains("entry.card.style.transform = 'translate3d('"));
    assert!(!COMPOSED.contains("entry.card.style.left ="));
    assert!(!COMPOSED.contains("entry.card.style.top ="));
}

#[test]
fn result_card_outline_does_not_bleed_into_the_control_gap() {
    assert!(COMPOSED.contains("box-shadow:inset 0 0 0 1px var(--result-outline)"));
    assert!(!COMPOSED.contains("0 8px 28px rgba(0,0,0,.22)"));
}

#[test]
fn refining_cards_own_a_compositor_only_processing_signal() {
    assert!(COMPOSED.contains("window.__SGT_CREATE_PROCESSING_AURA__()"));
    assert!(COMPOSED.contains("const processing = entry.refining || entry.navigationLoading"));
    assert!(COMPOSED.contains("model.processing_effect === 'minimal'"));
    assert!(COMPOSED.contains("entry.processing.resize(width, height, scale)"));

    let document = super::super::card_document::compositor_document("http://127.0.0.1:32123");
    assert!(document.contains("motion.setAttribute('type', 'rotate')"));
    assert!(document.contains("gradient.setAttribute('gradientUnits', 'userSpaceOnUse')"));
    assert!(document.contains("const halfSpan = Math.hypot(width, height) / 2"));
    assert!(document.contains("const edge = stroke"));
    assert!(document.contains("entry.processing.setState(processing, entry.processingEffect)"));
    assert!(document.contains("processing-runner-glow"));
    assert!(!document.contains("stroke-dasharray"));
    assert!(document.contains("@keyframes sgt-processing-scan"));
    assert!(document.contains("stroke:#00ff00"));
    assert!(document.contains("--processing-track"));
    assert!(document.contains("pathLength', '100'"));
    assert!(document.contains("prefers-reduced-motion:reduce"));
}

#[test]
fn external_navigation_keeps_the_processing_shell_until_the_native_page_is_ready() {
    assert!(COMPOSED.contains("entry.navigationLoading = Boolean(model.navigation_loading)"));
    assert!(COMPOSED.contains("entry.mode = 'navigation-loading'"));
    assert!(COMPOSED.contains("entry.directHost.hidden = true; entry.frame.hidden = true"));
    let loading = COMPOSED
        .find("if (entry.navigationLoading)")
        .expect("navigation loading branch");
    let native = COMPOSED
        .find("if (model.external_navigation === true)")
        .expect("native navigation branch");
    assert!(loading < native);
}

#[test]
fn text_only_cards_keep_the_fitter_without_card_chrome() {
    let direct_runtime = include_str!("direct_runtime.js");
    let shape_runtime = include_str!("shape_runtime.js");
    assert!(shape_runtime.contains("window.__SGT_SHAPE_LAYOUT__"));
    assert!(shape_runtime.contains("prefersVerticalWriting"));
    assert!(shape_runtime.contains("Script=Han"));
    assert!(direct_runtime.contains("shapeLayout.prefersVerticalWriting(text)"));
    assert!(!direct_runtime.contains("overflowWrap = 'anywhere'"));
    assert!(COMPOSED.contains("data-presentation=\"text_only\""));
    assert!(COMPOSED.contains("background:transparent!important;box-shadow:none"));
    assert!(COMPOSED.contains("const backdrop = document.createElement('img')"));
    assert!(COMPOSED.contains("model.backdrop_data_url || ''"));
    assert!(COMPOSED.contains("entry.directHost.style.setProperty('--text-color'"));
    assert!(COMPOSED.contains("window.__SGT_RUN_FIT__"));
    assert!(COMPOSED.contains("border-radius:3px;pointer-events:auto;user-select:text"));
    assert!(COMPOSED.contains("user-select:text;cursor:text"));
    assert!(COMPOSED.contains("entry.sourceReplacement = model.source_replacement === true"));
    assert!(COMPOSED.contains("preferredFontSize: entry.preferredFontSize"));
    assert!(COMPOSED.contains("sourceReplacement: entry.sourceReplacement === true"));
    assert!(COMPOSED.contains("preferredFontSize / scale"));
    assert!(direct_runtime.contains("isSourceReplacement ? '1.08' : '1.5'"));
    assert!(direct_runtime.contains("? 'center'"));
    assert!(direct_runtime.contains("[100, 90, 80, 70, 60, 50, 40, 30, 25]"));
    assert!(direct_runtime.contains("Math.min(100, candidateWidth)"));
    assert!(!direct_runtime.contains("stretchHigh = 151"));
    assert!(direct_runtime.contains("applyTypography(widthItem, fontMiddle, 50)"));
    assert!(direct_runtime.contains("var vertical = regions[widthItemIndex].vertical === true;"));
    assert!(!direct_runtime.contains(
        "region.vertical === true\n                    && shapeLayout.prefersVerticalWriting"
    ));
    assert!(direct_runtime.contains("fits(text, size, fitTolerance, true)"));
    assert!(direct_runtime.contains("!rejectPathologicalWrap || !hasPathologicalWrap"));
    assert!(!direct_runtime.contains("verticalInset"));
}

#[test]
fn source_replacements_cannot_enter_the_ordinary_result_fitter() {
    assert!(COMPOSED.contains("if (entry.sourceReplacement === true)"));
    assert!(COMPOSED.contains("completeFit(entry);"));
    let fit = crate::overlay::result::markdown_view::fit::runtime_fit_script();
    assert!(!fit.contains("isSourceReplacement"));
    assert!(!fit.contains("preferredFontSize"));
}

#[test]
fn streamed_words_use_a_bounded_adaptive_reveal_queue() {
    let reveal_runtime = include_str!("reveal_runtime.js");
    assert!(reveal_runtime.contains("var maximumLag = 80"));
    assert!(reveal_runtime.contains("reveal.queue.shift()"));
    assert!(reveal_runtime.contains("40 * (1 + reveal.queue.length / 10)"));
    assert!(reveal_runtime.contains("generation !== reveal.generation"));
    assert!(reveal_runtime.contains("opacity 0.22s ease-out"));
    assert!(!reveal_runtime.contains("blur(3px)"));
}

#[test]
fn ordinary_results_retain_a_scroll_recovery_path_while_streaming() {
    let direct_runtime = include_str!("direct_runtime.js");
    assert!(direct_runtime.contains("body.style.overflowY = 'auto'"));
    assert!(direct_runtime.contains("options.sourceReplacement === true"));
    assert!(!direct_runtime.contains("current * size.height / body.scrollHeight * 0.92"));
}

#[test]
fn streaming_dom_replacement_is_immediate() {
    assert!(COMPOSED.contains("if (flushPendingContent(entry)) return;"));
    assert!(COMPOSED.contains("activateCard(entry, becameVisible);"));
    assert!(!COMPOSED.contains("contentFrame"));
    assert!(!COMPOSED.contains("lastContentFlushAt"));
    assert!(!COMPOSED.contains("lastStreamingFitAt"));
    assert!(!COMPOSED.contains("streamingFitTimer"));
}

#[test]
fn ordinary_streaming_preserves_unchanged_dom_identity() {
    let direct_runtime = include_str!("direct_runtime.js");
    let patch_runtime = include_str!("dom_patch_runtime.js");
    assert!(direct_runtime.contains("window.__SGT_PATCH_BODY__(body, options.html)"));
    assert!(patch_runtime.contains("syncNode(existing, next)"));
    assert!(patch_runtime.contains("current.nodeValue = fresh.nodeValue"));
    assert!(!patch_runtime.contains("body.innerHTML = html"));
    assert!(COMPOSED.contains("requestRefinement: function()"));
}

#[test]
fn direct_card_destruction_stops_persistent_scale_motion() {
    let direct_runtime = include_str!("direct_runtime.js");
    assert!(direct_runtime.contains("cancelAnimationFrame(motion.frame)"));
    assert!(direct_runtime.contains("state.fit._sgtMotionController = null"));
}

#[test]
fn resizing_debounces_fit_without_penalizing_position_only_dragging() {
    assert!(COMPOSED.contains("const resized = entry.card.clientWidth !== width"));
    assert!(COMPOSED.contains("setTimeout(function() { queueFit(entry, entry.streaming); }, 40)"));
}

#[test]
fn theme_and_interaction_updates_stay_inside_the_shared_scene() {
    let document = super::super::card_document::compositor_document("http://127.0.0.1:32123");

    assert!(COMPOSED.contains("document.getElementById('sgt-theme-css').textContent"));
    assert!(COMPOSED.contains("card.addEventListener('pointerdown'"));
    assert!(document.contains("command.type === 'raise'"));
}

#[test]
fn result_text_is_selectable() {
    assert!(COMPOSED.contains("contain:layout paint style;user-select:text"));
    assert!(COMPOSED.contains("shadow.addEventListener('copy'"));
    assert!(COMPOSED.contains("action: 'copy_selection'"));
    assert!(COMPOSED.contains("event.data.type === 'copy_selection'"));
}

#[test]
fn stale_content_commands_cannot_undo_the_latest_interaction_order() {
    assert!(COMPOSED.contains("if (order >= current)"));
    assert!(COMPOSED.contains("raiseCard(entry)"));
}
