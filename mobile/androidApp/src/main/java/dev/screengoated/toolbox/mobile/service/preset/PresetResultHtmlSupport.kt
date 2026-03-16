package dev.screengoated.toolbox.mobile.service.preset

internal fun presetResultBaseHtmlTemplate(): String {
    return """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
            <style>{{FONT_CSS}}</style>
            <style>{{RESULT_CSS}}</style>
            <style>{{MARKDOWN_CSS}}</style>
        </head>
        <body>
            <div id="markdown-shell"></div>
            <script>{{FIT_SCRIPT}}</script>
            <script>
                window.ipc = {
                    postMessage(message) {
                        if (window.sgtAndroid && window.sgtAndroid.postMessage) {
                            window.sgtAndroid.postMessage(String(message));
                        }
                    }
                };
                {{RESULT_JS}}
            </script>
        </body>
        </html>
    """.trimIndent()
}

internal fun presetResultCss(isDark: Boolean): String {
    val shellBg = if (isDark) "rgba(22, 24, 32, 0.88)" else "rgba(252, 252, 255, 0.88)"
    val shellBorder = if (isDark) "rgba(255, 255, 255, 0.10)" else "rgba(10, 18, 28, 0.10)"
    return """
        * { box-sizing: border-box; }
        html, body {
            margin: 0;
            width: 100%;
            height: 100%;
            background: transparent;
            overflow: hidden;
            touch-action: manipulation;
        }
        body {
            padding: 6px;
            color: ${if (isDark) "#f4f7fb" else "#18212b"};
        }
        #markdown-shell {
            width: 100%;
            height: 100%;
            overflow: hidden;
            border-radius: 18px;
            border: 1px solid $shellBorder;
            background: $shellBg;
            backdrop-filter: blur(18px);
            -webkit-backdrop-filter: blur(18px);
            box-shadow: 0 22px 54px rgba(0, 0, 0, 0.28);
            padding: 8px 10px;
        }
        #markdown-shell > *:first-child { margin-top: 0; }
        a { cursor: pointer; }
    """.trimIndent()
}

internal fun presetResultJavascript(): String {
    return """
        const shell = document.getElementById('markdown-shell');
        let activeWindowId = null;
        let dragState = null;
        let resizeState = null;
        let holdTimer = null;
        let pendingStart = null;
        let dragging = false;
        const HOLD_DELAY_MS = 180;
        const DRAG_THRESHOLD_PX = 6;
        const RESIZE_ZONE_PX = 44;
        const DRAG_GAIN = 2.1;
        const RESIZE_GAIN = 1.85;

        function postJson(payload) {
            window.ipc.postMessage(JSON.stringify(payload));
        }

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

        function detectResizeCorner(clientX, clientY) {
            const rect = shell.getBoundingClientRect();
            const localX = clientX - rect.left;
            const localY = clientY - rect.top;
            if (localY < rect.height - RESIZE_ZONE_PX) {
                return null;
            }
            if (localX < RESIZE_ZONE_PX) return 'bl';
            if (localX > rect.width - RESIZE_ZONE_PX) return 'br';
            return null;
        }

        function clearHoldTimer() {
            if (holdTimer) {
                clearTimeout(holdTimer);
                holdTimer = null;
            }
        }

        function beginDrag(point) {
            if (!pendingStart) return;
            dragState = { x: point.screenX, y: point.screenY };
            dragging = true;
            activateWindow();
        }

        function runFit(streaming) {
            if (window.runWindowsMarkdownFit) {
                window.runWindowsMarkdownFit(!!streaming, streaming ? 'mobile_streaming_fit' : 'mobile_final_fit');
            }
        }

        window.applyResultState = function(raw) {
            const data = typeof raw === 'string' ? JSON.parse(raw) : raw;
            activeWindowId = data.windowId;
            shell.innerHTML = data.html || '';
            runFit(!!data.streaming);
        };

        window.navigateHistory = function(direction) {
            if (!activeWindowId) return;
            postJson({ type: direction === 'back' ? 'navigateBack' : 'navigateForward', windowId: activeWindowId });
        };

        document.addEventListener('click', event => {
            if (!shell.contains(event.target)) return;
            activateWindow();
            const link = event.target && event.target.closest && event.target.closest('a[href]');
            if (link) {
                sendNavigationState(1, 1, true);
            }
        }, true);

        document.addEventListener('touchstart', event => {
            if (event.touches.length !== 1) return;
            const point = touchPoint(event);
            activateWindow();
            const interactive = event.target && event.target.closest && event.target.closest('a, button, input, textarea, select');
            const resizeCorner = detectResizeCorner(point.clientX, point.clientY);
            if (resizeCorner) {
                resizeState = { corner: resizeCorner, x: point.screenX, y: point.screenY };
                pendingStart = null;
                clearHoldTimer();
                return;
            }
            if (interactive) {
                pendingStart = null;
                clearHoldTimer();
                return;
            }
            pendingStart = { x: point.screenX, y: point.screenY };
            clearHoldTimer();
            holdTimer = setTimeout(() => beginDrag(point), HOLD_DELAY_MS);
        }, { passive: true });

        document.addEventListener('touchmove', event => {
            const point = touchPoint(event);
            if (resizeState) {
                const dx = Math.round((point.screenX - resizeState.x) * RESIZE_GAIN);
                const dy = Math.round((point.screenY - resizeState.y) * RESIZE_GAIN);
                if (dx !== 0 || dy !== 0) {
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
                beginDrag(point);
            }
            if (!dragState) return;
            const dx = Math.round((point.screenX - dragState.x) * DRAG_GAIN);
            const dy = Math.round((point.screenY - dragState.y) * DRAG_GAIN);
            if (dx !== 0 || dy !== 0) {
                postJson({ type: 'dragResultWindow', windowId: activeWindowId, dx, dy });
                postJson({ type: 'dragResultWindowAt', windowId: activeWindowId, x: Math.round(point.screenX), y: Math.round(point.screenY) });
                dragState.x = point.screenX;
                dragState.y = point.screenY;
                if (event.cancelable) event.preventDefault();
            }
        }, { passive: false });

        document.addEventListener('touchend', event => {
            const point = touchPoint(event);
            if (dragState) {
                postJson({ type: 'dragResultWindowEnd', windowId: activeWindowId, x: Math.round(point.screenX), y: Math.round(point.screenY) });
            } else if (resizeState) {
                postJson({ type: 'resizeResultWindowEnd', windowId: activeWindowId });
            }
            dragging = false;
            dragState = null;
            resizeState = null;
            pendingStart = null;
            clearHoldTimer();
        }, { passive: true });

        document.addEventListener('touchcancel', () => {
            postJson({ type: 'cancelResultGesture', windowId: activeWindowId });
            dragging = false;
            dragState = null;
            resizeState = null;
            pendingStart = null;
            clearHoldTimer();
        }, { passive: true });

        window.addEventListener('popstate', () => {
            if (window.history.length <= 1) {
                sendNavigationState(0, 0, false);
            }
        });

        window.ipc.postMessage('result_ready');
    """.trimIndent()
}
