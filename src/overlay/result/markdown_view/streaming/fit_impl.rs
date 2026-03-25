use windows::Win32::Foundation::*;

use super::super::WEBVIEWS;

const FIT_FONT_SCRIPT: &str = r#"
(function() {
    const fitPhase = "__FIT_PHASE__";
    const isStreamingFit = __STREAMING_MODE__;

    window._sgtFitCallCount = (window._sgtFitCallCount || 0) + 1;
    if (window._sgtFitting) return;
    window._sgtFitting = true;

    function postFitDiagnostic(payload) {
        try {
            if (window.ipc && typeof window.ipc.postMessage === 'function') {
                window.ipc.postMessage(JSON.stringify(payload));
            }
        } catch (_err) {}
    }

    function revealAndUnlock(bodyRef) {
        try {
            if (bodyRef) {
                bodyRef.style.opacity = '1';
            }
        } finally {
            window._sgtFitting = false;
        }
    }

    function runFitWhenReady() {
        requestAnimationFrame(function() {
            requestAnimationFrame(function() {
                var body = document.body;
                var doc = document.documentElement;

                try {
                    if (!body || !doc) {
                        postFitDiagnostic({
                            action: 'render_diagnostics',
                            phase: fitPhase,
                            reason: 'fit_missing_body',
                            renderMode: 'markdown_fit'
                        });
                        return;
                    }

                    // Skip font fitting for image/audio input adapters - detect by checking for slider-container.
                    // These have special fixed layouts that shouldn't be affected by auto-scaling.
                    if (document.querySelector('.slider-container') || document.querySelector('.audio-player')) {
                        return;
                    }

                    var _fitStart = performance.now();

                    // Force layout recalculation before reading dimensions.
                    void body.offsetHeight;

                    var winH = window.innerHeight;
                    var winW = body.clientWidth || window.innerWidth;

                    // Get content and length early so the fit heuristic can reject pathological wraps.
                    var text = body.innerText || body.textContent || '';
                    var textLen = text.trim().length;

                    function currentLineHeightPx() {
                        var computed = window.getComputedStyle(body);
                        var fontSize = parseFloat(computed.fontSize) || parseFloat(body.style.fontSize) || 14;
                        var lineHeight = parseFloat(computed.lineHeight);
                        if (!Number.isFinite(lineHeight)) {
                            var inlineLineHeight = parseFloat(body.style.lineHeight);
                            lineHeight = fontSize * (Number.isFinite(inlineLineHeight) ? inlineLineHeight : 1.15);
                        }
                        return Math.max(1, lineHeight);
                    }

                    function hasPathologicalWrap() {
                        if (!isStreamingFit || textLen < 8) {
                            return false;
                        }

                        var tokens = text.trim().split(/\s+/).filter(Boolean);
                        var wordCount = tokens.length;
                        var longestToken = 0;
                        for (var i = 0; i < tokens.length; i++) {
                            longestToken = Math.max(longestToken, tokens[i].length);
                        }

                        var approxLineCount = Math.max(
                            1,
                            Math.round(doc.scrollHeight / currentLineHeightPx())
                        );
                        var avgCharsPerLine = textLen / approxLineCount;

                        return avgCharsPerLine < 3.5
                            && approxLineCount > Math.max(3, wordCount + 1)
                            && (wordCount <= 12 || longestToken >= 4);
                    }

                    // Helper: check if content fits (re-reads scrollHeight each time for accuracy).
                    function fits() {
                        void body.offsetHeight;
                        return doc.scrollHeight <= winH && !hasPathologicalWrap();
                    }

                    function getGap() {
                        void body.offsetHeight;
                        return winH - doc.scrollHeight;
                    }

                    // Helper: reset last child margin (used during binary search phases).
                    function clearLastMargin() {
                        var blocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
                        if (blocks.length > 0) {
                            blocks[blocks.length - 1].style.marginBottom = '0';
                        }
                    }

                    var isShortContent = textLen < 1500;
                    var isTinyContent = textLen < 300;
                    var isConstrainedWindow = (winH < 260 || winW < 420);
                    var isConstrainedShortContent = isConstrainedWindow && textLen < 450;

                    // Allowed ranges — match streaming's 14px readability floor.
                    var minSize = (textLen < 200) ? 6 : 14;
                    var maxSize = isStreamingFit
                        ? (isTinyContent
                            ? Math.min(96, winH)
                            : (isShortContent ? Math.min(72, winH) : Math.min(48, winH)))
                        : (isTinyContent
                            ? 200
                            : (isShortContent
                                ? 100
                                : Math.max(24, Math.min(48, Math.floor(winH / 10)))));

                    // ===== PHASE 0: RESET (Start TIGHT like GDI) =====
                    // Long text keeps this compact baseline too, so the final settle-fit
                    // does not snap away from the condensed streaming look.
                    body.style.fontVariationSettings = "'wght' 400, 'wdth' 90, 'slnt' 0, 'ROND' 100";
                    body.style.letterSpacing = '0px';
                    body.style.lineHeight = '1.15';
                    body.style.paddingTop = '0';
                    body.style.paddingBottom = '0';
                    var resetBlocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
                    for (var i = 0; i < resetBlocks.length; i++) {
                        resetBlocks[i].style.marginBottom = '0.15em';
                        resetBlocks[i].style.paddingBottom = '0';
                    }
                    clearLastMargin();

                    // Force reflow after reset to ensure measurements are accurate.
                    void body.offsetHeight;

                    // ===== PHASE 1: FONT SIZE (with tight line-height) =====
                    // Binary search for largest font size that fits.
                    var low = minSize, high = maxSize, bestSize = minSize;
                    var foundFittingSize = false;
                    while (low <= high) {
                        var mid = Math.floor((low + high) / 2);
                        body.style.fontSize = mid + 'px';
                        clearLastMargin();
                        if (fits()) {
                            foundFittingSize = true;
                            bestSize = mid;
                            low = mid + 1;
                        } else {
                            high = mid - 1;
                        }
                    }
                    if (!foundFittingSize) {
                        bestSize = minSize;
                    }
                    body.style.fontSize = bestSize + 'px';
                    clearLastMargin();

                    // Small-window + less-text path: run a settle pass to avoid "almost right" first paint.
                    if (isConstrainedShortContent) {
                        void body.offsetHeight;
                        var settleLow = minSize, settleHigh = bestSize, settleBest = minSize;
                        while (settleLow <= settleHigh) {
                            var settleMid = Math.floor((settleLow + settleHigh) / 2);
                            body.style.fontSize = settleMid + 'px';
                            clearLastMargin();
                            if (fits()) {
                                settleBest = settleMid;
                                settleLow = settleMid + 1;
                            } else {
                                settleHigh = settleMid - 1;
                            }
                        }
                        body.style.fontSize = settleBest + 'px';
                        clearLastMargin();
                    }

                    // ===== PHASE 1.5: CONDENSE OPTIMIZATION (wdth < 90) =====
                    // Dense/tall text can get stuck at small font sizes because wrapping is width-limited.
                    if (textLen > 0 && (bestSize < maxSize - 2 || !foundFittingSize)) {
                        var baseSize = parseFloat(body.style.fontSize) || bestSize;
                        var bestComboSize = baseSize;
                        var bestComboWdth = 90;
                        var bestComboFits = fits();
                        var bestComboOverflow = Math.max(0, doc.scrollHeight - winH);

                        for (var testWdth = 85; testWdth >= 55; testWdth -= 5) {
                            body.style.fontVariationSettings = "'wght' 400, 'wdth' " + testWdth + ", 'slnt' 0, 'ROND' 100";
                            clearLastMargin();

                            var cLow = minSize, cHigh = maxSize, cBest = minSize;
                            var cFoundFit = false;
                            while (cLow <= cHigh) {
                                var cMid = Math.floor((cLow + cHigh) / 2);
                                body.style.fontSize = cMid + 'px';
                                clearLastMargin();
                                if (fits()) {
                                    cFoundFit = true;
                                    cBest = cMid;
                                    cLow = cMid + 1;
                                } else {
                                    cHigh = cMid - 1;
                                }
                            }
                            if (!cFoundFit) {
                                cBest = minSize;
                                body.style.fontSize = cBest + 'px';
                                clearLastMargin();
                            }
                            var cFits = fits();
                            var cOverflow = Math.max(0, doc.scrollHeight - winH);

                            if (
                                (!bestComboFits && cFits)
                                || (bestComboFits && cFits && cBest > bestComboSize)
                                || (!bestComboFits && !cFits && (cOverflow < bestComboOverflow || (cOverflow === bestComboOverflow && testWdth > bestComboWdth)))
                            ) {
                                bestComboSize = cBest;
                                bestComboWdth = testWdth;
                                bestComboFits = cFits;
                                bestComboOverflow = cOverflow;
                            }
                        }

                        body.style.fontVariationSettings = "'wght' 400, 'wdth' " + bestComboWdth + ", 'slnt' 0, 'ROND' 100";
                        body.style.fontSize = bestComboSize + 'px';
                        clearLastMargin();
                    }

                    // ===== PHASES 2-7: gap filling =====
                    // During active streaming, skip the expansion passes entirely.
                    // They can stretch small partial chunks into narrow vertical columns.
                    if (isShortContent && !isStreamingFit) {
                        // ===== PHASE 2: LINE HEIGHT =====
                        if (fits() && getGap() > 2) {
                            var lowLH = 1.15, highLH = 2.5, bestLH = 1.15;
                            while (highLH - lowLH > 0.01) {
                                var midLH = (lowLH + highLH) / 2;
                                body.style.lineHeight = midLH;
                                clearLastMargin();
                                if (fits()) {
                                    bestLH = midLH;
                                    lowLH = midLH;
                                } else {
                                    highLH = midLH;
                                }
                            }
                            body.style.lineHeight = bestLH;
                            clearLastMargin();
                        }

                        // ===== PHASE 3: BLOCK MARGINS =====
                        if (fits() && getGap() > 2) {
                            var blocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
                            var lowM = 0, highM = 3.0, bestM = 0;
                            while (highM - lowM > 0.02) {
                                var midM = (lowM + highM) / 2;
                                for (var j = 0; j < blocks.length - 1; j++) {
                                    blocks[j].style.marginBottom = midM + 'em';
                                }
                                if (blocks.length > 0) blocks[blocks.length - 1].style.marginBottom = '0';
                                if (fits()) {
                                    bestM = midM;
                                    lowM = midM;
                                } else {
                                    highM = midM;
                                }
                            }
                            for (var k = 0; k < blocks.length - 1; k++) {
                                blocks[k].style.marginBottom = bestM + 'em';
                            }
                            if (blocks.length > 0) blocks[blocks.length - 1].style.marginBottom = '0';
                        }

                        // ===== PHASE 4: FONT SIZE MICRO-ADJUST =====
                        if (fits() && getGap() > 5) {
                            var currentSize = parseFloat(body.style.fontSize) || bestSize;
                            var testSize = currentSize;
                            while (testSize < maxSize) {
                                testSize += 0.5;
                                body.style.fontSize = testSize + 'px';
                                clearLastMargin();
                                if (!fits()) {
                                    body.style.fontSize = (testSize - 0.5) + 'px';
                                    clearLastMargin();
                                    break;
                                }
                            }
                        }

                        // ===== PHASE 5: LETTER SPACING =====
                        if (fits() && getGap() > 2) {
                            var lowLS = 0, highLS = 20, bestLS = 0;
                            while (highLS - lowLS > 0.1) {
                                var midLS = (lowLS + highLS) / 2;
                                body.style.letterSpacing = midLS + 'px';
                                clearLastMargin();
                                if (fits()) {
                                    bestLS = midLS;
                                    lowLS = midLS;
                                } else {
                                    highLS = midLS;
                                }
                            }
                            body.style.letterSpacing = bestLS + 'px';
                            clearLastMargin();
                        }

                        // ===== PHASE 6: FONT WIDTH (wdth) =====
                        if (fits() && getGap() > 2) {
                            var lowW = 90, highW = 150, bestW = 90;
                            while (lowW <= highW) {
                                var midW = Math.floor((lowW + highW) / 2);
                                body.style.fontVariationSettings = "'wght' 400, 'wdth' " + midW + ", 'slnt' 0, 'ROND' 100";
                                clearLastMargin();
                                if (fits()) {
                                    bestW = midW;
                                    lowW = midW + 1;
                                } else {
                                    highW = midW - 1;
                                }
                            }
                            body.style.fontVariationSettings = "'wght' 400, 'wdth' " + bestW + ", 'slnt' 0, 'ROND' 100";
                            clearLastMargin();
                        }

                        // ===== PHASE 7: HORIZONTAL FILL =====
                        var fontSize = parseFloat(body.style.fontSize) || 14;
                        var lineH = parseFloat(body.style.lineHeight) || 1.5;
                        var approxLineHeight = fontSize * lineH;
                        var isFewLines = doc.scrollHeight < approxLineHeight * 3;

                        if (fits() && isFewLines) {
                            var lowWFew = 90, highWFew = 500, bestWFew = 90;
                            var baseHeight = doc.scrollHeight;
                            while (lowWFew <= highWFew) {
                                var midWFew = Math.floor((lowWFew + highWFew) / 2);
                                body.style.fontVariationSettings = "'wght' 400, 'wdth' " + midWFew + ", 'slnt' 0, 'ROND' 100";
                                if (doc.scrollHeight <= baseHeight && fits()) {
                                    bestWFew = midWFew;
                                    lowWFew = midWFew + 1;
                                } else {
                                    highWFew = midWFew - 1;
                                }
                            }
                            body.style.fontVariationSettings = "'wght' 400, 'wdth' " + bestWFew + ", 'slnt' 0, 'ROND' 100";

                            baseHeight = doc.scrollHeight;
                            var lowLSFew = 0, highLSFew = 100, bestLSFew = 0;
                            while (highLSFew - lowLSFew > 0.5) {
                                var midLSFew = (lowLSFew + highLSFew) / 2;
                                body.style.letterSpacing = midLSFew + 'px';
                                if (doc.scrollHeight <= baseHeight && fits()) {
                                    bestLSFew = midLSFew;
                                    lowLSFew = midLSFew;
                                } else {
                                    highLSFew = midLSFew;
                                }
                            }
                            body.style.letterSpacing = bestLSFew + 'px';
                        }
                    }

                    // ===== PHASE 8: OVERFLOW RESCUE CONDENSE =====
                    if (!fits()) {
                        var rescueSize = Math.max(minSize, parseFloat(body.style.fontSize) || minSize);
                        body.style.fontSize = rescueSize + 'px';
                        body.style.letterSpacing = '0px';
                        clearLastMargin();

                        var rescueBestWdth = 90;
                        var rescueBestOverflow = Math.max(0, doc.scrollHeight - winH);
                        for (var rescueWdth = 90; rescueWdth >= 45; rescueWdth -= 5) {
                            body.style.fontVariationSettings = "'wght' 400, 'wdth' " + rescueWdth + ", 'slnt' 0, 'ROND' 100";
                            clearLastMargin();
                            var rescueOverflow = Math.max(0, doc.scrollHeight - winH);
                            if (rescueOverflow < rescueBestOverflow || (rescueOverflow === rescueBestOverflow && rescueWdth > rescueBestWdth)) {
                                rescueBestOverflow = rescueOverflow;
                                rescueBestWdth = rescueWdth;
                            }
                        }
                        body.style.fontVariationSettings = "'wght' 400, 'wdth' " + rescueBestWdth + ", 'slnt' 0, 'ROND' 100";
                        clearLastMargin();
                    }

                    // ===== FINAL: Fill any remaining gap by distributing space =====
                    var finalGap = winH - doc.scrollHeight;
                    if (!isStreamingFit && finalGap > 2) {
                        body.style.paddingTop = Math.floor(finalGap * 0.3) + 'px';
                        body.style.paddingBottom = Math.floor(finalGap * 0.7) + 'px';
                    } else {
                        body.style.paddingTop = '0';
                        body.style.paddingBottom = '0';
                    }

                    // Debug telemetry for runtime font-axis behavior and final fit result.
                    try {
                        if (window.__SGT_FIT_DEBUG__ === undefined) {
                            window.__SGT_FIT_DEBUG__ = false;
                        }
                        if (window.__SGT_FIT_DEBUG__ && window.ipc && typeof window.ipc.postMessage === 'function') {
                            var cs = window.getComputedStyle(body);
                            var probe = document.createElement('span');
                            probe.textContent = 'MMMMMMMMMMMMMMMMMMMM';
                            probe.style.position = 'absolute';
                            probe.style.visibility = 'hidden';
                            probe.style.pointerEvents = 'none';
                            probe.style.whiteSpace = 'nowrap';
                            probe.style.fontFamily = cs.fontFamily;
                            probe.style.fontSize = cs.fontSize;
                            probe.style.fontWeight = cs.fontWeight;
                            probe.style.lineHeight = cs.lineHeight;
                            document.body.appendChild(probe);
                            probe.style.fontVariationSettings = "'wght' 400, 'wdth' 90, 'slnt' 0, 'ROND' 100";
                            var widthAt90 = probe.getBoundingClientRect().width;
                            probe.style.fontVariationSettings = "'wght' 400, 'wdth' 55, 'slnt' 0, 'ROND' 100";
                            var widthAt55 = probe.getBoundingClientRect().width;
                            if (probe.parentNode) probe.parentNode.removeChild(probe);

                            var payload = {
                                action: 'fit_debug',
                                phase: fitPhase,
                                textLen: textLen,
                                winH: winH,
                                winW: winW,
                                scrollH: doc.scrollHeight,
                                finalGap: finalGap,
                                computedFontFamily: cs.fontFamily,
                                computedFontSize: cs.fontSize,
                                computedFontStretch: cs.fontStretch,
                                computedFontVariationSettings: cs.fontVariationSettings,
                                bodyStyleFontVariationSettings: body.style.fontVariationSettings || '',
                                letterSpacing: cs.letterSpacing,
                                lineHeight: cs.lineHeight,
                                googleSansFlexReady: (document.fonts && document.fonts.check)
                                    ? document.fonts.check("16px 'Google Sans Flex'")
                                    : null,
                                documentFontsStatus: (document.fonts && document.fonts.status) ? document.fonts.status : null,
                                probeWidthAtWdth90: widthAt90,
                                probeWidthAtWdth55: widthAt55,
                                probeWdthDelta: widthAt90 - widthAt55,
                                fitDurationMs: performance.now() - _fitStart,
                                fitCallCount: window._sgtFitCallCount || 0,
                                streamingFit: isStreamingFit
                            };
                            window.ipc.postMessage(JSON.stringify(payload));
                        }
                    } catch (_err) {}
                } catch (err) {
                    var renderedText = body ? ((body.innerText || body.textContent || '').trim()) : '';
                    postFitDiagnostic({
                        action: 'render_diagnostics',
                        phase: fitPhase,
                        reason: isStreamingFit ? 'streaming_fit_exception' : 'fit_exception',
                        renderMode: 'markdown_fit',
                        renderedTextLen: renderedText.length,
                        bodyHtmlLen: body ? body.innerHTML.length : 0,
                        bodyChildCount: body ? body.children.length : 0,
                        error: err && err.message ? err.message : String(err)
                    });
                } finally {
                    try {
                        if (window.__SGT_REPORT_RENDER_DIAGNOSTICS__) {
                            window.__SGT_REPORT_RENDER_DIAGNOSTICS__({ phase: fitPhase });
                        }
                    } catch (_err) {}
                    revealAndUnlock(body);
                }
            });
        });
    }

    try {
        var fontReady = !document.fonts || document.fonts.check('400 16px "Google Sans Flex"');
        if (fontReady) {
            runFitWhenReady();
        } else {
            document.fonts.load('400 16px "Google Sans Flex"').then(runFitWhenReady, runFitWhenReady);
        }
    } catch (_err) {
        runFitWhenReady();
    }
})();
"#;

