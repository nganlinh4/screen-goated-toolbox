(function() {
    var gridRuntime = null;
    var shapeLayout = window.__SGT_SHAPE_LAYOUT__;

    function createRuntime(entry, callbacks) {
        var body = entry.bodyElement;
        var viewport = entry.card;
        var state = entry.directState;
        var revealRuntime = window.__SGT_CREATE_REVEAL_RUNTIME__(body, state.reveal);

        function dimensions() {
            var inlineWidth = parseFloat(viewport.style.width);
            var inlineHeight = parseFloat(viewport.style.height);
            return {
                width: Math.max(1, Number.isFinite(inlineWidth) ? inlineWidth : viewport.clientWidth),
                height: Math.max(1, Number.isFinite(inlineHeight) ? inlineHeight : viewport.clientHeight)
            };
        }

        function currentLineHeight() {
            var computed = getComputedStyle(body);
            var fontSize = parseFloat(computed.fontSize) || parseFloat(body.style.fontSize) || 14;
            var lineHeight = parseFloat(computed.lineHeight);
            if (!Number.isFinite(lineHeight)) {
                var inline = parseFloat(body.style.lineHeight);
                lineHeight = fontSize * (Number.isFinite(inline) ? inline : 1.5);
            }
            return Math.max(1, lineHeight);
        }

        function hasPathologicalWrap(text, size) {
            if (text.length < 8) return false;
            var tokens = text.split(/\s+/).filter(Boolean);
            var longest = 0;
            for (var i = 0; i < tokens.length; i++) {
                longest = Math.max(longest, tokens[i].length);
            }
            var lines = Math.max(1, Math.round(body.scrollHeight / currentLineHeight()));
            return text.length / lines < 3.5
                && lines > Math.max(3, tokens.length + 1)
                && (tokens.length <= 12 || longest >= 4)
                && size.width > 0;
        }

        function fits(text, size, tolerance, rejectPathologicalWrap) {
            void body.offsetHeight;
            tolerance = Number.isFinite(tolerance) ? tolerance : 2;
            return body.scrollHeight <= size.height + tolerance
                && body.scrollWidth <= size.width + tolerance
                && (!rejectPathologicalWrap || !hasPathologicalWrap(text, size));
        }

        function renderShapePlan(plan) {
            body.innerHTML = '';
            body.style.cssText += ';position:absolute;inset:0;overflow:hidden;display:block;padding:0;margin:0;user-select:text;';
            body.style.fontSize = plan.fontSize + 'px';
            body.style.fontStretch = plan.stretch + '%';
            body.style.fontWeight = '400';
            body.style.fontVariationSettings = "'slnt' 0, 'ROND' 100";
            body.style.lineHeight = plan.lineSize + 'px';
            for (var index = 0; index < plan.lines.length; index++) {
                var line = plan.lines[index];
                var element = document.createElement('span');
                element.className = 'word';
                element.textContent = line.text;
                element.style.cssText = 'position:absolute;display:block;overflow:visible;white-space:nowrap;user-select:text;background:transparent;';
                if (plan.orientation === 'horizontal') {
                    element.style.left = line.slot.from + 'px';
                    element.style.top = line.slot.offset + 'px';
                    element.style.width = (line.slot.to - line.slot.from) + 'px';
                    element.style.height = plan.lineSize + 'px';
                } else {
                    element.style.left = line.slot.offset + 'px';
                    element.style.top = line.slot.from + 'px';
                    element.style.width = plan.lineSize + 'px';
                    element.style.height = (line.slot.to - line.slot.from) + 'px';
                    element.style.writingMode = 'vertical-rl';
                    element.style.textOrientation = 'mixed';
                }
                body.appendChild(element);
            }
        }

        function renderedShapePlanFits() {
            var lines = body.querySelectorAll('.word');
            if (!lines.length) return false;
            for (var index = 0; index < lines.length; index++) {
                var line = lines[index];
                var selection = document.createRange();
                selection.selectNodeContents(line);
                var content = selection.getBoundingClientRect();
                var bounds = line.getBoundingClientRect();
                if (line.scrollWidth > line.clientWidth + 0.5
                    || line.scrollHeight > line.clientHeight + 0.5
                    || content.left < bounds.left - 0.5
                    || content.top < bounds.top - 0.5
                    || content.right > bounds.right + 0.5
                    || content.bottom > bounds.bottom + 0.5) return false;
            }
            return true;
        }

        function layoutSourceRegions(options) {
            var regions = options.sourceRegions;
            var segments = options.sourceSegments;
            if (!Array.isArray(regions) || !Array.isArray(segments)
                || !regions.length || regions.length !== segments.length) return false;
            var size = dimensions();
            var naturalWidth = Math.max(1, entry.backdrop.naturalWidth || size.width);
            var naturalHeight = Math.max(1, entry.backdrop.naturalHeight || size.height);
            var scaleX = size.width / naturalWidth;
            var scaleY = size.height / naturalHeight;
            body.innerHTML = '';
            body.style.cssText += ';position:absolute;inset:0;overflow:hidden;display:block;padding:0;margin:0;user-select:text;';
            var containers = [];
            for (var index = 0; index < regions.length; index++) {
                var region = regions[index];
                var container = document.createElement('span');
                var content = document.createElement('span');
                container.className = 'source-line';
                content.className = 'word source-line-text';
                content.textContent = String(segments[index] || '');
                container.style.cssText = 'position:absolute;display:flex;align-items:center;justify-content:center;overflow:visible;background:transparent;user-select:text;';
                container.style.left = (Number(region.x) * scaleX) + 'px';
                container.style.top = (Number(region.y) * scaleY) + 'px';
                container.style.width = (Number(region.width) * scaleX) + 'px';
                container.style.height = (Number(region.height) * scaleY) + 'px';
                content.style.cssText = 'display:block;max-width:100%;max-height:100%;margin:0;padding:0;background:transparent;user-select:text;overflow:visible;white-space:nowrap;text-align:center;';
                if (region.vertical === true) {
                    content.style.writingMode = 'vertical-rl';
                    content.style.textOrientation = 'mixed';
                }
                container.appendChild(content);
                body.appendChild(container);
                containers.push({ box: container, text: content });
            }
            state.sourceLayoutReady = window.__SGT_QUEUE_SOURCE_FIT__(containers.map(
                function(item, itemIndex) {
                    return {
                        box: item.box,
                        text: item.text,
                        vertical: regions[itemIndex].vertical === true
                    };
                }
            ));
            body.dataset.shapeLayout = 'true';
            return true;
        }

        function layoutInBackdrop(options, text) {
            if (!options.sourceReplacement || !entry.backdrop || !entry.backdrop.dataset.url) return false;
            if (!entry.backdrop.complete || !entry.backdrop.naturalWidth) {
                body.style.opacity = '0';
                var pendingUrl = entry.backdrop.dataset.url;
                var decoded = typeof entry.backdrop.decode === 'function'
                    ? entry.backdrop.decode().catch(function() {})
                    : new Promise(function(resolve) {
                        entry.backdrop.addEventListener('load', resolve, { once: true });
                        entry.backdrop.addEventListener('error', resolve, { once: true });
                    });
                state.sourceLayoutReady = decoded.then(function() {
                    if (entry.backdrop.dataset.url !== pendingUrl) return;
                    var currentText = (body.textContent || text).trim();
                    layoutInBackdrop(options, currentText);
                    body.style.opacity = '1';
                    return state.sourceLayoutReady;
                });
                return true;
            }
            state.sourceLayoutReady = Promise.resolve();
            if (Array.isArray(options.sourceRegions) && options.sourceRegions.length) {
                layoutSourceRegions(options);
                return true;
            }
            var size = dimensions();
            body.style.webkitMaskImage = 'url("' + entry.backdrop.dataset.url + '")';
            body.style.maskImage = 'url("' + entry.backdrop.dataset.url + '")';
            body.style.webkitMaskSize = '100% 100%';
            body.style.maskSize = '100% 100%';
            body.style.webkitMaskRepeat = 'no-repeat';
            body.style.maskRepeat = 'no-repeat';
            var canvas = document.createElement('canvas');
            canvas.width = Math.max(1, Math.round(size.width));
            canvas.height = Math.max(1, Math.round(size.height));
            var context = canvas.getContext('2d', { willReadFrequently: true });
            context.drawImage(entry.backdrop, 0, 0, canvas.width, canvas.height);
            var alpha = context.getImageData(0, 0, canvas.width, canvas.height).data;
            var preferred = Math.max(5, Math.min(Number(options.preferredFontSize) || 14, canvas.height));
            var widths = [100, 90, 80, 70, 60, 50];
            var fallback = null;
            for (var fontSize = Math.floor(preferred); fontSize >= 1; fontSize--) {
                for (var widthIndex = 0; widthIndex < widths.length; widthIndex++) {
                    var orientation = options.sourceVertical
                        && shapeLayout.prefersVerticalWriting(text) ? 'vertical' : 'horizontal';
                    var candidate = shapeLayout.fillPlan(text, alpha, canvas.width, canvas.height,
                        orientation, fontSize, widths[widthIndex]);
                    if (candidate && candidate.complete) {
                        renderShapePlan(candidate);
                        if (renderedShapePlanFits()) {
                            body.dataset.shapeLayout = 'true';
                            return true;
                        }
                    }
                    if (candidate && (!fallback || candidate.consumed > fallback.consumed)) {
                        fallback = candidate;
                    }
                }
            }
            if (!fallback) return false;
            renderShapePlan(fallback);
            body.dataset.shapeLayout = 'true';
            return true;
        }

        function inlineSize(options, text, isNewSession) {
            if (!options.runInlineSizing) return;
            if (options.sourceReplacement && layoutInBackdrop(options, text)) return;
            if (!isNewSession) return;
            if (!options.sourceReplacement) {
                inlineSizeOrdinary(text, options.finalizing);
                return;
            }
            delete body.dataset.shapeLayout;
            var size = dimensions();
            var textLen = text.length;
            var isSourceReplacement = options.sourceReplacement === true;
            var preferredFontSize = Number(options.preferredFontSize);
            if (!Number.isFinite(preferredFontSize) || preferredFontSize <= 0) {
                preferredFontSize = Math.max(1, Math.min(14, size.height));
            }
            var constrained = size.height < 260 || size.width < 420;
            var minSize = isSourceReplacement ? Math.min(5, preferredFontSize) : (textLen < 200 ? 6 : 14);
            var finalMaximum = isSourceReplacement ? preferredFontSize : (textLen < 300
                ? 200
                : (textLen < 1500 ? 100 : Math.max(24, Math.min(48, Math.floor(size.height / 10)))));
            var maxPossible = Math.min(
                options.finalizing ? finalMaximum : (constrained ? 40 : 48),
                size.height
            );
            var estimated = Math.sqrt((size.width * size.height) / (textLen + 1));
            body.style.fontWeight = '400';
            body.style.fontVariationSettings = "'slnt' 0, 'ROND' 100";
            body.style.fontStretch = isSourceReplacement ? '100%' : '90%';
            body.style.letterSpacing = '0px';
            body.style.wordSpacing = '0px';
            body.style.lineHeight = isSourceReplacement ? '1.08' : '1.5';
            body.style.display = 'flex';
            body.style.flexDirection = 'column';
            // Measure from the top so oversized glyphs cannot hide in negative
            // flex overflow; center only after a contained size is committed.
            body.style.justifyContent = 'flex-start';
            var fitTolerance = isSourceReplacement ? 0 : 2;
            body.style.paddingTop = '0';
            body.style.paddingBottom = '0';
            var blocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
            for (var index = 0; index < blocks.length; index++) {
                blocks[index].style.marginBottom = isSourceReplacement ? '0' : '0.5em';
                blocks[index].style.paddingBottom = '0';
            }
            var widths = isSourceReplacement
                ? [100, 90, 80, 70, 60, 50, 40, 30, 25]
                : [90];
            var best = minSize;
            var bestWidth = isSourceReplacement ? 100 : 90;
            var bestScore = 0;
            var found = false;
            for (var widthIndex = 0; widthIndex < widths.length; widthIndex++) {
                var candidateWidth = widths[widthIndex];
                body.style.fontStretch = candidateWidth + '%';
                var low = isSourceReplacement
                    ? minSize
                    : Math.max(minSize, Math.floor(estimated * 0.5));
                var high = isSourceReplacement
                    ? Math.floor(maxPossible)
                    : Math.min(Math.floor(maxPossible), Math.ceil(estimated * 1.15));
                if (low > high) low = high;
                var candidateBest = minSize;
                var candidateFound = false;
                while (low <= high) {
                    var mid = Math.floor((low + high) / 2);
                    body.style.fontSize = mid + 'px';
                    if (fits(text, size, fitTolerance, true)) {
                        candidateBest = mid;
                        candidateFound = true;
                        low = mid + 1;
                    } else {
                        high = mid - 1;
                    }
                }
                // Treat width-axis compression as a readability cost, not free
                // space. Deep condensation remains an emergency containment
                // fallback while the complete variable-font axis remains usable.
                var widthQuality = 0.35 + 0.65 * Math.min(100, candidateWidth) / 100;
                var candidateScore = candidateBest * widthQuality;
                if (candidateFound && (!found || candidateScore > bestScore
                    || (Math.abs(candidateScore - bestScore) < 0.01
                        && candidateWidth > bestWidth))) {
                    found = true;
                    best = candidateBest;
                    bestWidth = candidateWidth;
                    bestScore = candidateScore;
                }
            }
            body.style.fontStretch = bestWidth + '%';
            body.style.fontSize = Math.max(best, minSize) + 'px';
            if (!isSourceReplacement && constrained && textLen < 450) {
                var settleLow = minSize;
                var settleHigh = best;
                var settleBest = minSize;
                while (settleLow <= settleHigh) {
                    var settleMid = Math.floor((settleLow + settleHigh) / 2);
                    body.style.fontSize = settleMid + 'px';
                    if (fits(text, size, fitTolerance, true)) {
                        settleBest = settleMid;
                        settleLow = settleMid + 1;
                    } else {
                        settleHigh = settleMid - 1;
                    }
                }
                body.style.fontSize = settleBest + 'px';
            }
            body.style.justifyContent = fits(text, size, fitTolerance, false)
                ? 'center'
                : 'flex-start';
        }

        function inlineSizeOrdinary(text, finalizing) {
            var size = dimensions();
            var textLen = text.length;
            var constrained = size.height < 260 || size.width < 420;
            var minSize = textLen < 200 ? 6 : 14;
            var finalMaximum = size.height;
            var maxPossible = Math.min(finalizing ? finalMaximum : (constrained ? 40 : 48), size.height);
            var estimated = Math.sqrt((size.width * size.height) / (textLen + 1));
            var low = Math.max(minSize, Math.floor(estimated * 0.5));
            var high = Math.min(maxPossible, Math.ceil(estimated * 1.15));
            if (low > high) low = high;

            body.style.fontWeight = '400';
            body.style.fontStretch = '90%';
            body.style.fontVariationSettings = "'slnt' 0, 'ROND' 100";
            body.style.letterSpacing = '0px';
            body.style.wordSpacing = '0px';
            body.style.lineHeight = '1.5';
            body.style.paddingTop = '0';
            body.style.paddingBottom = '0';
            var blocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
            for (var index = 0; index < blocks.length; index++) {
                blocks[index].style.marginBottom = '0.5em';
                blocks[index].style.paddingBottom = '0';
            }
            var best = low;
            while (low <= high) {
                var mid = Math.floor((low + high) / 2);
                body.style.fontSize = mid + 'px';
                if (fits(text, size, 2, true)) {
                    best = mid;
                    low = mid + 1;
                } else {
                    high = mid - 1;
                }
            }
            body.style.fontSize = Math.max(best, minSize) + 'px';
            if (constrained && textLen < 450) {
                var settleLow = minSize;
                var settleHigh = best;
                var settleBest = minSize;
                while (settleLow <= settleHigh) {
                    var settleMid = Math.floor((settleLow + settleHigh) / 2);
                    body.style.fontSize = settleMid + 'px';
                    if (fits(text, size, 2, true)) {
                        settleBest = settleMid;
                        settleLow = settleMid + 1;
                    } else {
                        settleHigh = settleMid - 1;
                    }
                }
                body.style.fontSize = settleBest + 'px';
            }
        }

        function installSourceOverflowGuard(preferredFontSize) {
            if (state.overflowObserver || typeof ResizeObserver === 'undefined') return;
            var debounceTimer = 0;
            state.overflowObserver = new ResizeObserver(function() {
                if (debounceTimer) return;
                debounceTimer = setTimeout(function() {
                    debounceTimer = 0;
                    if (state.fit._sgtFitAnim) return;
                    var size = dimensions();
                    var overflow = Math.max(
                        body.scrollHeight - size.height,
                        body.scrollWidth - size.width
                    );
                    if (overflow <= 0) return;
                    var current = parseFloat(body.style.fontSize) || 14;
                    var minimum = Math.min(5, Number(preferredFontSize) || 5);
                    if (current <= minimum) return;
                    var next = Math.max(minimum, Math.floor(current * Math.min(
                        size.height / Math.max(1, body.scrollHeight),
                        size.width / Math.max(1, body.scrollWidth)
                    ) * 0.96));
                    if (next < current) {
                        body.style.fontSize = next + 'px';
                        state.fit._sgtCurrentFontSize = next;
                    }
                }, 120);
            });
            state.overflowObserver.observe(body);
        }

        function finishBodyPresentation() {
            body.style.setProperty('animation', 'none', 'important');
            body.style.setProperty('opacity', '1', 'important');
            body.style.setProperty('filter', 'blur(0)', 'important');
            body.style.setProperty('-webkit-backdrop-filter', 'blur(0)', 'important');
            body.style.setProperty('backdrop-filter', 'blur(0)', 'important');
            body.style.setProperty('transform', 'translateY(0)', 'important');
            if (typeof body.getAnimations !== 'function') return;
            var animations = body.getAnimations();
            for (var index = 0; index < animations.length; index++) {
                try { animations[index].finish(); } catch (_error) {}
            }
        }

        function apply(options) {
            if (state.fit._sgtVisualSettle) state.fit._sgtVisualSettle.cancel();
            var previousWords = state.wordCount;
            var firstRender = state.renderCount === 0;
            // Each source replacement card owns one compositor animation. Stop the
            // inherited per-body animation so it cannot compete for the same filter.
            if (options.sourceReplacement) finishBodyPresentation();
            if (firstRender) body.style.opacity = '0';
            if (options.sourceReplacement) body.innerHTML = options.html;
            else window.__SGT_PATCH_BODY__(body, options.html);
            if (!options.sourceReplacement) {
                body.style.overflowX = 'hidden';
                body.style.overflowY = 'auto';
            }
            var text = (body.textContent || '').trim();
            var isNewSession = firstRender || (previousWords < 5 && text.length < 50);
            inlineSize(options, text, isNewSession);
            revealRuntime.update(Boolean(options.animateNewWords), isNewSession);
            if (body.style.opacity === '0') body.style.opacity = '1';
            if (options.settleBeforeReveal) finishBodyPresentation();
            state.wordCount = body.querySelectorAll('.word').length;
            state.renderCount++;
            if (!options.animateNewWords && options.sourceReplacement === true) {
                installSourceOverflowGuard(options.preferredFontSize);
            }
        }

        function ensureGridRuntime() {
            if (typeof gridjs !== 'undefined') return Promise.resolve();
            if (gridRuntime) return gridRuntime;
            gridRuntime = new Promise(function(resolve, reject) {
                var link = document.createElement('link');
                link.rel = 'stylesheet';
                link.href = '__SGT_GRID_CSS_URL__';
                document.head.appendChild(link);
                var script = document.createElement('script');
                script.src = '__SGT_GRID_JS_URL__';
                script.onload = resolve;
                script.onerror = function() { reject(new Error('Grid runtime failed to load')); };
                document.head.appendChild(script);
            });
            return gridRuntime;
        }

        function initGrids() {
            var selector = 'table:not(.gridjs-table):not([data-processed-table="true"])';
            if (!body.querySelector(selector)) return;
            ensureGridRuntime().then(function() {
                var tables = body.querySelectorAll(selector);
                var pending = 0;
                function ready() {
                    pending--;
                    if (pending === 0) callbacks.requestFit(false);
                }
                for (var i = 0; i < tables.length; i++) {
                    var table = tables[i];
                    if (table.closest('.gridjs-container,.gridjs-injected-wrapper')) continue;
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
                            style: { table: { width: '100%' } },
                            className: {
                                table: 'gridjs-table-premium',
                                th: 'gridjs-th-premium',
                                td: 'gridjs-td-premium'
                            }
                        });
                        pending++;
                        (function(source, instance) {
                            instance.on('ready', function() {
                                source.classList.add('gridjs-hidden-source');
                                requestAnimationFrame(ready);
                            });
                        })(table, grid);
                        grid.render(wrapper);
                    } catch (_error) {
                        wrapper.remove();
                    }
                }
            }).catch(function(error) {
                callbacks.diagnostic('grid_runtime_failed', error);
            });
        }

        function destroy() {
            if (state.fit._sgtVisualSettle) state.fit._sgtVisualSettle.cancel();
            if (state.overflowObserver) state.overflowObserver.disconnect();
            state.overflowObserver = null;
            revealRuntime.destroy();
            var motion = state.fit._sgtMotionController;
            if (motion && motion.frame !== null) cancelAnimationFrame(motion.frame);
            state.fit._sgtMotionController = null;
        }

        return {
            apply: apply,
            initGrids: initGrids,
            beginResizePreview: function(size) {
                if (state.fit._sgtVisualSettle) state.fit._sgtVisualSettle.cancel();
                return window.__SGT_TYPOGRAPHY_RESIZE__.begin(
                    body, state.fit, size, cancelAnimationFrame.bind(window));
            },
            destroy: destroy
        };
    }

    window.__SGT_CREATE_DIRECT_RUNTIME__ = createRuntime;
})();
