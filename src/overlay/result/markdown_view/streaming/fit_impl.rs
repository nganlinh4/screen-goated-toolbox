use windows::Win32::Foundation::*;

use super::super::WEBVIEWS;

const FIT_FONT_SCRIPT: &str = r#"
(function() {
    const fitPhase = "__FIT_PHASE__";
    const isStreamingFit = __STREAMING_MODE__;

    window._sgtFitCallCount = (window._sgtFitCallCount || 0) + 1;
    if (window._sgtFitting) return;
    window._sgtFitting = true;

    // Cancel any in-flight smoothing animation so this fit can retarget from
    // the currently-displayed axes without two animations fighting. Binary
    // search below writes body.fontSize synchronously for each probe and
    // reads scrollHeight — we need no CSS transition and no rAF driver
    // mutating the same values concurrently.
    if (window._sgtFitAnim) {
        try { cancelAnimationFrame(window._sgtFitAnim); } catch (_e) {}
        window._sgtFitAnim = null;
    }
    if (typeof window._sgtCurrentWdth !== 'number') {
        window._sgtCurrentWdth = 90;
    }
    // _sgtCurrentFontSize is intentionally left undefined on the first fit so
    // that fit snaps to its target (nothing to ease from yet).

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

                    // Helper: body wdth is driven via font-stretch (inherits to
                    // headings), not via variation-settings.
                    function applyBodyWdth(w) {
                        body.style.fontStretch = w + '%';
                    }

                    // Short-circuit redundant final fits. Window activate/deactivate
                    // can re-trigger fit_font_to_window even when text, window size,
                    // and committed axes are unchanged — wasted ~100ms each time.
                    if (!isStreamingFit) {
                        var lastFinal = window._sgtLastFinalFit;
                        var cachedFs = parseFloat(body.style.fontSize);
                        var cachedStretch = parseFloat(body.style.fontStretch);
                        if (lastFinal
                            && lastFinal.textLen === textLen
                            && lastFinal.winW === winW
                            && lastFinal.winH === winH
                            && Number.isFinite(cachedFs)
                            && Math.abs(lastFinal.fontSize - cachedFs) < 0.5
                            && Math.abs((lastFinal.fontStretch || 90) - (Number.isFinite(cachedStretch) ? cachedStretch : 90)) < 0.5) {
                            return;
                        }
                    }

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

                    // Capture currently-displayed axes BEFORE Phase 0 resets them.
                    // Using body.style (not window state) is robust to cross-script-
                    // context resets that can clear window globals between streaming
                    // and final fits. This is also the value the user currently SEES,
                    // which is what the ease-out animation needs to start from.
                    var priorDisplayedFontSize = parseFloat(body.style.fontSize);
                    var priorDisplayedWdth = parseFloat(body.style.fontStretch);
                    var priorDisplayedPadTop = parseFloat(body.style.paddingTop) || 0;
                    var priorDisplayedPadBottom = parseFloat(body.style.paddingBottom) || 0;
                    var hasPriorFontSize = Number.isFinite(priorDisplayedFontSize) && priorDisplayedFontSize > 0;
                    var hasPriorWdth = Number.isFinite(priorDisplayedWdth) && priorDisplayedWdth > 0;

                    // ===== PHASE 0: RESET (Start TIGHT like GDI) =====
                    // Long text keeps this compact baseline too, so the final settle-fit
                    // does not snap away from the condensed streaming look.
                    applyBodyWdth(90);
                    body.style.letterSpacing = '0px';
                    body.style.lineHeight = '1.15';
                    body.style.paddingTop = '0';
                    body.style.paddingBottom = '0';
                    // Headings (h1/h2/h3) keep their CSS-designed margins — the
                    // CSS has deliberate values (12px, 0.5em, 0.4em) that make
                    // headings visually distinct. Overriding them to 0.15em here
                    // caused "big→small" spacing blinks between chunks because
                    // fresh HTML has CSS defaults until Phase 0 runs.
                    var resetBlocks = body.querySelectorAll('p, li, blockquote');
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

                    // Streaming stability: before searching, try the previously-displayed
                    // font size. If the new chunk still fits, keep the size — avoids the
                    // tiny per-chunk reflows that cause the wrap-alternation eyesore.
                    // Each refit then applies hysteresis (below) so several subsequent
                    // chunks fit without forcing another reflow.
                    var preservedSize = false;
                    if (isStreamingFit && hasPriorFontSize && priorDisplayedFontSize >= minSize) {
                        body.style.fontSize = priorDisplayedFontSize + 'px';
                        clearLastMargin();
                        if (fits()) {
                            bestSize = priorDisplayedFontSize;
                            foundFittingSize = true;
                            preservedSize = true;
                        }
                    }

                    if (!preservedSize) {
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
                    }

                    // Small-window + less-text path: run a settle pass to avoid "almost right" first paint.
                    if (isConstrainedShortContent && !preservedSize) {
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
                    if (!preservedSize && textLen > 0 && (bestSize < maxSize - 2 || !foundFittingSize)) {
                        var baseSize = parseFloat(body.style.fontSize) || bestSize;
                        var bestComboSize = baseSize;
                        var bestComboWdth = 90;
                        var bestComboFits = fits();
                        var bestComboOverflow = Math.max(0, doc.scrollHeight - winH);

                        for (var testWdth = 85; testWdth >= 55; testWdth -= 5) {
                            applyBodyWdth(testWdth);
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

                        applyBodyWdth(bestComboWdth);
                        body.style.fontSize = bestComboSize + 'px';
                        clearLastMargin();
                    }

                    // ===== HYSTERESIS + QUANTIZATION: stabilize streaming size =====
                    // Binary search lands on the TIGHTEST fitting size; adding a few
                    // chars would overflow and trigger another refit. Two steps:
                    //   (1) Shrink 15% past strict-fit so scrollHeight lands under the
                    //       window, giving several chunks of growth headroom.
                    //   (2) Snap down to a 4-px bucket so refits differ by >=4 px.
                    //       Kills 1-3 px reflows that reshape wrapping for almost no
                    //       visible zoom — the core "wrap-alternation" eyesore.
                    // Skipped on final fits (user gets largest readable size) and
                    // when we preserved the previous size (no refit happened).
                    if (isStreamingFit && !preservedSize && foundFittingSize) {
                        var currentBest = parseFloat(body.style.fontSize) || bestSize;
                        var bucketPx = 4;
                        var hysteresisSize = currentBest * 0.85;
                        hysteresisSize = Math.floor(hysteresisSize / bucketPx) * bucketPx;
                        hysteresisSize = Math.max(minSize, hysteresisSize);
                        if (hysteresisSize < currentBest) {
                            bestSize = hysteresisSize;
                            body.style.fontSize = bestSize + 'px';
                            clearLastMargin();
                        }
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
                                applyBodyWdth(midW);
                                clearLastMargin();
                                if (fits()) {
                                    bestW = midW;
                                    lowW = midW + 1;
                                } else {
                                    highW = midW - 1;
                                }
                            }
                            applyBodyWdth(bestW);
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
                                applyBodyWdth(midWFew);
                                if (doc.scrollHeight <= baseHeight && fits()) {
                                    bestWFew = midWFew;
                                    lowWFew = midWFew + 1;
                                } else {
                                    highWFew = midWFew - 1;
                                }
                            }
                            applyBodyWdth(bestWFew);

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
                            applyBodyWdth(rescueWdth);
                            clearLastMargin();
                            var rescueOverflow = Math.max(0, doc.scrollHeight - winH);
                            if (rescueOverflow < rescueBestOverflow || (rescueOverflow === rescueBestOverflow && rescueWdth > rescueBestWdth)) {
                                rescueBestOverflow = rescueOverflow;
                                rescueBestWdth = rescueWdth;
                            }
                        }
                        applyBodyWdth(rescueBestWdth);
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
                            probe.style.fontStretch = '90%';
                            var widthAt90 = probe.getBoundingClientRect().width;
                            probe.style.fontStretch = '55%';
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

                    // ===== FONT-SIZE + WDTH SMOOTHING =====
                    // Binary-search lands on a per-chunk target for each axis, which
                    // pops between streaming fits. Ease from the currently-displayed
                    // values toward the new targets so later chunk-to-chunk jitter
                    // doesn't visibly twitch. Final (non-streaming) fits still snap
                    // so the padding distribution above stays accurate.
                    // First-ever fit also snaps — nothing to ease from yet.
                    try {
                        var targetStretch = parseFloat(body.style.fontStretch);
                        var targetWdth = Number.isFinite(targetStretch) && targetStretch > 0 ? targetStretch : 90;
                        var targetFontSize = parseFloat(body.style.fontSize) || 14;
                        var targetPadTop = parseFloat(body.style.paddingTop) || 0;
                        var targetPadBottom = parseFloat(body.style.paddingBottom) || 0;

                        var startWdth = hasPriorWdth ? priorDisplayedWdth : 90;
                        var startFontSize = hasPriorFontSize ? priorDisplayedFontSize : targetFontSize;
                        var startPadTop = priorDisplayedPadTop;
                        var startPadBottom = priorDisplayedPadBottom;
                        var hadPriorSize = hasPriorFontSize;

                        function applyAxes(fs, w) {
                            body.style.fontSize = fs + 'px';
                            body.style.fontStretch = w + '%';
                        }
                        function applyPadding(pt, pb) {
                            body.style.paddingTop = pt + 'px';
                            body.style.paddingBottom = pb + 'px';
                        }

                        // Save signature for the short-circuit at fit entry. Only for
                        // final fits (streaming changes mid-flight and shouldn't cache).
                        if (!isStreamingFit) {
                            window._sgtLastFinalFit = {
                                textLen: textLen,
                                winW: winW,
                                winH: winH,
                                fontSize: targetFontSize,
                                fontStretch: targetWdth
                            };
                        }

                        // Smoothly animate from the visually-displayed value
                        // (captured pre-fit as priorDisplayedFontSize) to the
                        // computed target. Binary search above wrote many
                        // probe values to body.fontSize synchronously and ended
                        // at targetFontSize; we now rewind to startFontSize and
                        // drive a clean interpolation to target. No CSS
                        // transition is active (removed) so measurements in
                        // future fits read whatever we set here exactly.
                        //
                        // Final (mouse-enter / settle) fits get a longer,
                        // more pronounced duration — they usually involve a
                        // bigger delta (streaming hysteresis'd size → final
                        // max-fit size after resize) and deserve to be
                        // visibly smooth.
                        var fsDelta = Math.abs(targetFontSize - startFontSize);
                        var wDelta = Math.abs(targetWdth - startWdth);
                        var duration = isStreamingFit ? 280 : 420;
                        var snapThreshold = isStreamingFit ? 0.5 : 0.15;
                        var snapWThreshold = isStreamingFit ? 1.5 : 0.5;
                        if (!hadPriorSize || (fsDelta < snapThreshold && wDelta < snapWThreshold)) {
                            // First fit, or nothing meaningful changed — snap.
                            applyAxes(targetFontSize, targetWdth);
                            applyPadding(targetPadTop, targetPadBottom);
                            window._sgtCurrentFontSize = targetFontSize;
                            window._sgtCurrentWdth = targetWdth;
                        } else {
                            applyAxes(startFontSize, startWdth);
                            applyPadding(startPadTop, startPadBottom);
                            var animStart = performance.now();
                            var tick = function(now) {
                                var t = Math.min(1, (now - animStart) / duration);
                                var eased = 1 - Math.pow(1 - t, 3);
                                var curFs = startFontSize + (targetFontSize - startFontSize) * eased;
                                var curW = startWdth + (targetWdth - startWdth) * eased;
                                var curPT = startPadTop + (targetPadTop - startPadTop) * eased;
                                var curPB = startPadBottom + (targetPadBottom - startPadBottom) * eased;
                                applyAxes(curFs, curW);
                                applyPadding(curPT, curPB);
                                window._sgtCurrentFontSize = curFs;
                                window._sgtCurrentWdth = curW;
                                if (t < 1) {
                                    window._sgtFitAnim = requestAnimationFrame(tick);
                                } else {
                                    window._sgtFitAnim = null;
                                }
                            };
                            window._sgtFitAnim = requestAnimationFrame(tick);
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

pub fn fit_font_to_window_streaming(parent_hwnd: HWND) {
    fit_font_to_window_ex(parent_hwnd, true);
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
                    // Post-Grid.js shrink: when the last grid fires 'ready' the
                    // real table layout is finally committed to scrollHeight.
                    // The final fit that ran moments ago measured the raw
                    // <table> (before Grid.js styling inflated it), so its
                    // target font-size can overshoot the viewport by the
                    // time the styled grid is in flow. Count pending grids,
                    // run a ratio-shrink once they're all ready.
                    var pendingGrids = 0;
                    function afterGridReady() {
                        pendingGrids -= 1;
                        if (pendingGrids > 0) return;
                        try {
                            var doc = document.documentElement;
                            var winH = window.innerHeight;
                            var overflowPx = doc.scrollHeight - winH;
                            if (overflowPx <= winH * 0.05) return;
                            var cFs = parseFloat(document.body.style.fontSize) || 14;
                            // After final fit we're past streaming — use the
                            // non-streaming floor of 14 for readability.
                            var minFs = 14;
                            if (cFs <= minFs) return;
                            var scale = (winH / doc.scrollHeight) * 0.92;
                            var nFs = Math.max(minFs, Math.floor(cFs * scale));
                            if (nFs >= cFs) return;
                            if (window._sgtFitAnim) {
                                try { cancelAnimationFrame(window._sgtFitAnim); } catch (_e) {}
                                window._sgtFitAnim = null;
                            }
                            document.body.style.fontSize = nFs + 'px';
                            window._sgtCurrentFontSize = nFs;
                        } catch (_e) {}
                    }

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
                            pendingGrids += 1;
                            (function(capturedTable) {
                                grid.on('ready', function() {
                                    capturedTable.classList.add('gridjs-hidden-source');
                                    // Wait one extra frame so the grid's
                                    // final layout is actually in flow
                                    // before we measure scrollHeight.
                                    requestAnimationFrame(afterGridReady);
                                });
                            })(table);
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
