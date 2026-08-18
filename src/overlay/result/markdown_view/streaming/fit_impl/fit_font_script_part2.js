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
                    if (!isStreamingFit && !foundFittingSize && !fits()) {
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
                    var finalGap = isStreamingFit ? 0 : winH - doc.scrollHeight;
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
                            body.appendChild(probe);
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
                                layoutProbes: layoutProbeCount,
                                fitCallCount: fitState._sgtFitCallCount || 0,
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

                        var lastReportedTarget = fitState._sgtLastReportedFitTarget;
                        var targetChanged = !lastReportedTarget
                            || lastReportedTarget.streaming !== isStreamingFit
                            || Math.abs(lastReportedTarget.fontSize - targetFontSize) >= 0.1
                            || Math.abs(lastReportedTarget.fontStretch - targetWdth) >= 0.3;
                        var paintSampleNow = performance.now();
                        var previousPaintSample = fitState._sgtLastFitPaintSample;
                        var paintedShrinkPxPerSec = 0;
                        if (previousPaintSample
                            && Number.isFinite(priorDisplayedFontSize)
                            && paintSampleNow > previousPaintSample.time) {
                            paintedShrinkPxPerSec = (
                                (previousPaintSample.fontSize - priorDisplayedFontSize)
                                * 1000
                                / (paintSampleNow - previousPaintSample.time)
                            );
                        }
                        fitState._sgtLastFitPaintSample = {
                            time: paintSampleNow,
                            fontSize: priorDisplayedFontSize
                        };
                        if (!isStreamingFit || targetChanged) {
                            postFitDiagnostic({
                                action: 'fit_target',
                                phase: fitPhase,
                                streamingFit: isStreamingFit,
                                textLen: textLen,
                                winW: winW,
                                winH: winH,
                                fromFontSize: priorDisplayedFontSize,
                                fontSize: targetFontSize,
                                fontStretch: targetWdth,
                                fitDurationMs: performance.now() - _fitStart,
                                layoutProbes: layoutProbeCount,
                                paintedShrinkPxPerSec: paintedShrinkPxPerSec,
                                settleBeforeReveal: settleBeforeReveal
                            });
                        }
                        fitState._sgtLastReportedFitTarget = {
                            streaming: isStreamingFit,
                            fontSize: targetFontSize,
                            fontStretch: targetWdth
                        };

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
                            fitState._sgtLastFinalFit = {
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
                        // A hidden final-only render must commit its target before
                        // reveal. Other final fits can still ease from visible state.
                        var fsDelta = Math.abs(targetFontSize - startFontSize);
                        var wDelta = Math.abs(targetWdth - startWdth);
                        // A growing response can move the target faster than a
                        // fixed-speed animation. Increase velocity with target
                        // debt so repeated retargeting converges instead of
                        // accumulating a delayed final collapse. A visible final
                        // fit continues the same linear controller.
                        if (isStreamingFit) fitState._sgtStreamingMotionActive = true;
                        var usesStreamingMotion = isStreamingFit
                            || fitState._sgtStreamingMotionActive === true;
                        var fontVelocity = usesStreamingMotion ? 55 + fsDelta * 7 : 75;
                        var widthVelocity = usesStreamingMotion ? 120 + wDelta * 5 : 120;
                        var durationFromFont = (fsDelta / fontVelocity) * 1000;
                        var durationFromWdth = (wDelta / widthVelocity) * 1000;
                        var durationFromDelta = Math.max(durationFromFont, durationFromWdth);
                        var minimumDuration = usesStreamingMotion ? 16 : 140;
                        var maximumDuration = usesStreamingMotion ? 180 : 900;
                        var duration = Math.max(minimumDuration,
                            Math.min(maximumDuration, durationFromDelta));
                        function retargetContinuousMotion() {
                            var motion = fitState._sgtMotionController;
                            if (!motion) {
                                motion = {
                                    frame: null,
                                    lastTime: 0,
                                    fontSize: startFontSize,
                                    fontVelocity: 0,
                                    fontStretch: startWdth,
                                    stretchVelocity: 0,
                                    padTop: startPadTop,
                                    padTopVelocity: 0,
                                    padBottom: startPadBottom,
                                    padBottomVelocity: 0,
                                    finalizing: false
                                };
                                fitState._sgtMotionController = motion;
                            }
                            motion.targetFontSize = targetFontSize;
                            motion.targetFontStretch = targetWdth;
                            motion.targetPadTop = targetPadTop;
                            motion.targetPadBottom = targetPadBottom;
                            motion.finalizing = !isStreamingFit;
                            // Search probes mutate the live axes. Restore the exact
                            // currently painted A state before retargeting to B.
                            applyAxes(motion.fontSize, motion.fontStretch);
                            applyPadding(motion.padTop, motion.padBottom);
                            if (motion.frame !== null) return;

                            motion.lastTime = performance.now();
                            var tickMotion = function(now) {
                                var elapsed = Math.max(0.001,
                                    Math.min(0.05, (now - motion.lastTime) / 1000));
                                motion.lastTime = now;
                                var steps = Math.max(1, Math.ceil(elapsed * 120));
                                var dt = elapsed / steps;
                                for (var step = 0; step < steps; step++) {
                                    var debt = Math.abs(motion.targetFontSize - motion.fontSize);
                                    var omega = 20 + Math.min(16, debt * 0.55);
                                    var damping = 2 * omega;
                                    motion.fontVelocity += (omega * omega
                                        * (motion.targetFontSize - motion.fontSize)
                                        - damping * motion.fontVelocity) * dt;
                                    motion.fontSize += motion.fontVelocity * dt;
                                    motion.stretchVelocity += (omega * omega
                                        * (motion.targetFontStretch - motion.fontStretch)
                                        - damping * motion.stretchVelocity) * dt;
                                    motion.fontStretch += motion.stretchVelocity * dt;
                                    motion.padTopVelocity += (omega * omega
                                        * (motion.targetPadTop - motion.padTop)
                                        - damping * motion.padTopVelocity) * dt;
                                    motion.padTop += motion.padTopVelocity * dt;
                                    motion.padBottomVelocity += (omega * omega
                                        * (motion.targetPadBottom - motion.padBottom)
                                        - damping * motion.padBottomVelocity) * dt;
                                    motion.padBottom += motion.padBottomVelocity * dt;
                                }
                                applyAxes(motion.fontSize, motion.fontStretch);
                                applyPadding(motion.padTop, motion.padBottom);
                                fitState._sgtCurrentFontSize = motion.fontSize;
                                fitState._sgtCurrentWdth = motion.fontStretch;
                                var settled = Math.abs(motion.targetFontSize - motion.fontSize) < 0.03
                                    && Math.abs(motion.fontVelocity) < 0.15
                                    && Math.abs(motion.targetFontStretch - motion.fontStretch) < 0.08
                                    && Math.abs(motion.stretchVelocity) < 0.3
                                    && Math.abs(motion.targetPadTop - motion.padTop) < 0.08
                                    && Math.abs(motion.targetPadBottom - motion.padBottom) < 0.08;
                                if (settled) {
                                    applyAxes(motion.targetFontSize, motion.targetFontStretch);
                                    applyPadding(motion.targetPadTop, motion.targetPadBottom);
                                    motion.fontSize = motion.targetFontSize;
                                    motion.fontStretch = motion.targetFontStretch;
                                    motion.padTop = motion.targetPadTop;
                                    motion.padBottom = motion.targetPadBottom;
                                    motion.frame = null;
                                    if (motion.finalizing) {
                                        fitState._sgtStreamingMotionActive = false;
                                    }
                                } else {
                                    motion.frame = scheduleFitFrame(tickMotion);
                                }
                            };
                            motion.frame = scheduleFitFrame(tickMotion);
                        }
                        // Only SNAP when the first fit of a session (no prior
                        // to animate from) or when the delta is essentially
                        // zero (< 0.1px wouldn't be visible anyway). Removed
                        // the old 0.5px threshold — those small jumps were
                        // forming the visible "stair-step" between chunks.
                        var snapThreshold = 0.1;
                        var snapWThreshold = 0.3;
                        if (usesStreamingMotion && hadPriorSize && !settleBeforeReveal) {
                            retargetContinuousMotion();
                        } else if (settleBeforeReveal || !hadPriorSize
                            || (fsDelta < snapThreshold && wDelta < snapWThreshold)) {
                            applyAxes(targetFontSize, targetWdth);
                            applyPadding(targetPadTop, targetPadBottom);
                            fitState._sgtCurrentFontSize = targetFontSize;
                            fitState._sgtCurrentWdth = targetWdth;
                            if (!isStreamingFit) fitState._sgtStreamingMotionActive = false;
                        } else {
                            applyAxes(startFontSize, startWdth);
                            applyPadding(startPadTop, startPadBottom);
                            var animStart = performance.now();
                            var tick = function(now) {
                                var t = Math.min(1, (now - animStart) / duration);
                                var eased = usesStreamingMotion
                                    ? t
                                    : 1 - Math.pow(1 - t, 3);
                                var curFs = startFontSize + (targetFontSize - startFontSize) * eased;
                                var curW = startWdth + (targetWdth - startWdth) * eased;
                                var curPT = startPadTop + (targetPadTop - startPadTop) * eased;
                                var curPB = startPadBottom + (targetPadBottom - startPadBottom) * eased;
                                applyAxes(curFs, curW);
                                applyPadding(curPT, curPB);
                                fitState._sgtCurrentFontSize = curFs;
                                fitState._sgtCurrentWdth = curW;
                                if (t < 1) {
                                    fitState._sgtFitAnim = scheduleFitFrame(tick);
                                } else {
                                    fitState._sgtFitAnim = null;
                                    if (!isStreamingFit) {
                                        fitState._sgtStreamingMotionActive = false;
                                    }
                                }
                            };
                            fitState._sgtFitAnim = scheduleFitFrame(tick);
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
        if ((fitContext && fitContext.fontReady === true)
            || (document.fonts
                && document.fonts.check('400 16px "Google Sans Flex"'))) {
            runFitWhenReady();
        } else {
            fitState._sgtFitting = false;
            try {
                if (fitContext && typeof fitContext.complete === 'function') {
                    fitContext.complete();
                } else if (window.parent && window.parent !== window) {
                    window.parent.postMessage({ type: 'fit_complete' }, '*');
                }
            } catch (_err) {}
            postFitDiagnostic({
                action: 'render_diagnostics',
                phase: fitPhase,
                reason: 'required_font_unavailable',
                renderMode: 'markdown_fit'
            });
        }
    } catch (error) {
        fitState._sgtFitting = false;
        try {
            if (fitContext && typeof fitContext.complete === 'function') {
                fitContext.complete();
            } else if (window.parent && window.parent !== window) {
                window.parent.postMessage({ type: 'fit_complete' }, '*');
            }
        } catch (_err) {}
        postFitDiagnostic({
            action: 'render_diagnostics',
            phase: fitPhase,
            reason: 'required_font_check_failed',
            renderMode: 'markdown_fit',
            error: error && error.message ? error.message : String(error)
        });
    }
})();
