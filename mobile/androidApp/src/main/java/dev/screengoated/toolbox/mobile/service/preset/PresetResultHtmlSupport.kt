package dev.screengoated.toolbox.mobile.service.preset

internal fun presetResultBaseHtmlTemplate(): String {
    return """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
            <style>{{FONT_CSS}}</style>
            <style>{{THEME_CSS}}</style>
            <style>{{MARKDOWN_CSS}}</style>
            <style>{{WINDOW_CHROME_CSS}}</style>
            <link href="{{GRIDJS_CSS_URL}}" rel="stylesheet" />
            <style>{{GRIDJS_CSS}}</style>
        </head>
        <body></body>
        <script src="{{GRIDJS_JS_URL}}"></script>
        <script>{{FIT_SCRIPT}}</script>
        <script>
            window.ipc = {
                postMessage(message) {
                    if (window.sgtAndroid && window.sgtAndroid.postMessage) {
                        window.sgtAndroid.postMessage(String(message));
                    }
                }
            };
            {{GRIDJS_INIT_SCRIPT}}
            {{RESULT_JS}}
        </script>
        </html>
    """.trimIndent()
}

internal fun presetResultCss(isDark: Boolean): String {
    val shellBg = if (isDark) "rgba(18, 20, 28, 0.86)" else "rgba(252, 252, 255, 0.88)"
    val shellBorder = if (isDark) "rgba(255, 255, 255, 0.12)" else "rgba(10, 18, 28, 0.10)"
    return """
        html {
            width: 100%;
            height: 100%;
            background: transparent;
            overflow-y: hidden;
            overflow-x: hidden;
            touch-action: manipulation;
            -webkit-tap-highlight-color: transparent;
            -webkit-touch-callout: default;
            scrollbar-width: none;
        }
        body {
            position: relative;
            width: 100%;
            min-height: 100%;
            margin: 0;
            padding: 0;
            overflow-y: hidden;
            overflow-x: hidden;
            -webkit-overflow-scrolling: touch;
            scrollbar-width: none;
            user-select: text;
            -webkit-user-select: text;
            border-radius: 14px;
            border: 1px solid $shellBorder;
            background: $shellBg;
            backdrop-filter: blur(18px);
            -webkit-backdrop-filter: blur(18px);
            box-shadow: 0 20px 48px rgba(0, 0, 0, 0.26);
        }
        html::-webkit-scrollbar, body::-webkit-scrollbar { display: none; }
        body > *:first-child { margin-top: 0; }
        a { cursor: pointer; }
    """.trimIndent()
}

