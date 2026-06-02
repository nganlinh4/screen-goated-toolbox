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
                        // Continuous-flow duration: duration scales linearly
                        // with delta so visual velocity is constant (~55 px/s
                        // for streaming, ~75 px/s for final). A 5px change
                        // finishes in ~90ms, a 40px change takes ~720ms — no
                        // jarring fast flicks for small deltas, no
                        // "everything takes 280ms" for big ones. Clamped at
                        // [140, 900]ms so the loop never feels instant or
                        // glacial regardless of delta.
                        var PX_PER_SEC = isStreamingFit ? 55 : 75;
                        var durationFromDelta = (fsDelta / PX_PER_SEC) * 1000;
                        var duration = Math.max(140, Math.min(900, durationFromDelta));
                        // Only SNAP when the first fit of a session (no prior
                        // to animate from) or when the delta is essentially
                        // zero (< 0.1px wouldn't be visible anyway). Removed
                        // the old 0.5px threshold — those small jumps were
                        // forming the visible "stair-step" between chunks.
                        var snapThreshold = 0.1;
                        var snapWThreshold = 0.3;
                        if (!hadPriorSize || (fsDelta < snapThreshold && wDelta < snapWThreshold)) {
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
                                // ease-out cubic — non-zero initial velocity
                                // preserves visual continuity when a new
                                // target comes in mid-animation (common in
                                // fast streaming). smootherStep would create
                                // a brake-and-restart feel at every new fit.
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
