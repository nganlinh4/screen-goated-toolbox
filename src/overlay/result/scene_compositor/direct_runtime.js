(function() {
    var gridRuntime = null;

    function createRuntime(entry, callbacks) {
        var body = entry.bodyElement;
        var viewport = entry.card;
        var state = entry.directState;

        function dimensions() {
            return {
                width: Math.max(1, viewport.clientWidth),
                height: Math.max(1, viewport.clientHeight)
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

        function inlineSize(options, text, isNewSession) {
            if (!options.runInlineSizing || !isNewSession) return;
            var size = dimensions();
            var textLen = text.length;
            var isSourceReplacement = options.sourceReplacement === true;
            var preferredFontSize = Number(options.preferredFontSize);
            if (!Number.isFinite(preferredFontSize) || preferredFontSize <= 0) {
                preferredFontSize = Math.max(1, Math.min(14, size.height));
            }
            var constrained = size.height < 260 || size.width < 420;
            var minSize = isSourceReplacement ? 1 : (textLen < 200 ? 6 : 14);
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
            body.style.fontStretch = '90%';
            body.style.letterSpacing = '0px';
            body.style.wordSpacing = '0px';
            body.style.lineHeight = isSourceReplacement ? '1.15' : '1.5';
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
            var widths = isSourceReplacement ? [90, 85, 80, 75, 70, 65, 60, 55] : [90];
            var best = minSize;
            var bestWidth = 90;
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
                    if (fits(text, size, fitTolerance, !isSourceReplacement)) {
                        candidateBest = mid;
                        candidateFound = true;
                        low = mid + 1;
                    } else {
                        high = mid - 1;
                    }
                }
                if (candidateFound && (!found || candidateBest > best
                    || (candidateBest === best && candidateWidth > bestWidth))) {
                    found = true;
                    best = candidateBest;
                    bestWidth = candidateWidth;
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
            body.style.justifyContent = 'center';
        }

        function revealWords(animate, isNewSession) {
            var words = body.querySelectorAll('.word');
            var reveal = state.reveal;
            if (isNewSession) {
                reveal.queue = [];
                reveal.lastRevealedIndex = words.length - 1;
                reveal.credits = 0;
                return;
            }
            if (!animate) {
                reveal.queue = [];
                reveal.lastRevealedIndex = words.length - 1;
                reveal.credits = 0;
                return;
            }
            reveal.queue = [];
            var start = Math.max(0, reveal.lastRevealedIndex + 1);
            for (var index = start; index < words.length; index++) {
                var word = words[index];
                word.style.visibility = 'hidden';
                word.style.opacity = '0';
                word.style.filter = 'blur(3px)';
                word.style.transition = 'opacity 0.35s ease-out, filter 0.35s ease-out';
                reveal.queue.push({ el: word, index: index });
            }
            if (reveal.active || reveal.queue.length === 0) return;
            reveal.active = true;
            reveal.lastTick = performance.now();
            reveal.credits = 1;
            requestAnimationFrame(function tick(now) {
                if (!reveal.queue.length) {
                    reveal.active = false;
                    reveal.credits = 0;
                    return;
                }
                var elapsed = Math.max(0, now - reveal.lastTick);
                reveal.lastTick = now;
                reveal.credits += 40 * (1 + reveal.queue.length / 10) * elapsed / 1000;
                var emitted = 0;
                while (reveal.credits >= 1 && reveal.queue.length && emitted < 64) {
                    var item = reveal.queue.shift();
                    if (item.el && item.el.isConnected) {
                        item.el.style.visibility = 'visible';
                        item.el.style.opacity = '1';
                        item.el.style.filter = 'blur(0)';
                    }
                    reveal.lastRevealedIndex = item.index;
                    reveal.credits--;
                    emitted++;
                }
                requestAnimationFrame(tick);
            });
        }

        function installOverflowGuard() {
            if (state.overflowObserver || typeof ResizeObserver === 'undefined') return;
            var debounceTimer = 0;
            state.overflowObserver = new ResizeObserver(function() {
                if (debounceTimer) return;
                debounceTimer = setTimeout(function() {
                    debounceTimer = 0;
                    if (state.fit._sgtFitAnim) return;
                    var size = dimensions();
                    var overflow = body.scrollHeight - size.height;
                    if (overflow <= size.height * 0.05) return;
                    var current = parseFloat(body.style.fontSize) || 14;
                    var minimum = state.reveal.lastRevealedIndex + 1 < 200 ? 6 : 14;
                    if (current <= minimum) return;
                    var next = Math.max(minimum, Math.floor(current * size.height / body.scrollHeight * 0.92));
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
            var previousWords = state.wordCount;
            var firstRender = state.renderCount === 0;
            if (firstRender) body.style.opacity = '0';
            body.innerHTML = options.html;
            var text = (body.innerText || body.textContent || '').trim();
            var isNewSession = firstRender || (previousWords < 5 && text.length < 50);
            inlineSize(options, text, isNewSession);
            revealWords(Boolean(options.animateNewWords), isNewSession);
            if (body.style.opacity === '0') body.style.opacity = '1';
            if (options.settleBeforeReveal) finishBodyPresentation();
            state.wordCount = body.querySelectorAll('.word').length;
            state.renderCount++;
            if (!options.animateNewWords) installOverflowGuard();
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
            if (state.overflowObserver) state.overflowObserver.disconnect();
            state.overflowObserver = null;
            state.reveal.queue = [];
        }

        return { apply: apply, initGrids: initGrids, destroy: destroy };
    }

    window.__SGT_CREATE_DIRECT_RUNTIME__ = createRuntime;
})();
