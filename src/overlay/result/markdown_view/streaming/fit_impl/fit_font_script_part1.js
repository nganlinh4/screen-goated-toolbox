(function() {
    const fitPhase = "__FIT_PHASE__";
    const isStreamingFit = __STREAMING_MODE__;
    const fitContext = window.__SGT_FIT_CONTEXT__ || null;
    const fitState = fitContext ? fitContext.state : window;
    const fitBody = fitContext ? fitContext.body : document.body;
    const fitViewport = fitContext ? fitContext.viewport : null;
    const settleBeforeReveal = Boolean(fitContext && fitContext.settleBeforeReveal);
    const scheduleFitFrame = fitContext && typeof fitContext.scheduleFrame === 'function'
        ? fitContext.scheduleFrame
        : requestAnimationFrame.bind(window);
    const cancelFitFrame = fitContext && typeof fitContext.cancelFrame === 'function'
        ? fitContext.cancelFrame
        : cancelAnimationFrame.bind(window);
    const fitDocument = fitContext ? {
        get scrollHeight() { return fitBody ? fitBody.scrollHeight : 0; },
        get scrollWidth() { return fitBody ? fitBody.scrollWidth : 0; }
    } : document.documentElement;

    fitState._sgtFitCallCount = (fitState._sgtFitCallCount || 0) + 1;
    if (fitState._sgtFitting) return;
    fitState._sgtFitting = true;

    if (typeof fitState._sgtCurrentWdth !== 'number') {
        fitState._sgtCurrentWdth = 90;
    }
    // _sgtCurrentFontSize is intentionally left undefined on the first fit so
    // that fit snaps to its target (nothing to ease from yet).

    function postFitDiagnostic(payload) {
        try {
            if (fitContext && typeof fitContext.reportDiagnostic === 'function') {
                fitContext.reportDiagnostic(payload);
            } else if (window.parent && window.parent !== window) {
                window.parent.postMessage({
                    type: 'fit_diagnostic',
                    payload: payload
                }, '*');
            } else if (window.ipc && typeof window.ipc.postMessage === 'function') {
                window.ipc.postMessage(JSON.stringify(payload));
            }
        } catch (_err) {}
    }

    function revealAndUnlock(bodyRef) {
        try {
            if (bodyRef) {
                if (settleBeforeReveal) {
                    bodyRef.style.setProperty('opacity', '1', 'important');
                } else {
                    bodyRef.style.opacity = '1';
                }
            }
        } finally {
            fitState._sgtFitting = false;
            try {
                if (fitContext && typeof fitContext.complete === 'function') {
                    fitContext.complete();
                } else if (window.parent && window.parent !== window) {
                    window.parent.postMessage({ type: 'fit_complete' }, '*');
                }
            } catch (_err) {}
        }
    }

    function runFitWhenReady() {
        scheduleFitFrame(function() {
            scheduleFitFrame(function() {
                var body = fitBody;
                var doc = fitDocument;

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
                    if (body.querySelector('.slider-container') || body.querySelector('.audio-player')) {
                        return;
                    }

                    var _fitStart = performance.now();

                    // Force layout recalculation before reading dimensions.
                    void body.offsetHeight;

                    var winH = fitViewport ? fitViewport.clientHeight : window.innerHeight;
                    var winW = fitViewport ? fitViewport.clientWidth : window.innerWidth;

                    // Count text nodes that can contribute content, including
                    // visibility:hidden streaming words. body.textContent also
                    // includes the inline fitter/bridge source, which has no
                    // layout and made first/final-only renders look like huge
                    // documents and collapse to the minimum font size.
                    function getContentText(root) {
                        var parts = [];
                        var walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
                        var node;
                        while ((node = walker.nextNode())) {
                            var parent = node.parentElement;
                            var tag = parent ? parent.tagName : '';
                            if (tag !== 'SCRIPT' && tag !== 'STYLE' && tag !== 'NOSCRIPT' && tag !== 'TEMPLATE') {
                                parts.push(node.nodeValue || '');
                            }
                        }
                        return parts.join('').trim();
                    }
                    var text = getContentText(body);
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
                        var lastFinal = fitState._sgtLastFinalFit;
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

                    var layoutProbeCount = 0;

                    function readLayoutMetrics() {
                        void body.offsetHeight;
                        layoutProbeCount++;
                        return {
                            height: doc.scrollHeight,
                            width: Math.max(doc.scrollWidth || 0, body.scrollWidth || 0)
                        };
                    }

                    function metricsFit(metrics) {
                        return metrics.height <= winH && metrics.width <= winW + 1;
                    }

                    function getHorizontalOverflow() {
                        return readLayoutMetrics().width - Math.max(1, winW);
                    }

                    // Helper: one forced layout supplies both axes.
                    function fits() {
                        return metricsFit(readLayoutMetrics());
                    }

                    function getGap() {
                        void body.offsetHeight;
                        return winH - doc.scrollHeight;
                    }

                    // Cache the final block once. Re-querying the whole streamed DOM
                    // for every search probe made target calculation scale with chunk size.
                    var fitBlocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
                    var finalFitBlock = fitBlocks.length > 0 ? fitBlocks[fitBlocks.length - 1] : null;

                    // Helper: reset last child margin (used during binary search phases).
                    function clearLastMargin() {
                        if (finalFitBlock) {
                            finalFitBlock.style.marginBottom = '0';
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

                    // A newer fit may be queued immediately after this invocation.
                    // Let the active interpolation paint during our two readiness
                    // frames, then freeze it at the latest displayed value immediately
                    // before measurement mutates the same axes. Cancelling at function
                    // entry starved every streaming animation under a fast response:
                    // queued fits cancelled one another before their first painted
                    // frame, leaving only the final fit able to visibly shrink.
                    if (fitState._sgtFitAnim) {
                        try { cancelFitFrame(fitState._sgtFitAnim); } catch (_e) {}
                        fitState._sgtFitAnim = null;
                    }

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

                    // Streaming target search starts at the previous verified target,
                    // not the lagging displayed value. An area estimate followed by at
                    // most two verified refinements bounds renderer-thread layout work
                    // while the exact final fit retains the exhaustive search.
                    var preservedSize = false;
                    if (isStreamingFit) {
                        var previousTarget = fitState._sgtLastReportedFitTarget;
                        var previousTextLen = fitState._sgtLastStreamingFitTextLen;
                        var contentOnlyGrew = Number.isFinite(previousTextLen)
                            && textLen >= previousTextLen;
                        var searchHigh = maxSize;
                        if (contentOnlyGrew
                            && previousTarget
                            && previousTarget.streaming
                            && Number.isFinite(previousTarget.fontSize)) {
                            searchHigh = Math.max(
                                minSize,
                                Math.min(maxSize, previousTarget.fontSize)
                            );
                        }
                        fitState._sgtLastStreamingFitTextLen = textLen;

                        body.style.fontSize = searchHigh + 'px';
                        clearLastMargin();
                        var highMetrics = readLayoutMetrics();
                        if (metricsFit(highMetrics)) {
                            bestSize = searchHigh;
                            foundFittingSize = true;
                            preservedSize = true;
                        } else {
                            var heightScale = Math.sqrt(winH / Math.max(1, highMetrics.height));
                            var widthScale = winW / Math.max(1, highMetrics.width);
                            var estimateScale = Math.min(1, heightScale, widthScale);
                            var estimate = Math.max(
                                minSize,
                                Math.min(searchHigh - 1, Math.floor(searchHigh * estimateScale))
                            );
                            var searchLow = minSize;
                            var searchUpper = searchHigh - 1;

                            body.style.fontSize = estimate + 'px';
                            clearLastMargin();
                            if (fits()) {
                                foundFittingSize = true;
                                bestSize = estimate;
                                searchLow = estimate + 1;
                            } else {
                                searchUpper = estimate - 1;
                            }

                            var refinementProbe = 0;
                            var MAX_STREAMING_REFINEMENT_PROBES = 2;
                            while (searchLow <= searchUpper
                                && refinementProbe < MAX_STREAMING_REFINEMENT_PROBES) {
                                var refinementSize = Math.floor((searchLow + searchUpper) / 2);
                                body.style.fontSize = refinementSize + 'px';
                                clearLastMargin();
                                if (fits()) {
                                    foundFittingSize = true;
                                    bestSize = refinementSize;
                                    searchLow = refinementSize + 1;
                                } else {
                                    searchUpper = refinementSize - 1;
                                }
                                refinementProbe++;
                            }

                            if (!foundFittingSize) {
                                bestSize = minSize;
                            }
                            body.style.fontSize = bestSize + 'px';
                            clearLastMargin();
                        }
                    } else {
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
                    if (!isStreamingFit && isConstrainedShortContent && !preservedSize) {
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

                    // Streaming keeps Phase 1's true best size. Growth headroom
                    // quantization is intentionally absent because it makes the
                    // streaming target disagree with the identical final DOM.

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
