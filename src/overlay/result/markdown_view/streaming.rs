//! Streaming content functions for markdown view

mod fit_impl;

use windows::Win32::Foundation::*;

use super::WEBVIEWS;
use super::conversion::markdown_to_html;
use super::webview::create_markdown_webview_ex;

pub use fit_impl::{fit_font_to_window, fit_font_to_window_streaming, init_gridjs};

#[derive(Clone, Copy)]
struct StreamingUpdateOptions {
    run_inline_sizing: bool,
    animate_new_words: bool,
    smooth_scroll: bool,
}

/// Stream markdown content - optimized for rapid updates during streaming.
/// Uses innerHTML instead of document recreation.
pub fn stream_markdown_content(parent_hwnd: HWND, markdown_text: &str) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;
    let (is_refining, preset_prompt, input_text) = {
        let states = crate::overlay::result::state::WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get(&hwnd_key) {
            (
                state.is_refining,
                state.preset_prompt.clone(),
                state.input_text.clone(),
            )
        } else {
            (false, String::new(), String::new())
        }
    };

    stream_markdown_content_ex(
        parent_hwnd,
        markdown_text,
        is_refining,
        &preset_prompt,
        &input_text,
    )
}

/// Finalize streamed markdown content without the live-stream tail effects.
pub fn finalize_stream_markdown_content(parent_hwnd: HWND, markdown_text: &str) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;
    let (is_refining, preset_prompt, input_text) = {
        let states = crate::overlay::result::state::WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get(&hwnd_key) {
            (
                state.is_refining,
                state.preset_prompt.clone(),
                state.input_text.clone(),
            )
        } else {
            (false, String::new(), String::new())
        }
    };

    finalize_stream_markdown_content_ex(
        parent_hwnd,
        markdown_text,
        is_refining,
        &preset_prompt,
        &input_text,
    )
}

/// Stream markdown content - internal version for rapid streaming updates.
pub fn stream_markdown_content_ex(
    parent_hwnd: HWND,
    markdown_text: &str,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
) -> bool {
    update_stream_markdown_content_ex(
        parent_hwnd,
        markdown_text,
        is_refining,
        preset_prompt,
        input_text,
        StreamingUpdateOptions {
            run_inline_sizing: true,
            animate_new_words: true,
            smooth_scroll: true,
        },
    )
}

/// Finalize streamed markdown content for the last flush after streaming stops.
pub fn finalize_stream_markdown_content_ex(
    parent_hwnd: HWND,
    markdown_text: &str,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
) -> bool {
    update_stream_markdown_content_ex(
        parent_hwnd,
        markdown_text,
        is_refining,
        preset_prompt,
        input_text,
        StreamingUpdateOptions {
            run_inline_sizing: false,
            animate_new_words: false,
            smooth_scroll: false,
        },
    )
}

