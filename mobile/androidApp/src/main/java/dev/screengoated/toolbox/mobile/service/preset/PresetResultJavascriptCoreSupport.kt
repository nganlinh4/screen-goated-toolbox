package dev.screengoated.toolbox.mobile.service.preset

internal fun presetResultJavascriptCore(): String {
    return """
        let activeWindowId = null;
        let dragState = null;
        let resizeState = null;
        let holdTimer = null;
        let pendingStart = null;
        let twoFingerScrollState = null;
        const DRAG_THRESHOLD_PX = 6;
        const RESIZE_ZONE_PX = 48;
        const SYSTEM_GESTURE_SIDE_GUARD_PX = 20;
        const SYSTEM_GESTURE_BOTTOM_GUARD_PX = 30;
        const DRAG_GAIN = 2.25;
        const RESIZE_GAIN = 1.85;
        const INTERACTIVE_WORD_WRAP_CHAR_LIMIT = 6000;
        const INTERACTIVE_WORD_WRAP_WORD_LIMIT = 900;
        const INERTIA_MIN_VELOCITY = 0.15;
        const INERTIA_FRICTION = 0.92;
        let inertiaFrame = null;
        ${presetResultSelectionJavascriptHelpers()}

        function postJson(payload) {
            window.ipc.postMessage(JSON.stringify(payload));
        }

        function stopInertiaScroll() {
            if (inertiaFrame) {
                cancelAnimationFrame(inertiaFrame);
                inertiaFrame = null;
            }
        }

        function debugGesture(_phase, _extra) {}

        window.configureResultWindow = function(windowId) {
            activeWindowId = windowId;
            if (document.body) {
                document.body.setAttribute('data-window-id', windowId);
            }
        };

        function activateWindow() {
            if (!activeWindowId) return;
            postJson({ type: 'activateResultWindow', windowId: activeWindowId });
        }

        function promoteWindow() {
            if (!activeWindowId) return;
            postJson({ type: 'promoteResultWindow', windowId: activeWindowId });
        }

        function sendNavigationState(navDepth, maxNavDepth, isBrowsing) {
            if (!activeWindowId) return;
            postJson({
                type: 'navigationState',
                windowId: activeWindowId,
                navDepth: navDepth,
                maxNavDepth: maxNavDepth,
                isBrowsing: !!isBrowsing
            });
        }

        function touchPoint(event) {
            if (event.touches && event.touches.length > 0) return event.touches[0];
            if (event.changedTouches && event.changedTouches.length > 0) return event.changedTouches[0];
            return event;
        }

        function detectResizeCorner(clientX, clientY) {
            const localX = clientX;
            const localY = clientY;
            if (localY < window.innerHeight - RESIZE_ZONE_PX) return null;
            if (localX < RESIZE_ZONE_PX) return 'bl';
            if (localX > window.innerWidth - RESIZE_ZONE_PX) return 'br';
            return null;
        }

        function isSystemGestureGuardPoint(clientX, clientY) {
            return clientY >= (window.innerHeight - SYSTEM_GESTURE_BOTTOM_GUARD_PX) ||
                clientX <= SYSTEM_GESTURE_SIDE_GUARD_PX ||
                clientX >= (window.innerWidth - SYSTEM_GESTURE_SIDE_GUARD_PX);
        }

        function clearHoldTimer() {
            if (holdTimer) {
                clearTimeout(holdTimer);
                holdTimer = null;
            }
        }

        function normalizeTarget(target) {
            if (!target) return null;
            return target.nodeType === Node.TEXT_NODE ? target.parentElement : target;
        }

        function isSelectionTarget(target) {
            if (!target) return false;
            if (target.nodeType === Node.TEXT_NODE) return true;
            const element = normalizeTarget(target);
            if (!element || !element.closest) return false;
            if (element.closest('a, button, input, textarea, select, canvas, video, audio, iframe, [contenteditable="true"]')) return true;
            if (element.closest('span.word, code, pre, td, th')) return true;
            const text = (element.textContent || '').trim();
            if (text.length < 2) return false;
            const computed = window.getComputedStyle(element);
            const userSelect = computed.userSelect || computed.webkitUserSelect || '';
            if (userSelect === 'none') return false;
            if (element === document.body || element === document.documentElement) return false;
            return true;
        }

        function beginDrag(point) {
            if (!pendingStart) return;
            dragState = { x: point.screenX, y: point.screenY };
            activateWindow();
        }

        function runFit(streaming) {
            if (window.runWindowsMarkdownFit) {
                window.runWindowsMarkdownFit(!!streaming, streaming ? 'mobile_streaming_fit' : 'mobile_final_fit');
            }
        }

        function resetStreamCounters() {
            window._streamWordCount = 0;
            window._streamRenderCount = 0;
            if (window._streamRevealState) {
                window._streamRevealState.queue = [];
                window._streamRevealState.active = false;
                window._streamRevealState.lastRevealedIndex = -1;
                window._streamRevealState.credits = 0;
            }
        }

        function shouldEnableInteractiveWordWrap(text) {
            if ((navigator.maxTouchPoints || 0) > 0 || 'ontouchstart' in window) return false;
            if (!text || text.length > INTERACTIVE_WORD_WRAP_CHAR_LIMIT) return false;
            const words = text.trim() ? text.trim().split(/\s+/) : [];
            return words.length <= INTERACTIVE_WORD_WRAP_WORD_LIMIT;
        }

        function shouldSkipWordWrap(node) {
            const parent = node.parentElement;
            if (!parent) return true;
            return !!parent.closest('pre, code, table, script, style');
        }

        function wrapInteractiveWords(root) {
            if (!root || root.querySelector('.word')) return;
            const text = (root.innerText || root.textContent || '').trim();
            if (!shouldEnableInteractiveWordWrap(text)) return;
            const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
                acceptNode(node) {
                    if (!node.nodeValue || !node.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
                    return shouldSkipWordWrap(node) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
                }
            });
            const textNodes = [];
            while (walker.nextNode()) textNodes.push(walker.currentNode);
            textNodes.forEach(node => {
                const fragment = document.createDocumentFragment();
                const parts = node.nodeValue.split(/(\s+)/);
                parts.forEach(part => {
                    if (!part) return;
                    if (/^\s+$/.test(part)) {
                        fragment.appendChild(document.createTextNode(part));
                        return;
                    }
                    const span = document.createElement('span');
                    span.className = 'word';
                    span.textContent = part;
                    fragment.appendChild(span);
                });
                node.parentNode.replaceChild(fragment, node);
            });
        }

        function schedulePostTableFit(streaming) {
            if (!document.body.querySelector('table')) return;
            setTimeout(() => runFit(streaming), 250);
        }

        function applyBodyHtml(html) {
            document.body.innerHTML = html || '';
            wrapInteractiveWords(document.body);
        }

        function applyFinalResultState(raw) {
            const data = typeof raw === 'string' ? JSON.parse(raw) : raw;
            activeWindowId = data.windowId;
            if (data.loading) {
                document.body.innerHTML = data.html || '';
                document.body.style.opacity = '1';
                resetStreamCounters();
                return;
            }
            applyBodyHtml(data.html);
            resetStreamCounters();
            runFit(!!data.streaming);
            schedulePostTableFit(!!data.streaming);
        }

        function applyStreamingResultState(raw) {
            const data = typeof raw === 'string' ? JSON.parse(raw) : raw;
            activeWindowId = data.windowId;
            if (data.loading) {
                document.body.innerHTML = data.html || '';
                document.body.style.opacity = '1';
                resetStreamCounters();
                return;
            }
            const prevWordCount = window._streamWordCount || 0;
            const prevRenderCount = window._streamRenderCount || 0;

            document.body.innerHTML = data.html || '';
            wrapInteractiveWords(document.body);

            if (document.fonts && !document.fonts.check('400 16px "Google Sans Flex"')) {
                document.fonts.load('400 16px "Google Sans Flex"').then(
                    function() { applyStreamingFitCore(data, prevWordCount, prevRenderCount); },
                    function() { applyStreamingFitCore(data, prevWordCount, prevRenderCount); }
                );
                return;
            }
            applyStreamingFitCore(data, prevWordCount, prevRenderCount);
        }

        function applyStreamingFitCore(data, prevWordCount, prevRenderCount) {

            const body = document.body;
            const doc = document.documentElement;
            if (!body || !doc) return;

            const winH = window.innerHeight;
            const winW = window.innerWidth;
            const isConstrainedWindow = (winH < 260 || winW < 420);
            const text = (body.innerText || body.textContent || '').trim();
            const textLen = text.length;
            const isNewSession = (prevRenderCount === 0 || (prevWordCount < 5 && textLen < 50));
            const isConstrainedShortContent = isConstrainedWindow && textLen < 450;

            function currentLineHeightPx() {
                const computed = window.getComputedStyle(body);
                const fontSize = parseFloat(computed.fontSize) || parseFloat(body.style.fontSize) || 14;
                let lineHeight = parseFloat(computed.lineHeight);
                if (!Number.isFinite(lineHeight)) {
                    const inlineLineHeight = parseFloat(body.style.lineHeight);
                    lineHeight = fontSize * (Number.isFinite(inlineLineHeight) ? inlineLineHeight : 1.5);
                }
                return Math.max(1, lineHeight);
            }

            function hasPathologicalWrap() {
                if (textLen < 8) return false;
                const tokens = text.split(/\s+/).filter(Boolean);
                const wordCount = tokens.length;
                let longestToken = 0;
                for (let index = 0; index < tokens.length; index += 1) {
                    longestToken = Math.max(longestToken, tokens[index].length);
                }
                const approxLineCount = Math.max(1, Math.round(doc.scrollHeight / currentLineHeightPx()));
                const avgCharsPerLine = textLen / approxLineCount;
                return avgCharsPerLine < 3.5 &&
                    approxLineCount > Math.max(3, wordCount + 1) &&
                    (wordCount <= 12 || longestToken >= 4);
            }

            function fitsVertically() {
                void body.offsetHeight;
                return doc.scrollHeight <= (winH + 2) && !hasPathologicalWrap();
            }

            const minSize = (textLen < 200) ? 6 : 14;

            if (isNewSession) {
                const maxPossible = Math.min(isConstrainedWindow ? 84 : 110, winH);
                const estimated = Math.sqrt((winW * winH) / (textLen + 1));
                let low = Math.max(minSize, Math.floor(estimated * 0.5));
                let high = Math.min(maxPossible, Math.ceil(estimated * 1.15));
                if (low > high) low = high;

                body.style.fontVariationSettings = "'wght' 400, 'wdth' 90, 'slnt' 0, 'ROND' 100";
                body.style.letterSpacing = '0px';
                body.style.wordSpacing = '0px';
                body.style.lineHeight = '1.5';
                body.style.paddingTop = '0';
                body.style.paddingBottom = '0';

                const blocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
                for (let index = 0; index < blocks.length; index += 1) {
                    blocks[index].style.marginBottom = '0.5em';
                    blocks[index].style.paddingBottom = '0';
                }

                void body.offsetHeight;
                let best = low;
                while (low <= high) {
                    const mid = Math.floor((low + high) / 2);
                    body.style.fontSize = mid + 'px';
                    if (fitsVertically()) {
                        best = mid;
                        low = mid + 1;
                    } else {
                        high = mid - 1;
                    }
                }
                if (best < minSize) best = minSize;
                body.style.fontSize = best + 'px';

                if (isConstrainedShortContent) {
                    void body.offsetHeight;
                    let settleLow = minSize;
                    let settleHigh = best;
                    let settleBest = minSize;
                    while (settleLow <= settleHigh) {
                        const settleMid = Math.floor((settleLow + settleHigh) / 2);
                        body.style.fontSize = settleMid + 'px';
                        if (fitsVertically()) {
                            settleBest = settleMid;
                            settleLow = settleMid + 1;
                        } else {
                            settleHigh = settleMid - 1;
                        }
                    }
                    body.style.fontSize = settleBest + 'px';
                }
            } else {
                const hasOverflow = !fitsVertically();
                if (hasOverflow) {
                    const currentSize = parseFloat(body.style.fontSize) || 14;
                    if (currentSize > minSize) {
                        let low = minSize;
                        let high = currentSize;
                        let best = minSize;
                        while (low <= high) {
                            const mid = Math.floor((low + high) / 2);
                            body.style.fontSize = mid + 'px';
                            if (fitsVertically()) {
                                best = mid;
                                low = mid + 1;
                            } else {
                                high = mid - 1;
                            }
                        }
                        body.style.fontSize = best + 'px';
                    }
                }
            }

            const words = document.querySelectorAll('.word');
            const newWordCount = words.length;

            // ===== ADAPTIVE WORD-BY-WORD REVEAL =====
            // Queue new words and release on rAF at a backlog-adaptive rate.
            // Low backlog → smooth ~25ms/word feel; bursts accelerate so the
            // displayed text catches up to the model without hiding throughput
            // behind animation. Credit accumulation gives a ~16ms floor.
            if (!window._streamRevealState) {
                window._streamRevealState = {
                    queue: [],
                    active: false,
                    lastRevealedIndex: -1,
                    lastTick: 0,
                    credits: 0
                };
            }
            const revealState = window._streamRevealState;

            if (isNewSession) {
                revealState.queue = [];
                revealState.active = false;
                revealState.lastRevealedIndex = newWordCount - 1;
                revealState.credits = 0;
            } else {
                // Word-centric hide: display:none removes the word from LAYOUT
                // entirely so scrollHeight reflects only REVEALED content. This
                // lets font-size shrink gradually per-word as reveal progresses.
                revealState.queue = [];
                const revealStart = Math.max(0, revealState.lastRevealedIndex + 1);
                for (let rv = revealStart; rv < newWordCount; rv += 1) {
                    const rw = words[rv];
                    if (!rw) continue;
                    rw.style.display = 'none';
                    rw.style.opacity = '0';
                    rw.style.filter = 'blur(3px)';
                    rw.style.transform = 'translateY(14px)';
                    rw.style.transition = 'opacity 0.35s ease-out, filter 0.35s ease-out, transform 0.4s cubic-bezier(0.16, 1, 0.3, 1)';
                    revealState.queue.push({ el: rw, index: rv });
                }

                if (revealState.queue.length > 0 && !revealState.active) {
                    revealState.active = true;
                    revealState.lastTick = performance.now();
                    revealState.credits = 1;
                    const SMOOTH_WPS = 40;
                    const CATCH_THRESHOLD = 10;
                    const BATCH_CAP = 64;
                    const tick = function(now) {
                        const q = revealState.queue;
                        if (!q || q.length === 0) {
                            revealState.active = false;
                            revealState.credits = 0;
                            return;
                        }
                        let dt = now - revealState.lastTick;
                        if (dt < 0) dt = 0;
                        revealState.lastTick = now;
                        const backlog = q.length;
                        const targetWps = SMOOTH_WPS * (1 + backlog / CATCH_THRESHOLD);
                        revealState.credits += targetWps * dt / 1000;
                        let emitted = 0;
                        while (revealState.credits >= 1 && q.length > 0 && emitted < BATCH_CAP) {
                            const item = q.shift();
                            if (item.el && item.el.isConnected) {
                                item.el.style.display = '';
                                void item.el.offsetWidth;
                                item.el.style.opacity = '1';
                                item.el.style.filter = 'blur(0)';
                                item.el.style.transform = 'translateY(0)';
                            }
                            revealState.lastRevealedIndex = item.index;
                            revealState.credits -= 1;
                            emitted += 1;
                        }

                        // Font-size handled by the native fit algorithm only —
                        // per-tick shrinks here caused measurement/transition
                        // feedback loops that manifested as zoom oscillation.
                        requestAnimationFrame(tick);
                    };
                    requestAnimationFrame(tick);
                }
            }

            if (body.style.opacity === '0') body.style.opacity = '1';
            window._streamWordCount = newWordCount;
            window._streamRenderCount = prevRenderCount + 1;
            window.scrollTo({ top: document.body.scrollHeight, behavior: 'smooth' });
            schedulePostTableFit(true);
        }

        window.applyResultState = function(raw) {
            const data = typeof raw === 'string' ? JSON.parse(raw) : raw;
            if (data.streaming) {
                applyStreamingResultState(data);
            } else {
                applyFinalResultState(data);
            }
        };

        window.navigateHistory = function(direction) {
            if (!activeWindowId) return;
            postJson({ type: direction === 'back' ? 'navigateBack' : 'navigateForward', windowId: activeWindowId });
        };
    """.trimIndent()
}
