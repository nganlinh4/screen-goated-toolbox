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

                    // Use textContent (not innerText) so visibility:hidden
                    // words contribute to length/wordcount heuristics. During
                    // streaming the reveal queue holds words at
                    // visibility:hidden — they still take layout space
                    // (scrollHeight reflects them) but innerText excludes
                    // them, which made hasPathologicalWrap trip on the
                    // mismatch between "few visible chars" and "many laid
                    // out lines" and forced bestSize down to minSize=6.
                    var text = (body.textContent || '').trim();
                    var textLen = text.length;

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

                    function getHorizontalOverflow() {
                        void body.offsetHeight;
                        var scrollW = Math.max(
                            doc.scrollWidth || 0,
                            body.scrollWidth || 0
                        );
                        return scrollW - Math.max(1, winW);
                    }

                    // Helper: check if content fits (re-reads scrollHeight each time for accuracy).
                    function fits() {
                        void body.offsetHeight;
                        return doc.scrollHeight <= winH
                            && getHorizontalOverflow() <= 1
                            && !hasPathologicalWrap();
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
                    // Streaming cap is deliberately conservative (48px). An
                    // early tiny chunk could otherwise be sized up to 96
                    // and then forced to climb down a long shrink ladder
                    // (110 -> 60 -> 44 -> 32) as the response grows. The
                    // final (non-streaming) fit keeps the full range so
                    // short final responses can still display large.
                    var maxSize = isStreamingFit
                        ? Math.min(48, winH)
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

                    // ===== STREAMING HYSTERESIS =====
                    // If the prior size still produces a layout within a
                    // tolerance band of winH, skip the refit entirely — no
                    // new target, no animation, nothing visually changes.
                    // This is the "predictable, careful" behavior: chunks 2-N
                    // mostly inherit chunk 1's size, and the user sees one
                    // gentle settle at the end instead of N progressive
                    // shrinks. We only trigger a refit when overflow is
                    // meaningfully wrong (> 12% over winH) or when content
                    // is drastically under-filled (> 40% whitespace, which
                    // means prior size is way too small for current content
                    // — rare but happens if the response ended up compact).
                    if (isStreamingFit && hasPriorFontSize && priorDisplayedFontSize >= minSize) {
                        body.style.fontSize = priorDisplayedFontSize + 'px';
                        void body.offsetHeight;
                        var hystScrollH = doc.scrollHeight;
                        var hystOverRatio = hystScrollH / winH;
                        if (hystOverRatio <= 1.12 && hystOverRatio >= 0.60) {
                            return;
                        }
                    }

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
                    // Skip during streaming — the per-chunk condense search
                    // was finding narrower combos that forced bestSize down
                    // (e.g. wdth=85 → bestSize=32 instead of the wdth=90
                    // Phase-1 result of 40), producing the streaming-vs-final
                    // size disagreement. The final fit still runs condense
                    // so the settled state gets the benefit.
                    if (!isStreamingFit && !preservedSize && textLen > 0 && (bestSize < maxSize - 2 || !foundFittingSize)) {
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

                    // Legacy "15% shrink + 4px bucket" block removed. It was
                    // making streaming land 15% smaller than the actual
                    // best fit (e.g. 38 -> 32), so the final fit would
                    // then jump UP to the correct value — exactly the
                    // streaming-vs-final disagreement the logs exposed.
                    // The fit-entry hysteresis (tolerates 12% overflow
                    // before refitting) already provides growth headroom,
                    // so this shrink is redundant. Streaming now keeps
                    // Phase-1's true best size, matching final fit.

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