fn update_stream_markdown_content_ex(
    parent_hwnd: HWND,
    markdown_text: &str,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
    options: StreamingUpdateOptions,
) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;

    if !WEBVIEWS.with(|webviews| webviews.borrow().contains_key(&hwnd_key)) {
        return create_markdown_webview_ex(
            parent_hwnd,
            markdown_text,
            false,
            is_refining,
            preset_prompt,
            input_text,
        );
    }

    let html = markdown_to_html(markdown_text, is_refining, preset_prompt, input_text);
    let body_content = if let Some(body_start) = html.find("<body>") {
        let after_body = &html[body_start + 6..];
        if let Some(body_end) = after_body.find("</body>") {
            &after_body[..body_end]
        } else {
            &html[..]
        }
    } else {
        &html[..]
    };

    let escaped_content = body_content
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${");
    let scroll_behavior = if options.smooth_scroll {
        "smooth"
    } else {
        "auto"
    };
    let source_len = markdown_text.len();
    let source_trimmed_len = markdown_text.trim().len();

    let script = format!(
        r#"(function() {{
    const newContent = `{}`;
    const sourceTextLen = {};
    const sourceTrimmedLen = {};
    const prevWordCount = window._streamWordCount || 0;
    const prevRenderCount = window._streamRenderCount || 0;
    const shouldRunInlineSizing = {};
    const shouldAnimateNewWords = {};
    const scrollBehavior = '{}';

    document.body.innerHTML = newContent;

    var body = document.body;
    var doc = document.documentElement;
    if (!body || !doc) {{
        if (window.__SGT_REPORT_RENDER_DIAGNOSTICS__) {{
            window.__SGT_REPORT_RENDER_DIAGNOSTICS__({{
                phase: 'stream_update',
                reason: 'missing_body_after_stream_update',
                sourceTextLen: sourceTextLen,
                sourceTrimmedLen: sourceTrimmedLen
            }});
        }}
        return;
    }}

    var winH = window.innerHeight;
    var winW = window.innerWidth;
    var isConstrainedWindow = (winH < 260 || winW < 420);
    var text = (body.innerText || body.textContent || '').trim();
    var textLen = text.length;
    var isNewSession = (prevRenderCount === 0 || (prevWordCount < 5 && textLen < 50));
    var isConstrainedShortContent = isConstrainedWindow && textLen < 450;

    function currentLineHeightPx() {{
        var computed = window.getComputedStyle(body);
        var fontSize = parseFloat(computed.fontSize) || parseFloat(body.style.fontSize) || 14;
        var lineHeight = parseFloat(computed.lineHeight);
        if (!Number.isFinite(lineHeight)) {{
            var inlineLineHeight = parseFloat(body.style.lineHeight);
            lineHeight = fontSize * (Number.isFinite(inlineLineHeight) ? inlineLineHeight : 1.5);
        }}
        return Math.max(1, lineHeight);
    }}

    function hasPathologicalWrap() {{
        if (textLen < 8) {{
            return false;
        }}

        var tokens = text.split(/\s+/).filter(Boolean);
        var wordCount = tokens.length;
        var longestToken = 0;
        for (var i = 0; i < tokens.length; i++) {{
            longestToken = Math.max(longestToken, tokens[i].length);
        }}

        var approxLineCount = Math.max(1, Math.round(doc.scrollHeight / currentLineHeightPx()));
        var avgCharsPerLine = textLen / approxLineCount;

        return avgCharsPerLine < 3.5
            && approxLineCount > Math.max(3, wordCount + 1)
            && (wordCount <= 12 || longestToken >= 4);
    }}

    function fitsVertically() {{
        void body.offsetHeight;
        return doc.scrollHeight <= (winH + 2) && !hasPathologicalWrap();
    }}

    var minSize = (textLen < 200) ? 6 : 14;

    if (shouldRunInlineSizing) {{
        if (isNewSession) {{
            var maxPossible = Math.min(isConstrainedWindow ? 84 : 110, winH);
            var estimated = Math.sqrt((winW * winH) / (textLen + 1));
            var low = Math.max(minSize, Math.floor(estimated * 0.5));
            var high = Math.min(maxPossible, Math.ceil(estimated * 1.15));
            if (low > high) low = high;

            body.style.fontVariationSettings = "'wght' 400, 'wdth' 90, 'slnt' 0, 'ROND' 100";
            body.style.letterSpacing = '0px';
            body.style.wordSpacing = '0px';
            body.style.lineHeight = '1.5';
            body.style.paddingTop = '0';
            body.style.paddingBottom = '0';

            var blocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
            for (var i = 0; i < blocks.length; i++) {{
                blocks[i].style.marginBottom = '0.5em';
                blocks[i].style.paddingBottom = '0';
            }}

            void body.offsetHeight;
            var best = low;
            while (low <= high) {{
                var mid = Math.floor((low + high) / 2);
                body.style.fontSize = mid + 'px';
                if (fitsVertically()) {{
                    best = mid;
                    low = mid + 1;
                }} else {{
                    high = mid - 1;
                }}
            }}
            if (best < minSize) best = minSize;
            body.style.fontSize = best + 'px';

            if (isConstrainedShortContent) {{
                void body.offsetHeight;
                var settleLow = minSize;
                var settleHigh = best;
                var settleBest = minSize;
                while (settleLow <= settleHigh) {{
                    var settleMid = Math.floor((settleLow + settleHigh) / 2);
                    body.style.fontSize = settleMid + 'px';
                    if (fitsVertically()) {{
                        settleBest = settleMid;
                        settleLow = settleMid + 1;
                    }} else {{
                        settleHigh = settleMid - 1;
                    }}
                }}
                body.style.fontSize = settleBest + 'px';
            }}
        }} else {{
            var hasOverflow = !fitsVertically();
            if (hasOverflow) {{
                var currentSize = parseFloat(body.style.fontSize) || 14;
                if (currentSize > minSize) {{
                    var low = minSize;
                    var high = currentSize;
                    var best = minSize;
                    while (low <= high) {{
                        var mid = Math.floor((low + high) / 2);
                        body.style.fontSize = mid + 'px';
                        if (fitsVertically()) {{
                            best = mid;
                            low = mid + 1;
                        }} else {{
                            high = mid - 1;
                        }}
                    }}
                    body.style.fontSize = best + 'px';
                }}
            }}
        }}
    }}

    const words = document.querySelectorAll('.word');
    const newWordCount = words.length;

    // ===== ADAPTIVE WORD-BY-WORD REVEAL =====
    // Instead of fading in all new words at once per chunk, queue them and
    // release on rAF at a backlog-adaptive rate. Low backlog → smooth
    // ~25ms/word feel; large bursts → rate scales so the displayed text
    // catches up to the model without hiding throughput behind animation.
    // Credit accumulation gives a natural ~16ms floor (1 word per frame).
    if (!window._streamRevealState) {{
        window._streamRevealState = {{
            queue: [],
            active: false,
            lastRevealedIndex: -1,
            lastTick: 0,
            credits: 0
        }};
    }}
    var revealState = window._streamRevealState;

    if (isNewSession) {{
        // First render of a session — all words default-visible, no animation.
        revealState.queue = [];
        revealState.active = false;
        revealState.lastRevealedIndex = newWordCount - 1;
        revealState.credits = 0;
    }} else if (!shouldAnimateNewWords) {{
        // Finalize path: flush anything pending and reveal the rest instantly.
        if (revealState.queue.length > 0) {{
            revealState.queue.forEach(function(item) {{
                if (item.el && item.el.isConnected) {{
                    item.el.style.visibility = 'visible';
                    item.el.style.opacity = '1';
                    item.el.style.filter = 'blur(0)';
                    item.el.style.transform = 'translateY(0)';
                }}
            }});
            revealState.queue = [];
        }}
        revealState.active = false;
        revealState.lastRevealedIndex = newWordCount - 1;
        revealState.credits = 0;
    }} else {{
        // Word-centric hide: visibility:hidden keeps the word IN LAYOUT (so
        // scrollHeight reflects the FULL final paragraph as soon as the
        // chunk is parsed), but the word is invisible until revealed. This
        // lets the per-chunk fit measure the real final height and commit
        // a correct font-size up front — preventing the "fast single
        // mega-chunk" overshoot where display:none used to leave queued
        // words out of layout and the fit undersized the content.
        //
        // innerHTML was just replaced — any pre-existing queue entries hold
        // stale DOM refs. Rebuild from fresh refs starting at the first word
        // past lastRevealedIndex, which includes any the previous chunk
        // enqueued but hadn't released yet.
        revealState.queue = [];
        var revealStart = Math.max(0, revealState.lastRevealedIndex + 1);
        for (var rv = revealStart; rv < newWordCount; rv++) {{
            var rw = words[rv];
            if (!rw) continue;
            rw.style.visibility = 'hidden';
            rw.style.opacity = '0';
            rw.style.filter = 'blur(3px)';
            rw.style.transform = 'translateY(14px)';
            rw.style.transition = 'opacity 0.35s ease-out, filter 0.35s ease-out, transform 0.4s cubic-bezier(0.16, 1, 0.3, 1)';
            revealState.queue.push({{ el: rw, index: rv }});
        }}

        if (revealState.queue.length > 0 && !revealState.active) {{
            revealState.active = true;
            revealState.lastTick = performance.now();
            // Prime with 1 credit so the very first word releases on the
            // first tick without waiting for credit accumulation.
            revealState.credits = 1;
            var SMOOTH_WPS = 40;      // ~25ms/word at zero backlog
            var CATCH_THRESHOLD = 10; // doubles rate per +10 backlog
            var BATCH_CAP = 64;       // hardware safety ceiling per frame
            var tick = function(now) {{
                var q = revealState.queue;
                if (!q || q.length === 0) {{
                    revealState.active = false;
                    revealState.credits = 0;
                    return;
                }}
                var dt = now - revealState.lastTick;
                if (dt < 0) dt = 0;
                revealState.lastTick = now;
                var backlog = q.length;
                // targetWps scales linearly with backlog: at 0 backlog we
                // release at SMOOTH_WPS; each CATCH_THRESHOLD units of
                // backlog doubles the effective rate.
                var targetWps = SMOOTH_WPS * (1 + backlog / CATCH_THRESHOLD);
                revealState.credits += targetWps * dt / 1000;
                var emitted = 0;
                while (revealState.credits >= 1 && q.length > 0 && emitted < BATCH_CAP) {{
                    var item = q.shift();
                    if (item.el && item.el.isConnected) {{
                        // Word is already in layout (visibility:hidden) —
                        // flip to visible and trigger the transform/opacity
                        // transitions for the rise-from-below reveal.
                        item.el.style.visibility = 'visible';
                        item.el.style.opacity = '1';
                        item.el.style.filter = 'blur(0)';
                        item.el.style.transform = 'translateY(0)';
                    }}
                    revealState.lastRevealedIndex = item.index;
                    revealState.credits -= 1;
                    emitted++;
                }}

                // Stand down while fit's rAF is interpolating — its animation
                // transiently parks body.fontSize at the OLD pre-fit value
                // before easing down to the new target, and scrollHeight
                // spikes there. Reading during that window and shrinking
                // caused an undershoot. With visibility:hidden keeping all
                // queued words in layout, the fit already measures the full
                // final height correctly, so this shrink is a fallback only.
                if (emitted > 0 && !window._sgtFitAnim) {{
                    var doc2 = document.documentElement;
                    var overflowPx = doc2.scrollHeight - window.innerHeight;
                    if (overflowPx > window.innerHeight * 0.05) {{
                        var currentFs = parseFloat(document.body.style.fontSize) || 14;
                        var minFs = ((revealState.lastRevealedIndex + 1) < 200) ? 6 : 14;
                        if (currentFs > minFs) {{
                            // Ratio-based shrink with safety margin: aim for
                            // scrollHeight to land at 88% of winH after the
                            // write, giving headroom for the next burst of
                            // reveals before another refit.
                            var scale = (window.innerHeight / doc2.scrollHeight) * 0.92;
                            var newFs = Math.max(minFs, Math.floor(currentFs * scale));
                            if (newFs < currentFs) {{
                                console.log('[SGT-fit] reveal-tick shrink', currentFs.toFixed(1), '->', newFs, 'scrollH=' + doc2.scrollHeight, 'winH=' + window.innerHeight, 'revealed=' + (revealState.lastRevealedIndex + 1));
                                document.body.style.fontSize = newFs + 'px';
                                window._sgtCurrentFontSize = newFs;
                            }}
                        }}
                    }}
                }}
                requestAnimationFrame(tick);
            }};
            requestAnimationFrame(tick);
        }}
    }}

    if (window.__SGT_REPORT_RENDER_DIAGNOSTICS__) {{
        window.__SGT_REPORT_RENDER_DIAGNOSTICS__({{
            phase: 'stream_update',
            sourceTextLen: sourceTextLen,
            sourceTrimmedLen: sourceTrimmedLen
        }});
    }}

    if (body.style.opacity === '0') {{
        body.style.opacity = '1';
    }}

    // Post-stream overflow watchdog: the reveal tick's inline overflow shrink
    // only runs while the reveal queue has entries. Tables expanding via
    // Grid.js, images loading, or any late reflow AFTER streaming ends can
    // push scrollHeight past winH with no loop alive to catch it — user
    // would then have to hover the overlay to trigger a resize + fit.
    // A persistent ResizeObserver on body runs the same ratio-shrink
    // whenever body resizes post-stream, so the view auto-neutralizes
    // without needing a mouse-enter.
    if (!window._sgtOverflowObserver && typeof ResizeObserver !== 'undefined') {{
        try {{
            var debounceTimer = null;
            var ob = new ResizeObserver(function() {{
                if (debounceTimer) return;
                debounceTimer = setTimeout(function() {{
                    debounceTimer = null;
                    // Stand down while the fit's rAF is interpolating — its
                    // animation transiently parks body.fontSize at the OLD
                    // pre-fit value before easing to the new (smaller)
                    // target. scrollHeight spikes there because the fit
                    // already committed the full final content but body
                    // is still visually at the pre-fit size. Reading
                    // scrollHeight during that window and panic-shrinking
                    // caused the "too small after streaming" undershoot.
                    if (window._sgtFitAnim) return;
                    try {{
                        var doc = document.documentElement;
                        var winH = window.innerHeight;
                        var overflowPx = doc.scrollHeight - winH;
                        if (overflowPx <= winH * 0.05) return;
                        var revealed = (window._streamRevealState && typeof window._streamRevealState.lastRevealedIndex === 'number')
                            ? (window._streamRevealState.lastRevealedIndex + 1)
                            : 0;
                        var cFs = parseFloat(document.body.style.fontSize) || 14;
                        var minFs = (revealed > 0 && revealed < 200) ? 6 : 14;
                        if (cFs <= minFs) return;
                        var scale = (winH / doc.scrollHeight) * 0.92;
                        var nFs = Math.max(minFs, Math.floor(cFs * scale));
                        if (nFs >= cFs) return;
                        console.log('[SGT-fit] RO shrink', cFs.toFixed(1), '->', nFs, 'scrollH=' + doc.scrollHeight, 'winH=' + winH, 'revealed=' + revealed);
                        document.body.style.fontSize = nFs + 'px';
                        window._sgtCurrentFontSize = nFs;
                    }} catch (_e) {{}}
                }}, 120);
            }});
            ob.observe(document.body);
            window._sgtOverflowObserver = ob;
        }} catch (_e) {{}}
    }}

    window._streamWordCount = newWordCount;
    window._streamRenderCount = prevRenderCount + 1;
    window.scrollTo({{ top: document.body.scrollHeight, behavior: scrollBehavior }});
}})()"#,
        escaped_content,
        source_len,
        source_trimmed_len,
        options.run_inline_sizing,
        options.animate_new_words,
        scroll_behavior
    );

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            match webview.evaluate_script(&script) {
                Ok(()) => true,
                Err(err) => {
                    crate::log_info!(
                        "[MarkdownDiag] stream_evaluate_script_failed hwnd={:?} source_len={} source_trimmed_len={} html_len={} err={:?}",
                        parent_hwnd,
                        source_len,
                        source_trimmed_len,
                        body_content.len(),
                        err
                    );
                    false
                }
            }
        } else {
            crate::log_info!(
                "[MarkdownDiag] stream_update_missing_webview hwnd={:?} source_len={} source_trimmed_len={} html_len={}",
                parent_hwnd,
                source_len,
                source_trimmed_len,
                body_content.len()
            );
            false
        }
    })
}

/// Reset the stream content tracker.
pub fn reset_stream_counter(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key)
            && let Err(err) = webview.evaluate_script(
                "window._streamPrevLen = 0; window._streamPrevContent = ''; window._streamWordCount = 0; window._streamRenderCount = 0; if (window._streamRevealState) { window._streamRevealState.queue = []; window._streamRevealState.active = false; window._streamRevealState.lastRevealedIndex = -1; window._streamRevealState.credits = 0; }",
            )
        {
            crate::log_info!(
                "[MarkdownDiag] reset_stream_counter_failed hwnd={:?} err={:?}",
                parent_hwnd,
                err
            );
        }
    });
}

/// Set body opacity (hide before fitting, reveal after).
pub fn set_body_opacity(parent_hwnd: HWND, visible: bool) {
    let hwnd_key = parent_hwnd.0 as isize;
    let opacity = if visible { "1" } else { "0" };
    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key)
            && let Err(err) = webview.evaluate_script(&format!(
                "if(document.body) document.body.style.opacity = '{}';",
                opacity
            ))
        {
            crate::log_info!(
                "[MarkdownDiag] set_body_opacity_failed hwnd={:?} visible={} err={:?}",
                parent_hwnd,
                visible,
                err
            );
        }
    });
}