pub fn fit_font_to_window(parent_hwnd: HWND) {
    fit_font_to_window_ex(parent_hwnd, false);
}

fn fit_font_to_window_ex(parent_hwnd: HWND, streaming_mode: bool) {
    let hwnd_key = parent_hwnd.0 as isize;
    let phase = if streaming_mode {
        "fit_font_to_window_streaming"
    } else {
        "fit_font_to_window_final"
    };
    let script = FIT_FONT_SCRIPT.replace("__FIT_PHASE__", phase).replace(
        "__STREAMING_MODE__",
        if streaming_mode { "true" } else { "false" },
    );

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key)
            && let Err(err) = webview.evaluate_script(&script)
        {
            crate::log_info!(
                "[MarkdownDiag] fit_evaluate_script_failed hwnd={:?} phase={} err={:?}",
                parent_hwnd,
                phase,
                err
            );
        }
    });
}

/// Trigger Grid.js initialization on any tables in the WebView.
/// Call this after streaming ends to convert tables to interactive Grid.js tables.
pub fn init_gridjs(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            let script = r#"
                (function() {
                    if (typeof gridjs === 'undefined') return;

                    var tables = document.querySelectorAll('table:not(.gridjs-table):not([data-processed-table="true"])');
                    for (var i = 0; i < tables.length; i++) {
                        var table = tables[i];
                        if (table.closest('.gridjs-container') || table.closest('.gridjs-injected-wrapper')) continue;

                        table.setAttribute('data-processed-table', 'true');

                        var wrapper = document.createElement('div');
                        wrapper.className = 'gridjs-injected-wrapper';
                        table.parentNode.insertBefore(wrapper, table);

                        try {
                            var grid = new gridjs.Grid({
                                from: table,
                                sort: true,
                                fixedHeader: true,
                                search: false,
                                resizable: false,
                                autoWidth: false,
                                style: {
                                    table: { 'width': '100%' },
                                    td: { 'border': '1px solid #333' },
                                    th: { 'border': '1px solid #333' }
                                },
                                className: {
                                    table: 'gridjs-table-premium',
                                    th: 'gridjs-th-premium',
                                    td: 'gridjs-td-premium'
                                }
                            });
                            grid.on('ready', function() {
                                table.classList.add('gridjs-hidden-source');
                            });
                            grid.render(wrapper);
                        } catch (e) {
                            console.error('Grid.js streaming init error:', e);
                            if(wrapper.parentNode) wrapper.parentNode.removeChild(wrapper);
                        }
                    }
                })();
            "#;
            if let Err(err) = webview.evaluate_script(script) {
                crate::log_info!(
                    "[MarkdownDiag] gridjs_init_failed hwnd={:?} err={:?}",
                    parent_hwnd,
                    err
                );
            }
        }
    });
}
