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
            <style>
                .opacity-btn-expandable.touch-expanded:not(.vertical-slider) {
                    width: 110px !important;
                    background: var(--btn-hover-bg) !important;
                    transform: none !important;
                }
                .opacity-btn-expandable.touch-expanded.vertical-slider {
                    height: 110px !important;
                    background: var(--btn-hover-bg) !important;
                    transform: none !important;
                }
                .opacity-btn-expandable.touch-expanded .opacity-slider-wrapper {
                    opacity: 1;
                    pointer-events: auto;
                }
            </style>
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
        let canvasPinned = false;
        let sliderActive = false;

        function scheduleCanvasHide() {
            if (canvasPinned || sliderActive) return;
            if (hideTimer) clearTimeout(hideTimer);
            const remaining = Math.max(0, revealDeadline - Date.now());
            hideTimer = setTimeout(() => {
                if (canvasPinned || sliderActive) return;
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
            if (opacityOpen) {
                const ob = group.querySelector('.opacity-btn-expandable');
                if (ob) ob.classList.add('touch-expanded');
            }
            canvasPinned = !!state.isEditing;
            if (canvasPinned && hideTimer) {
                clearTimeout(hideTimer);
                hideTimer = null;
            }
            requestAnimationFrame(() => {
                const dpr = window.devicePixelRatio || 1;
                const groupEl = document.querySelector('.button-group');
                if (groupEl) {
                    const rect = groupEl.getBoundingClientRect();
                    window.ipc.postMessage(JSON.stringify({
                        action: 'canvas_content_size',
                        w: Math.round(rect.width * dpr),
                        h: Math.round(rect.height * dpr)
                    }));
                }
            });
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
            const visible = canvasPinned || sliderActive || (activeWindowId && Date.now() < revealDeadline && group.dataset.hwnd === activeWindowId);
            group.style.opacity = visible ? '1' : '0';
            group.style.pointerEvents = visible ? 'auto' : 'none';
            lastVisibleState.set(group.dataset.hwnd, visible);

            const regions = [];
            if (visible) {
                const dpr = window.devicePixelRatio || 1;
                const rect = group.getBoundingClientRect();
                regions.push({
                    x: Math.round(rect.left * dpr),
                    y: Math.round(rect.top * dpr),
                    w: Math.round(rect.width * dpr),
                    h: Math.round(rect.height * dpr)
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

        let opacityOpen = false;
        let opacityCollapseTimer = null;
        let activeSliderEl = null;
        let sliderRect = null;
        let sliderHwnd = null;
        let openerTouchActive = false;
        let openerTouchMoved = false;

        function collapseOpacity() {
            opacityOpen = false;
            sliderActive = false;
            activeSliderEl = null;
            sliderRect = null;
            sliderHwnd = null;
            openerTouchActive = false;
            openerTouchMoved = false;
            if (opacityCollapseTimer) { clearTimeout(opacityCollapseTimer); opacityCollapseTimer = null; }
            document.querySelectorAll('.touch-expanded').forEach(el => el.classList.remove('touch-expanded'));
            window.revealWindow(renderedWindowId || activeWindowId, 2000);
        }

        window.collapseOpacitySlider = collapseOpacity;

        function scheduleOpacityCollapse() {
            if (opacityCollapseTimer) clearTimeout(opacityCollapseTimer);
            opacityCollapseTimer = setTimeout(collapseOpacity, 3000);
        }

        function prepSliderForDrag(opacityBtn, hwnd) {
            activeSliderEl = opacityBtn.querySelector('.opacity-slider-inline');
            sliderHwnd = hwnd;
            if (activeSliderEl) {
                sliderRect = activeSliderEl.getBoundingClientRect();
            }
        }

        function updateSliderFromTouch(clientX) {
            if (!activeSliderEl || !sliderRect) return;
            const ratio = Math.max(0, Math.min(1, (clientX - sliderRect.left) / sliderRect.width));
            const min = parseInt(activeSliderEl.min) || 10;
            const max = parseInt(activeSliderEl.max) || 100;
            const val = Math.round(min + ratio * (max - min));
            activeSliderEl.value = val;
            if (typeof updateOpacity === 'function' && sliderHwnd) {
                updateOpacity(sliderHwnd, val);
            }
        }

        document.addEventListener('touchstart', event => {
            const target = event.target;
            if (!(target instanceof Element)) return;
            const group = target.closest('.button-group');
            if (!group) return;

            const opacityBtn = target.closest('.opacity-btn-expandable');

            if (opacityBtn) {
                if (!opacityOpen) {
                    opacityOpen = true;
                    sliderActive = true;
                    openerTouchActive = true;
                    openerTouchMoved = false;
                    opacityBtn.classList.add('touch-expanded');
                    window.revealWindow(group.dataset.hwnd, 30000);
                    prepSliderForDrag(opacityBtn, group.dataset.hwnd);
                } else if (!target.closest('.opacity-slider-inline') && !target.closest('.opacity-slider-wrapper')) {
                    collapseOpacity();
                } else {
                    sliderActive = true;
                    openerTouchActive = false;
                    openerTouchMoved = false;
                    prepSliderForDrag(opacityBtn, group.dataset.hwnd);
                    const touch = event.touches[0];
                    if (touch && activeSliderEl) updateSliderFromTouch(touch.clientX);
                    if (opacityCollapseTimer) { clearTimeout(opacityCollapseTimer); opacityCollapseTimer = null; }
                    window.revealWindow(group.dataset.hwnd, 30000);
                }
                return;
            }

            collapseOpacity();
            window.revealWindow(group.dataset.hwnd, 5000);
        }, { passive: true });

        document.addEventListener('touchmove', event => {
            if (opacityOpen && activeSliderEl && event.touches.length > 0) {
                openerTouchMoved = true;
                updateSliderFromTouch(event.touches[0].clientX);
            }
        }, { passive: true });

        document.addEventListener('touchend', () => {
            if (!opacityOpen) return;
            if (openerTouchActive && !openerTouchMoved) {
                // Simple tap to open — keep open, start auto-collapse timer
                openerTouchActive = false;
                scheduleOpacityCollapse();
            } else {
                // Dragged the slider or second touch — collapse
                collapseOpacity();
            }
        }, { passive: true });

        document.addEventListener('touchcancel', () => {
            if (opacityOpen) {
                collapseOpacity();
            }
        }, { passive: true });
    """.trimIndent()
}
