package dev.screengoated.toolbox.mobile.service.preset

internal fun presetButtonCanvasBaseHtmlTemplate(): String {
    return """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
            <style>{{FONT_CSS}}</style>
            <style id="theme-css">{{THEME_CSS}}</style>
            <style>{{BASE_CSS}}</style>
        </head>
        <body>
            <div id="button-container"></div>
            <script>
                window.ipc = {
                    postMessage(message) {
                        if (window.sgtAndroid && window.sgtAndroid.postMessage) {
                            window.sgtAndroid.postMessage(String(message));
                        }
                    }
                };
                {{CANVAS_JS}}
                {{MOBILE_CANVAS_JS}}
            </script>
        </body>
        </html>
    """.trimIndent()
}

internal fun mobileCanvasJavascript(): String {
    return """
        let activeWindowId = null;
        let revealDeadline = 0;
        let hideTimer = null;
        let renderedWindowId = null;

        function scheduleCanvasHide() {
            if (hideTimer) clearTimeout(hideTimer);
            const remaining = Math.max(0, revealDeadline - Date.now());
            hideTimer = setTimeout(() => {
                activeWindowId = null;
                updateButtonVisibility();
            }, remaining || 1);
        }

        function applyDisabledActions() {
            document.querySelectorAll('.button-group').forEach(group => {
                const hwnd = group.dataset.hwnd;
                const state = (window.registeredWindows && window.registeredWindows[hwnd] && window.registeredWindows[hwnd].state) || {};
                const disabled = new Set(state.disabledActions || []);
                group.querySelectorAll('[data-action]').forEach(btn => {
                    const action = btn.dataset.action;
                    const blocked = disabled.has(action);
                    btn.classList.toggle('disabled', blocked);
                    if (blocked) {
                        btn.onclick = () => window.ipc.postMessage(JSON.stringify({
                            action: 'placeholder_action',
                            hwnd: hwnd,
                            placeholder: action
                        }));
                    }
                });
            });
        }

        function applyMobileCanvasAdaptations() {
            document.querySelectorAll('.button-group').forEach(group => {
                const markdownBtn = group.querySelector('[data-action="markdown"]');
                if (markdownBtn) markdownBtn.remove();
                const broomBtn = group.querySelector('.btn.broom');
                if (broomBtn) broomBtn.remove();
            });
        }

        function renderSingleWindow(data) {
            const container = document.getElementById('button-container');
            const windowData = data && data.window;
            if (!windowData) {
                container.innerHTML = '';
                renderedWindowId = null;
                window.ipc.postMessage(JSON.stringify({
                    action: 'update_clickable_regions',
                    regions: []
                }));
                return;
            }

            let group = container.querySelector('.button-group');
            if (!group) {
                group = document.createElement('div');
                group.className = 'button-group';
                container.appendChild(group);
            }

            const hwnd = String(windowData.id || '');
            const isVertical = !!windowData.vertical;
            const state = windowData.state || {};
            const nextStateKey = JSON.stringify(state) + ':' + (isVertical ? 'v' : 'h');
            if (group.dataset.lastState !== nextStateKey || renderedWindowId !== hwnd) {
                group.innerHTML = generateButtonsHTML(hwnd, state, isVertical);
                group.dataset.lastState = nextStateKey;
            }
            group.dataset.hwnd = hwnd;
            group.classList.toggle('vertical', isVertical);
            group.style.left = '0px';
            group.style.top = '0px';
            group.style.right = 'auto';
            group.style.bottom = 'auto';
            renderedWindowId = hwnd;
            applyMobileCanvasAdaptations();
            applyDisabledActions();
        }

        function updateButtonVisibility() {
            const group = document.querySelector('.button-group');
            if (!group) {
                window.ipc.postMessage(JSON.stringify({
                    action: 'update_clickable_regions',
                    regions: []
                }));
                return;
            }
            const visible = activeWindowId && Date.now() < revealDeadline && group.dataset.hwnd === activeWindowId;
            group.style.opacity = visible ? '1' : '0';
            group.style.pointerEvents = visible ? 'auto' : 'none';
            lastVisibleState.set(group.dataset.hwnd, visible);

            const regions = [];
            if (visible) {
                const rect = group.getBoundingClientRect();
                regions.push({
                    x: Math.round(rect.left),
                    y: Math.round(rect.top),
                    w: Math.round(rect.width),
                    h: Math.round(rect.height)
                });
            }
            window.ipc.postMessage(JSON.stringify({
                action: 'update_clickable_regions',
                regions: regions
            }));
        }

        window.setCanvasWindows = function(raw) {
            const payload = typeof raw === 'string' ? JSON.parse(raw) : raw;
            renderSingleWindow(payload || {});
            if (payload && payload.activeWindowId) {
                window.revealWindow(payload.activeWindowId, payload.lingerMs || 2000);
            } else {
                activeWindowId = null;
                updateButtonVisibility();
            }
        };

        window.revealWindow = function(windowId, lingerMs) {
            activeWindowId = String(windowId);
            revealDeadline = Date.now() + (lingerMs || 2000);
            updateButtonVisibility();
            scheduleCanvasHide();
        };

        document.addEventListener('touchstart', event => {
            const target = event.target;
            if (!(target instanceof Element)) return;
            const group = target.closest('.button-group');
            if (!group) return;
            window.revealWindow(group.dataset.hwnd, 2000);
        }, { passive: true });
    """.trimIndent()
}