internal fun presetResultJavascript(): String {
    return """
        let activeWindowId = null;
        let dragState = null;
        let resizeState = null;
        let holdTimer = null;
        let pendingStart = null;
        let twoFingerScrollState = null;
        const DRAG_THRESHOLD_PX = 6;
        const RESIZE_ZONE_PX = 48;
        const DRAG_GAIN = 2.25;
        const RESIZE_GAIN = 1.85;
        const INTERACTIVE_WORD_WRAP_CHAR_LIMIT = 6000;
        const INTERACTIVE_WORD_WRAP_WORD_LIMIT = 900;
        const SELECTION_GUARD_MS = 260;
        const INERTIA_MIN_VELOCITY = 0.15;
        const INERTIA_FRICTION = 0.92;
        let inertiaFrame = null;

        function postJson(payload) {
            window.ipc.postMessage(JSON.stringify(payload));
        }

        function stopInertiaScroll() {
            if (inertiaFrame) {
                cancelAnimationFrame(inertiaFrame);
                inertiaFrame = null;
            }
        }

        function debugGesture(phase, extra) {
            try {
                postJson({
                    type: 'gestureDebug',
                    phase: phase,
                    activeWindowId: activeWindowId,
                    ...extra
                });
            } catch (error) {
                window.ipc.postMessage('gesture_debug_error:' + String(error));
            }
        }

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

        function bodyRect() {
            return document.body.getBoundingClientRect();
        }

        function detectResizeCorner(clientX, clientY) {
            const localX = clientX;
            const localY = clientY;
            if (localY < window.innerHeight - RESIZE_ZONE_PX) {
                return null;
            }
            if (localX < RESIZE_ZONE_PX) return 'bl';
            if (localX > window.innerWidth - RESIZE_ZONE_PX) return 'br';
            return null;
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
            if (!target) {
                return false;
            }
            if (target.nodeType === Node.TEXT_NODE) {
                return true;
            }
            const element = normalizeTarget(target);
            if (!element || !element.closest) {
                return false;
            }
            if (element.closest('a, button, input, textarea, select, canvas, video, audio, iframe, [contenteditable="true"]')) {
                return true;
            }
            if (element.closest('span.word, code, pre, td, th')) {
                return true;
            }
            const text = (element.textContent || '').trim();
            if (text.length < 2) {
                return false;
            }
            const computed = window.getComputedStyle(element);
            const userSelect = computed.userSelect || computed.webkitUserSelect || '';
            return userSelect !== 'none' && text.length <= 80;
        }

        function findScrollableContainer(target) {
            let element = normalizeTarget(target);
            while (element && element !== document.body && element !== document.documentElement) {
                const style = window.getComputedStyle(element);
                const canScrollY =
                    (style.overflowY === 'auto' || style.overflowY === 'scroll') &&
                    element.scrollHeight > element.clientHeight + 1;
                const canScrollX =
                    (style.overflowX === 'auto' || style.overflowX === 'scroll') &&
                    element.scrollWidth > element.clientWidth + 1;
                if (canScrollY || canScrollX) {
                    return element;
                }
                element = element.parentElement;
            }
            return document.scrollingElement || document.documentElement;
        }

        function startTwoFingerScroll(event) {
            stopInertiaScroll();
            if (event.touches.length < 2) {
                twoFingerScrollState = null;
                return;
            }
            const first = event.touches[0];
            const second = event.touches[1];
            const now = performance.now();
            twoFingerScrollState = {
                x: (first.clientX + second.clientX) / 2,
                y: (first.clientY + second.clientY) / 2,
                container: findScrollableContainer(event.target),
                vx: 0,
                vy: 0,
                lastTs: now
            };
        }

        function updateTwoFingerScroll(event) {
            if (event.touches.length < 2) {
                twoFingerScrollState = null;
                return false;
            }
            const first = event.touches[0];
            const second = event.touches[1];
            const nextX = (first.clientX + second.clientX) / 2;
            const nextY = (first.clientY + second.clientY) / 2;
            if (!twoFingerScrollState) {
                startTwoFingerScroll(event);
                return false;
            }
            const dx = nextX - twoFingerScrollState.x;
            const dy = nextY - twoFingerScrollState.y;
            const now = performance.now();
            const dt = Math.max(1, now - (twoFingerScrollState.lastTs || now));
            const container = twoFingerScrollState.container || findScrollableContainer(event.target);
            if (container && typeof container.scrollBy === 'function') {
                container.scrollBy(-dx, -dy);
            } else {
                window.scrollBy(-dx, -dy);
            }
            twoFingerScrollState.x = nextX;
            twoFingerScrollState.y = nextY;
            twoFingerScrollState.vx = dx / dt;
            twoFingerScrollState.vy = dy / dt;
            twoFingerScrollState.lastTs = now;
            return Math.abs(dx) > 0 || Math.abs(dy) > 0;
        }

        function startInertiaScroll(state) {
            if (!state) return;
            const container = state.container;
            let vx = state.vx || 0;
            let vy = state.vy || 0;
            if (Math.hypot(vx, vy) < INERTIA_MIN_VELOCITY) {
                return;
            }
            let lastTs = performance.now();
            function tick(now) {
                const dt = Math.max(1, now - lastTs);
                lastTs = now;
                const moveX = vx * dt;
                const moveY = vy * dt;
                if (container && typeof container.scrollBy === 'function') {
                    container.scrollBy(-moveX, -moveY);
                } else {
                    window.scrollBy(-moveX, -moveY);
                }
                vx *= Math.pow(INERTIA_FRICTION, dt / 16.0);
                vy *= Math.pow(INERTIA_FRICTION, dt / 16.0);
                if (Math.hypot(vx, vy) < INERTIA_MIN_VELOCITY) {
                    inertiaFrame = null;
                    return;
                }
                inertiaFrame = requestAnimationFrame(tick);
            }
            inertiaFrame = requestAnimationFrame(tick);
        }

        function beginDrag(point) {
            if (!pendingStart) return;
            dragState = { x: point.screenX, y: point.screenY };
            activateWindow();
        }

        function hasActiveSelection() {
            if (!window.getSelection) return false;
            const selection = window.getSelection();
            return !!selection && String(selection).trim().length > 0;
        }

        function runFit(streaming) {
            if (window.runWindowsMarkdownFit) {
                window.runWindowsMarkdownFit(!!streaming, streaming ? 'mobile_streaming_fit' : 'mobile_final_fit');
            }
        }

        function resetStreamCounters() {
            window._streamWordCount = 0;
            window._streamRenderCount = 0;
        }

        function shouldEnableInteractiveWordWrap(text) {
            if ((navigator.maxTouchPoints || 0) > 0 || 'ontouchstart' in window) {
                return false;
            }
            if (!text || text.length > INTERACTIVE_WORD_WRAP_CHAR_LIMIT) {
                return false;
            }
            const words = text.trim() ? text.trim().split(/\s+/) : [];
            return words.length <= INTERACTIVE_WORD_WRAP_WORD_LIMIT;
        }

        function shouldSkipWordWrap(node) {
            const parent = node.parentElement;
            if (!parent) return true;
            return !!parent.closest('pre, code, table, script, style');
        }

        function wrapInteractiveWords(root) {
            if (!root || root.querySelector('.word')) {
                return;
            }
            const text = (root.innerText || root.textContent || '').trim();
            if (!shouldEnableInteractiveWordWrap(text)) {
                return;
            }

            const walker = document.createTreeWalker(
                root,
                NodeFilter.SHOW_TEXT,
                {
                    acceptNode(node) {
                        if (!node.nodeValue || !node.nodeValue.trim()) {
                            return NodeFilter.FILTER_REJECT;
                        }
                        return shouldSkipWordWrap(node)
                            ? NodeFilter.FILTER_REJECT
                            : NodeFilter.FILTER_ACCEPT;
                    }
                }
            );

            const textNodes = [];
            while (walker.nextNode()) {
                textNodes.push(walker.currentNode);
            }

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
            if (!document.body.querySelector('table')) {
                return;
            }
            setTimeout(() => runFit(streaming), 250);
        }

        function applyBodyHtml(html) {
            document.body.innerHTML = html || '';
            wrapInteractiveWords(document.body);
        }

        function applyFinalResultState(raw) {
            const data = typeof raw === 'string' ? JSON.parse(raw) : raw;
            activeWindowId = data.windowId;
            applyBodyHtml(data.html);
            resetStreamCounters();
            runFit(!!data.streaming);
            schedulePostTableFit(!!data.streaming);
        }

        function applyStreamingResultState(raw) {
            const data = typeof raw === 'string' ? JSON.parse(raw) : raw;
            activeWindowId = data.windowId;
            const sourceTextLen = Number.isFinite(data.sourceTextLen) ? data.sourceTextLen : 0;
            const sourceTrimmedLen = Number.isFinite(data.sourceTrimmedLen) ? data.sourceTrimmedLen : sourceTextLen;
            const prevWordCount = window._streamWordCount || 0;
            const prevRenderCount = window._streamRenderCount || 0;

            document.body.innerHTML = data.html || '';
            wrapInteractiveWords(document.body);

            const body = document.body;
            const doc = document.documentElement;
            if (!body || !doc) {
                return;
            }

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
                if (textLen < 8) {
                    return false;
                }
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
                if (low > high) {
                    low = high;
                }

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
                if (best < minSize) {
                    best = minSize;
                }
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
            if (!isNewSession) {
                const newWords = [];
                for (let index = prevWordCount; index < newWordCount; index += 1) {
                    newWords.push(words[index]);
                }
                if (newWords.length > 0) {
                    newWords.forEach(word => {
                        word.style.opacity = '0';
                        word.style.filter = 'blur(2px)';
                    });
                    requestAnimationFrame(() => {
                        newWords.forEach(word => {
                            word.style.transition = 'opacity 0.35s ease-out, filter 0.35s ease-out';
                            word.style.opacity = '1';
                            word.style.filter = 'blur(0)';
                        });
                    });
                }
            }

            if (body.style.opacity === '0') {
                body.style.opacity = '1';
            }

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

        document.addEventListener('click', event => {
            if (!document.body.contains(event.target)) return;
            activateWindow();
            const link = event.target && event.target.closest && event.target.closest('a[href]');
            if (link) {
                sendNavigationState(1, 1, true);
            }
        }, true);

        document.addEventListener('touchstart', event => {
            if (event.touches.length > 1) {
                dragState = null;
                resizeState = null;
                pendingStart = null;
                clearHoldTimer();
                startTwoFingerScroll(event);
                debugGesture('touchstart_multi', { touches: event.touches.length });
                return;
            }
            if (event.touches.length !== 1) return;
            const point = touchPoint(event);
            stopInertiaScroll();
            twoFingerScrollState = null;
            const target = normalizeTarget(event.target);
            const interactive = target && target.closest && target.closest('a, button, input, textarea, select');
            const resizeCorner = detectResizeCorner(point.clientX, point.clientY);
            if (resizeCorner) {
                resizeState = { corner: resizeCorner, x: point.screenX, y: point.screenY };
                pendingStart = null;
                clearHoldTimer();
                activateWindow();
                debugGesture('touchstart_resize', {
                    x: Math.round(point.screenX),
                    y: Math.round(point.screenY),
                    target: target ? target.tagName || target.nodeName : null,
                    resizeCorner: resizeCorner
                });
                return;
            }
            if (interactive) {
                pendingStart = null;
                clearHoldTimer();
                debugGesture('touchstart_interactive', {
                    target: target ? target.tagName || target.nodeName : null
                });
                return;
            }
            const selectionTarget = isSelectionTarget(target);
            pendingStart = {
                x: point.screenX,
                y: point.screenY,
                selectionTarget: selectionTarget,
                startedAt: Date.now(),
            };
            clearHoldTimer();
            debugGesture('touchstart_pending', {
                x: Math.round(point.screenX),
                y: Math.round(point.screenY),
                target: target ? target.tagName || target.nodeName : null,
                selectionTarget: selectionTarget
            });
        }, { passive: true });

        document.addEventListener('touchmove', event => {
            if (event.touches.length > 1) {
                dragState = null;
                resizeState = null;
                pendingStart = null;
                clearHoldTimer();
                if (updateTwoFingerScroll(event) && event.cancelable) event.preventDefault();
                debugGesture('touchmove_multi', { touches: event.touches.length });
                return;
            }
            const point = touchPoint(event);
            if (resizeState) {
                const dx = Math.round((point.screenX - resizeState.x) * RESIZE_GAIN);
                const dy = Math.round((point.screenY - resizeState.y) * RESIZE_GAIN);
                if (dx !== 0 || dy !== 0) {
                    debugGesture('touchmove_resize', { dx: dx, dy: dy, corner: resizeState.corner });
                    postJson({ type: 'resizeResultWindow', windowId: activeWindowId, corner: resizeState.corner, dx, dy });
                    resizeState.x = point.screenX;
                    resizeState.y = point.screenY;
                    activateWindow();
                    if (event.cancelable) event.preventDefault();
                }
                return;
            }
            if (!pendingStart && !dragState) return;
            const movedEnough = pendingStart &&
                (Math.abs(point.screenX - pendingStart.x) > DRAG_THRESHOLD_PX || Math.abs(point.screenY - pendingStart.y) > DRAG_THRESHOLD_PX);
            if (!dragState && movedEnough) {
                clearHoldTimer();
                if (pendingStart.selectionTarget && hasActiveSelection()) {
                    debugGesture('touchmove_selection_active', {
                        selectionText: window.getSelection ? String(window.getSelection()) : ''
                    });
                    pendingStart = null;
                    return;
                }
                if (pendingStart.selectionTarget && (Date.now() - pendingStart.startedAt) >= SELECTION_GUARD_MS) {
                    debugGesture('touchmove_selection_guard', {
                        heldMs: Date.now() - pendingStart.startedAt
                    });
                    pendingStart = null;
                    return;
                }
                debugGesture('touchmove_begin_drag', {
                    dx: Math.round(point.screenX - pendingStart.x),
                    dy: Math.round(point.screenY - pendingStart.y),
                    selectionTarget: pendingStart.selectionTarget
                });
                beginDrag(point);
            }
            if (!dragState) return;
            const dx = Math.round((point.screenX - dragState.x) * DRAG_GAIN);
            const dy = Math.round((point.screenY - dragState.y) * DRAG_GAIN);
            if (dx !== 0 || dy !== 0) {
                debugGesture('touchmove_drag', { dx: dx, dy: dy });
                postJson({ type: 'dragResultWindow', windowId: activeWindowId, dx, dy });
                postJson({ type: 'dragResultWindowAt', windowId: activeWindowId, x: Math.round(point.screenX), y: Math.round(point.screenY) });
                dragState.x = point.screenX;
                dragState.y = point.screenY;
                if (event.cancelable) event.preventDefault();
            }
        }, { passive: false });

        document.addEventListener('touchend', event => {
            const point = touchPoint(event);
            const inertiaState = twoFingerScrollState;
            if (dragState) {
                debugGesture('touchend_drag', { x: Math.round(point.screenX), y: Math.round(point.screenY) });
                postJson({ type: 'dragResultWindowEnd', windowId: activeWindowId, x: Math.round(point.screenX), y: Math.round(point.screenY) });
            } else if (resizeState) {
                debugGesture('touchend_resize', { corner: resizeState.corner });
                postJson({ type: 'resizeResultWindowEnd', windowId: activeWindowId });
            } else {
                debugGesture('touchend_idle', {
                    selectionText: window.getSelection ? String(window.getSelection()) : ''
                });
            }
            dragState = null;
            resizeState = null;
            pendingStart = null;
            twoFingerScrollState = null;
            clearHoldTimer();
            if (inertiaState) {
                startInertiaScroll(inertiaState);
            }
        }, { passive: true });

        document.addEventListener('touchcancel', () => {
            debugGesture('touchcancel', {});
            postJson({ type: 'cancelResultGesture', windowId: activeWindowId });
            dragState = null;
            resizeState = null;
            pendingStart = null;
            twoFingerScrollState = null;
            clearHoldTimer();
        }, { passive: true });

        document.addEventListener('selectionchange', () => {
            const selectionText = window.getSelection ? String(window.getSelection()) : '';
            if (selectionText && selectionText.trim().length > 0) {
                debugGesture('selectionchange', { selectionText: selectionText });
            }
        });

        window.addEventListener('popstate', () => {
            if (window.history.length <= 1) {
                sendNavigationState(0, 0, false);
            }
        });

        window.ipc.postMessage('result_ready');
    """.trimIndent()
}

