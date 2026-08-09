use super::*;

fn test_card(streaming: bool) -> SceneCard {
    SceneCard {
        id: 42,
        rect: SceneRect {
            x: 1,
            y: 2,
            width: 300,
            height: 100,
        },
        html: "<html><body>result</body></html>".to_string(),
        background: "#ffffff".to_string(),
        opacity: 90,
        visible: true,
        streaming,
        stack_order: 7,
    }
}

#[test]
fn heartbeat_timeout_uses_saturating_elapsed_time() {
    assert!(!heartbeat_is_stale(5_000, 0));
    assert!(heartbeat_is_stale(5_001, 0));
    assert!(!heartbeat_is_stale(1, 2));
}

#[test]
fn identical_completed_sync_is_not_dispatched_again() {
    let card = test_card(false);
    assert_eq!(
        command_for_transition(Some(&card), &card, "result".to_string()),
        None
    );
}

#[test]
fn streaming_to_completed_transition_is_always_finalize() {
    let streaming = test_card(true);
    let completed = test_card(false);

    assert!(matches!(
        command_for_transition(Some(&streaming), &completed, "result".to_string()),
        Some(HostCommand::Finalize { .. })
    ));
}

#[test]
fn final_fit_remains_callable_after_iframe_resize() {
    let fitted = with_fit("<html><body>result</body></html>".to_string(), false);
    let bridged = with_card_bridge(fitted);

    assert!(bridged.contains("window.__SGT_RUN_FIT__=function(streaming)"));
    assert!(bridged.contains("window.addEventListener('resize'"));
    assert!(bridged.contains("requestFit(window.__SGT_STREAMING__)"));
    assert!(bridged.contains("reportCardState('bridge_ready', null)"));
    assert!(bridged.contains("reportCardState('script_error'"));
}

#[test]
fn card_content_stays_hidden_until_the_bundled_font_is_loaded() {
    let bridged = with_card_bridge("<html><body>result</body></html>".to_string());

    assert!(bridged.contains("if (!fontReady)"));
    assert!(bridged.contains("document.fonts.load"));
    assert!(bridged.contains("classList.add('sgt-font-ready')"));
    assert!(bridged.contains("reportCardState('font_failed'"));
}

#[test]
fn card_document_waits_for_activation_before_its_first_fit() {
    let fitted = with_fit("<html><body>result</body></html>".to_string(), true);

    assert!(fitted.contains("window.__SGT_RUN_FIT__=function(streaming)"));
    assert!(!fitted.contains("window.__SGT_RUN_FIT__(window.__SGT_STREAMING__)"));
}

#[test]
fn streaming_cards_use_the_full_fitter() {
    let fitted = with_fit("<html><body>result</body></html>".to_string(), true);
    assert!(fitted.contains("fit_font_to_window_runtime"));
    assert!(fitted.contains("const isStreamingFit = Boolean(streaming)"));
    assert!(fitted.contains("window.__SGT_STREAMING__=true"));
}

#[test]
fn finalization_reuses_the_loaded_document() {
    let bridged = with_card_bridge("<html><body>result</body></html>".to_string());

    assert!(bridged.contains("event.data.type === 'finalize'"));
    assert!(bridged.contains("window.__SGT_APPLY_STREAM_UPDATE__"));
    assert!(bridged.contains("animateNewWords: false"));
    assert!(bridged.contains("window.__SGT_INIT_STREAM_GRIDS__()"));
    assert!(bridged.contains("requestFit(false)"));
}

#[test]
fn streaming_keeps_the_legacy_word_reveal_contract() {
    let bridged = with_card_bridge("<html><body>result</body></html>".to_string());

    assert!(bridged.contains("lastRevealedIndex"));
    assert!(bridged.contains("targetWordsPerSecond = 40"));
    assert!(bridged.contains("runInlineSizing: true"));
}

#[test]
fn auto_fitted_streaming_content_remains_top_anchored() {
    let bridged = with_card_bridge("<html><body>result</body></html>".to_string());

    assert!(bridged.contains("window.scrollTo({"));
    assert!(bridged.contains("top: 0"));
    assert!(!bridged.contains("smoothScroll"));
}

#[test]
fn stream_updates_extract_only_body_markup() {
    assert_eq!(
        document_body("<html><body class='result'><p>Hello</p></body></html>"),
        "<p>Hello</p>"
    );
}
