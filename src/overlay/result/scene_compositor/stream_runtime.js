(function() {
    window.__SGT_APPLY_STREAM_UPDATE__ = function(options) {
        var prevWordCount = window._streamWordCount || 0;
        var prevRenderCount = window._streamRenderCount || 0;
        if (prevRenderCount === 0) {
            document.body.style.opacity = '0';
        }
        document.body.innerHTML = options.html;

        var body = document.body;
        var doc = document.documentElement;
        if (!body || !doc) return;

        var winH = window.innerHeight;
        var winW = window.innerWidth;
        var text = (body.innerText || body.textContent || '').trim();
        var textLen = text.length;
        var isConstrainedWindow = winH < 260 || winW < 420;
        var isNewSession = prevRenderCount === 0 || (prevWordCount < 5 && textLen < 50);
        var isConstrainedShortContent = isConstrainedWindow && textLen < 450;

        function currentLineHeightPx() {
            var computed = window.getComputedStyle(body);
            var fontSize = parseFloat(computed.fontSize) || parseFloat(body.style.fontSize) || 14;
            var lineHeight = parseFloat(computed.lineHeight);
            if (!Number.isFinite(lineHeight)) {
                var inlineLineHeight = parseFloat(body.style.lineHeight);
                lineHeight = fontSize * (Number.isFinite(inlineLineHeight) ? inlineLineHeight : 1.5);
            }
            return Math.max(1, lineHeight);
        }

        function hasPathologicalWrap() {
            if (textLen < 8) return false;
            var tokens = text.split(/\s+/).filter(Boolean);
            var longestToken = 0;
            for (var i = 0; i < tokens.length; i++) {
                longestToken = Math.max(longestToken, tokens[i].length);
            }
            var approxLineCount = Math.max(1, Math.round(doc.scrollHeight / currentLineHeightPx()));
            var avgCharsPerLine = textLen / approxLineCount;
            return avgCharsPerLine < 3.5
                && approxLineCount > Math.max(3, tokens.length + 1)
                && (tokens.length <= 12 || longestToken >= 4);
        }

        function fitsWindow() {
            void body.offsetHeight;
            var scrollW = Math.max(doc.scrollWidth || 0, body.scrollWidth || 0);
            return doc.scrollHeight <= winH + 2
                && scrollW <= winW + 2
                && !hasPathologicalWrap();
        }

        var minSize = textLen < 200 ? 6 : 14;
        if (options.runInlineSizing && isNewSession) {
            var finalMaximum = textLen < 300
                ? 200
                : (textLen < 1500 ? 100 : Math.max(24, Math.min(48, Math.floor(winH / 10))));
            var maxPossible = Math.min(
                options.finalizing ? finalMaximum : (isConstrainedWindow ? 40 : 48),
                winH
            );
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
            for (var blockIndex = 0; blockIndex < blocks.length; blockIndex++) {
                blocks[blockIndex].style.marginBottom = '0.5em';
                blocks[blockIndex].style.paddingBottom = '0';
            }

            void body.offsetHeight;
            var best = low;
            while (low <= high) {
                var mid = Math.floor((low + high) / 2);
                body.style.fontSize = mid + 'px';
                if (fitsWindow()) {
                    best = mid;
                    low = mid + 1;
                } else {
                    high = mid - 1;
                }
            }
            body.style.fontSize = Math.max(best, minSize) + 'px';

            if (isConstrainedShortContent) {
                var settleLow = minSize;
                var settleHigh = best;
                var settleBest = minSize;
                while (settleLow <= settleHigh) {
                    var settleMid = Math.floor((settleLow + settleHigh) / 2);
                    body.style.fontSize = settleMid + 'px';
                    if (fitsWindow()) {
                        settleBest = settleMid;
                        settleLow = settleMid + 1;
                    } else {
                        settleHigh = settleMid - 1;
                    }
                }
                body.style.fontSize = settleBest + 'px';
            }
        }

        var words = document.querySelectorAll('.word');
        var newWordCount = words.length;
        if (!window._streamRevealState) {
            window._streamRevealState = {
                queue: [],
                active: false,
                lastRevealedIndex: -1,
                lastTick: 0,
                credits: 0
            };
        }
        var revealState = window._streamRevealState;

        if (isNewSession) {
            revealState.queue = [];
            revealState.active = false;
            revealState.lastRevealedIndex = newWordCount - 1;
            revealState.credits = 0;
        } else if (!options.animateNewWords) {
            revealState.queue.forEach(function(item) {
                if (item.el && item.el.isConnected) {
                    item.el.style.visibility = 'visible';
                    item.el.style.opacity = '1';
                    item.el.style.filter = 'blur(0)';
                }
            });
            revealState.queue = [];
            revealState.active = false;
            revealState.lastRevealedIndex = newWordCount - 1;
            revealState.credits = 0;
        } else {
            revealState.queue = [];
            var revealStart = Math.max(0, revealState.lastRevealedIndex + 1);
            for (var wordIndex = revealStart; wordIndex < newWordCount; wordIndex++) {
                var word = words[wordIndex];
                if (!word) continue;
                word.style.visibility = 'hidden';
                word.style.opacity = '0';
                word.style.filter = 'blur(3px)';
                word.style.transition = 'opacity 0.35s ease-out, filter 0.35s ease-out';
                revealState.queue.push({ el: word, index: wordIndex });
            }

            if (revealState.queue.length > 0 && !revealState.active) {
                revealState.active = true;
                revealState.lastTick = performance.now();
                revealState.credits = 1;
                var tick = function(now) {
                    var queue = revealState.queue;
                    if (!queue || queue.length === 0) {
                        revealState.active = false;
                        revealState.credits = 0;
                        return;
                    }
                    var elapsed = Math.max(0, now - revealState.lastTick);
                    revealState.lastTick = now;
                    var targetWordsPerSecond = 40 * (1 + queue.length / 10);
                    revealState.credits += targetWordsPerSecond * elapsed / 1000;
                    var emitted = 0;
                    while (revealState.credits >= 1 && queue.length > 0 && emitted < 64) {
                        var item = queue.shift();
                        if (item.el && item.el.isConnected) {
                            item.el.style.visibility = 'visible';
                            item.el.style.opacity = '1';
                            item.el.style.filter = 'blur(0)';
                        }
                        revealState.lastRevealedIndex = item.index;
                        revealState.credits -= 1;
                        emitted++;
                    }
                    requestAnimationFrame(tick);
                };
                requestAnimationFrame(tick);
            }
        }

        if (body.style.opacity === '0') body.style.opacity = '1';
        if (!options.animateNewWords
            && !window._sgtOverflowObserver
            && typeof ResizeObserver !== 'undefined') {
            var debounceTimer = null;
            var observer = new ResizeObserver(function() {
                if (debounceTimer) return;
                debounceTimer = setTimeout(function() {
                    debounceTimer = null;
                    if (window._sgtFitAnim) return;
                    var overflowPx = doc.scrollHeight - window.innerHeight;
                    if (overflowPx <= window.innerHeight * 0.05) return;
                    var revealed = revealState.lastRevealedIndex + 1;
                    var currentSize = parseFloat(body.style.fontSize) || 14;
                    var minimumSize = revealed > 0 && revealed < 200 ? 6 : 14;
                    if (currentSize <= minimumSize) return;
                    var scale = (window.innerHeight / doc.scrollHeight) * 0.92;
                    var nextSize = Math.max(minimumSize, Math.floor(currentSize * scale));
                    if (nextSize >= currentSize) return;
                    body.style.fontSize = nextSize + 'px';
                    window._sgtCurrentFontSize = nextSize;
                }, 120);
            });
            observer.observe(body);
            window._sgtOverflowObserver = observer;
        }

        window._streamWordCount = newWordCount;
        window._streamRenderCount = prevRenderCount + 1;
        // Auto-fit owns the complete viewport, so keep its stable top edge
        // while newly appended content temporarily overflows. Following the
        // bottom here makes scroll position fight the shrink animation: each
        // chunk hides the first line, then layout clamping snaps it back.
        window.scrollTo({
            top: 0,
            left: 0,
            behavior: 'auto'
        });
    };

    window.__SGT_INIT_STREAM_GRIDS__ = function() {
        var tableSelector = 'table:not(.gridjs-table):not([data-processed-table="true"])';
        if (!document.querySelector(tableSelector)) return;
        if (typeof gridjs === 'undefined') {
            if (!window._sgtGridRuntime) {
                window._sgtGridRuntime = new Promise(function(resolve, reject) {
                    var stylesheet = document.createElement('link');
                    stylesheet.rel = 'stylesheet';
                    stylesheet.href = '__SGT_GRID_CSS_URL__';
                    document.head.appendChild(stylesheet);
                    var script = document.createElement('script');
                    script.src = '__SGT_GRID_JS_URL__';
                    script.onload = resolve;
                    script.onerror = function() {
                        reject(new Error('Grid runtime failed to load'));
                    };
                    document.head.appendChild(script);
                });
            }
            window._sgtGridRuntime.then(function() {
                window.__SGT_INIT_STREAM_GRIDS__();
            }).catch(function(error) {
                window.parent.postMessage({
                    type: 'card_diagnostic',
                    phase: 'grid_runtime_failed',
                    error: String(error && error.message ? error.message : error)
                }, '*');
            });
            return;
        }
        var tables = document.querySelectorAll(
            tableSelector
        );
        var pendingGrids = 0;

        function shrinkAfterLayout() {
            var doc = document.documentElement;
            var winH = window.innerHeight;
            var overflowPx = doc.scrollHeight - winH;
            if (overflowPx <= winH * 0.05) return;
            var currentSize = parseFloat(document.body.style.fontSize) || 14;
            if (currentSize <= 14) return;
            var scale = (winH / doc.scrollHeight) * 0.92;
            var nextSize = Math.max(14, Math.floor(currentSize * scale));
            if (nextSize >= currentSize) return;
            document.body.style.fontSize = nextSize + 'px';
            window._sgtCurrentFontSize = nextSize;
        }

        function afterGridReady() {
            pendingGrids--;
            if (pendingGrids > 0) return;
            var poll = function() {
                if (window._sgtFitAnim) {
                    requestAnimationFrame(poll);
                } else {
                    shrinkAfterLayout();
                    if (typeof window.__SGT_REQUEST_FIT__ === 'function') {
                        window.__SGT_REQUEST_FIT__(false);
                    }
                }
            };
            poll();
        }

        for (var i = 0; i < tables.length; i++) {
            var table = tables[i];
            if (table.closest('.gridjs-container')
                || table.closest('.gridjs-injected-wrapper')) continue;
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
                        table: { width: '100%' },
                        td: { border: '1px solid #333' },
                        th: { border: '1px solid #333' }
                    },
                    className: {
                        table: 'gridjs-table-premium',
                        th: 'gridjs-th-premium',
                        td: 'gridjs-td-premium'
                    }
                });
                pendingGrids++;
                (function(sourceTable, currentGrid) {
                    currentGrid.on('ready', function() {
                        sourceTable.classList.add('gridjs-hidden-source');
                        requestAnimationFrame(afterGridReady);
                    });
                })(table, grid);
                grid.render(wrapper);
            } catch (_error) {
                if (wrapper.parentNode) wrapper.parentNode.removeChild(wrapper);
            }
        }
    };
})();