internal fun presetResultInteractionJavascript(): String {
    return """
        let activeWindowId = null;
        let dragState = null;
        let resizeState = null;
        let holdTimer = null;
        let pendingStart = null;
        let twoFingerScrollState = null;
        const DRAG_THRESHOLD_PX = 6;
        const RESIZE_ZONE_PX = 48;
        const DRAG_GAIN = 2.25;
        const RESIZE_GAIN = 1.85;
        const SELECTION_GUARD_MS = 260;
        const INERTIA_MIN_VELOCITY = 0.15;
        const INERTIA_FRICTION = 0.92;
        let inertiaFrame = null;

        function postJson(payload) {
            window.ipc.postMessage(JSON.stringify(payload));
        }

        function stopInertiaScroll() {
            if (inertiaFrame) {
                cancelAnimationFrame(inertiaFrame);
                inertiaFrame = null;
            }
        }

        function debugGesture(phase, extra) {
            try {
                postJson({
                    type: 'gestureDebug',
                    phase: phase,
                    activeWindowId: activeWindowId,
                    ...extra
                });
            } catch (error) {
                window.ipc.postMessage('gesture_debug_error:' + String(error));
            }
        }

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

        function bodyRect() {
            return document.body.getBoundingClientRect();
        }

        function detectResizeCorner(clientX, clientY) {
            const localX = clientX;
            const localY = clientY;
            if (localY < window.innerHeight - RESIZE_ZONE_PX) {
                return null;
            }
            if (localX < RESIZE_ZONE_PX) return 'bl';
            if (localX > window.innerWidth - RESIZE_ZONE_PX) return 'br';
            return null;
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
            if (!target) {
                return false;
            }
            if (target.nodeType === Node.TEXT_NODE) {
                return true;
            }
            const element = normalizeTarget(target);
            if (!element || !element.closest) {
                return false;
            }
            if (element.closest('a, button, input, textarea, select, canvas, video, audio, iframe, [contenteditable="true"]')) {
                return true;
            }
            if (element.closest('span.word, code, pre, td, th')) {
                return true;
            }
            const text = (element.textContent || '').trim();
            if (text.length < 2) {
                return false;
            }
            const computed = window.getComputedStyle(element);
            const userSelect = computed.userSelect || computed.webkitUserSelect || '';
            return userSelect !== 'none' && text.length <= 80;
        }

        function findScrollableContainer(target) {
            let element = normalizeTarget(target);
            while (element && element !== document.body && element !== document.documentElement) {
                const style = window.getComputedStyle(element);
                const canScrollY =
                    (style.overflowY === 'auto' || style.overflowY === 'scroll') &&
                    element.scrollHeight > element.clientHeight + 1;
                const canScrollX =
                    (style.overflowX === 'auto' || style.overflowX === 'scroll') &&
                    element.scrollWidth > element.clientWidth + 1;
                if (canScrollY || canScrollX) {
                    return element;
                }
                element = element.parentElement;
            }
            return document.scrollingElement || document.documentElement;
        }

        function startTwoFingerScroll(event) {
            stopInertiaScroll();
            if (event.touches.length < 2) {
                twoFingerScrollState = null;
                return;
            }
            const first = event.touches[0];
            const second = event.touches[1];
            const now = performance.now();
            twoFingerScrollState = {
                x: (first.clientX + second.clientX) / 2,
                y: (first.clientY + second.clientY) / 2,
                container: findScrollableContainer(event.target),
                vx: 0,
                vy: 0,
                lastTs: now
            };
        }

        function updateTwoFingerScroll(event) {
            if (event.touches.length < 2) {
                twoFingerScrollState = null;
                return false;
            }
            const first = event.touches[0];
            const second = event.touches[1];
            const nextX = (first.clientX + second.clientX) / 2;
            const nextY = (first.clientY + second.clientY) / 2;
            if (!twoFingerScrollState) {
                startTwoFingerScroll(event);
                return false;
            }
            const dx = nextX - twoFingerScrollState.x;
            const dy = nextY - twoFingerScrollState.y;
            const now = performance.now();
            const dt = Math.max(1, now - (twoFingerScrollState.lastTs || now));
            const container = twoFingerScrollState.container || findScrollableContainer(event.target);
            if (container && typeof container.scrollBy === 'function') {
                container.scrollBy(-dx, -dy);
            } else {
                window.scrollBy(-dx, -dy);
            }
            twoFingerScrollState.x = nextX;
            twoFingerScrollState.y = nextY;
            twoFingerScrollState.vx = dx / dt;
            twoFingerScrollState.vy = dy / dt;
            twoFingerScrollState.lastTs = now;
            return Math.abs(dx) > 0 || Math.abs(dy) > 0;
        }

        function startInertiaScroll(state) {
            if (!state) return;
            const container = state.container;
            let vx = state.vx || 0;
            let vy = state.vy || 0;
            if (Math.hypot(vx, vy) < INERTIA_MIN_VELOCITY) {
                return;
            }
            let lastTs = performance.now();
            function tick(now) {
                const dt = Math.max(1, now - lastTs);
                lastTs = now;
                const moveX = vx * dt;
                const moveY = vy * dt;
                if (container && typeof container.scrollBy === 'function') {
                    container.scrollBy(-moveX, -moveY);
                } else {
                    window.scrollBy(-moveX, -moveY);
                }
                vx *= Math.pow(INERTIA_FRICTION, dt / 16.0);
                vy *= Math.pow(INERTIA_FRICTION, dt / 16.0);
                if (Math.hypot(vx, vy) < INERTIA_MIN_VELOCITY) {
                    inertiaFrame = null;
                    return;
                }
                inertiaFrame = requestAnimationFrame(tick);
            }
            inertiaFrame = requestAnimationFrame(tick);
        }

        function beginDrag(point) {
            if (!pendingStart) return;
            dragState = { x: point.screenX, y: point.screenY };
            activateWindow();
        }

        function hasActiveSelection() {
            if (!window.getSelection) return false;
            const selection = window.getSelection();
            return !!selection && String(selection).trim().length > 0;
        }

        document.addEventListener('click', event => {
            if (!document.body.contains(event.target)) return;
            activateWindow();
            const link = event.target && event.target.closest && event.target.closest('a[href]');
            if (link) {
                sendNavigationState(1, 1, true);
            }
        }, true);

        document.addEventListener('touchstart', event => {
            if (event.touches.length > 1) {
                dragState = null;
                resizeState = null;
                pendingStart = null;
                clearHoldTimer();
                startTwoFingerScroll(event);
                debugGesture('touchstart_multi', { touches: event.touches.length });
                return;
            }
            if (event.touches.length !== 1) return;
            const point = touchPoint(event);
            activeWindowId = activeWindowId || document.body.getAttribute('data-window-id');
            stopInertiaScroll();
            twoFingerScrollState = null;
            const target = normalizeTarget(event.target);
            const interactive = target && target.closest && target.closest('a, button, input, textarea, select, canvas');
            const resizeCorner = detectResizeCorner(point.clientX, point.clientY);
            if (resizeCorner) {
                resizeState = { corner: resizeCorner, x: point.screenX, y: point.screenY };
                pendingStart = null;
                clearHoldTimer();
                activateWindow();
                debugGesture('touchstart_resize', {
                    x: Math.round(point.screenX),
                    y: Math.round(point.screenY),
                    target: target ? target.tagName || target.nodeName : null,
                    resizeCorner: resizeCorner
                });
                return;
            }
            if (interactive) {
                pendingStart = null;
                clearHoldTimer();
                debugGesture('touchstart_interactive', {
                    target: target ? target.tagName || target.nodeName : null
                });
                return;
            }
            const selectionTarget = isSelectionTarget(target);
            pendingStart = {
                x: point.screenX,
                y: point.screenY,
                selectionTarget: selectionTarget,
                startedAt: Date.now(),
            };
            clearHoldTimer();
            debugGesture('touchstart_pending', {
                x: Math.round(point.screenX),
                y: Math.round(point.screenY),
                target: target ? target.tagName || target.nodeName : null,
                selectionTarget: selectionTarget
            });
        }, { passive: true });

        document.addEventListener('touchmove', event => {
            if (event.touches.length > 1) {
                dragState = null;
                resizeState = null;
                pendingStart = null;
                clearHoldTimer();
                if (updateTwoFingerScroll(event) && event.cancelable) event.preventDefault();
                debugGesture('touchmove_multi', { touches: event.touches.length });
                return;
            }
            const point = touchPoint(event);
            if (resizeState) {
                const dx = Math.round((point.screenX - resizeState.x) * RESIZE_GAIN);
                const dy = Math.round((point.screenY - resizeState.y) * RESIZE_GAIN);
                if (dx !== 0 || dy !== 0) {
                    debugGesture('touchmove_resize', { dx: dx, dy: dy, corner: resizeState.corner });
                    postJson({ type: 'resizeResultWindow', windowId: activeWindowId, corner: resizeState.corner, dx, dy });
                    resizeState.x = point.screenX;
                    resizeState.y = point.screenY;
                    activateWindow();
                    if (event.cancelable) event.preventDefault();
                }
                return;
            }
            if (!pendingStart && !dragState) return;
            const movedEnough = pendingStart &&
                (Math.abs(point.screenX - pendingStart.x) > DRAG_THRESHOLD_PX || Math.abs(point.screenY - pendingStart.y) > DRAG_THRESHOLD_PX);
            if (!dragState && movedEnough) {
                clearHoldTimer();
                if (pendingStart.selectionTarget && hasActiveSelection()) {
                    debugGesture('touchmove_selection_active', {
                        selectionText: window.getSelection ? String(window.getSelection()) : ''
                    });
                    pendingStart = null;
                    return;
                }
                if (pendingStart.selectionTarget && (Date.now() - pendingStart.startedAt) >= SELECTION_GUARD_MS) {
                    debugGesture('touchmove_selection_guard', {
                        heldMs: Date.now() - pendingStart.startedAt
                    });
                    pendingStart = null;
                    return;
                }
                debugGesture('touchmove_begin_drag', {
                    dx: Math.round(point.screenX - pendingStart.x),
                    dy: Math.round(point.screenY - pendingStart.y),
                    selectionTarget: pendingStart.selectionTarget
                });
                beginDrag(point);
            }
            if (!dragState) return;
            const dx = Math.round((point.screenX - dragState.x) * DRAG_GAIN);
            const dy = Math.round((point.screenY - dragState.y) * DRAG_GAIN);
            if (dx !== 0 || dy !== 0) {
                debugGesture('touchmove_drag', { dx: dx, dy: dy });
                postJson({ type: 'dragResultWindow', windowId: activeWindowId, dx, dy });
                postJson({ type: 'dragResultWindowAt', windowId: activeWindowId, x: Math.round(point.screenX), y: Math.round(point.screenY) });
                dragState.x = point.screenX;
                dragState.y = point.screenY;
                if (event.cancelable) event.preventDefault();
            }
        }, { passive: false });

        document.addEventListener('touchend', event => {
            const point = touchPoint(event);
            const inertiaState = twoFingerScrollState;
            if (dragState) {
                debugGesture('touchend_drag', { x: Math.round(point.screenX), y: Math.round(point.screenY) });
                postJson({ type: 'dragResultWindowEnd', windowId: activeWindowId, x: Math.round(point.screenX), y: Math.round(point.screenY) });
            } else if (resizeState) {
                debugGesture('touchend_resize', { corner: resizeState.corner });
                postJson({ type: 'resizeResultWindowEnd', windowId: activeWindowId });
            } else {
                debugGesture('touchend_idle', {
                    selectionText: window.getSelection ? String(window.getSelection()) : ''
                });
            }
            dragState = null;
            resizeState = null;
            pendingStart = null;
            twoFingerScrollState = null;
            clearHoldTimer();
            if (inertiaState) {
                startInertiaScroll(inertiaState);
            }
        }, { passive: true });

        document.addEventListener('touchcancel', () => {
            debugGesture('touchcancel', {});
            postJson({ type: 'cancelResultGesture', windowId: activeWindowId });
            dragState = null;
            resizeState = null;
            pendingStart = null;
            twoFingerScrollState = null;
            clearHoldTimer();
        }, { passive: true });

        document.addEventListener('selectionchange', () => {
            const selectionText = window.getSelection ? String(window.getSelection()) : '';
            if (selectionText && selectionText.trim().length > 0) {
                debugGesture('selectionchange', { selectionText: selectionText });
            }
        });

        window.addEventListener('popstate', () => {
            if (window.history.length <= 1) {
                sendNavigationState(0, 0, false);
            }
        });

        window.ipc.postMessage('result_ready');
    """.trimIndent()
}
