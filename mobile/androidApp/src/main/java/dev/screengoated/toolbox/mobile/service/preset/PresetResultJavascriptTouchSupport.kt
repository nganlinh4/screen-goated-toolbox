package dev.screengoated.toolbox.mobile.service.preset

internal fun presetResultJavascriptTouchSupport(): String {
    return """
        function axisAllowsScroll(style, axis) {
            const axisValue = axis === 'x' ? (style.overflowX || style.overflow) : (style.overflowY || style.overflow);
            return axisValue !== 'hidden' && axisValue !== 'clip';
        }

        function elementCanScrollAxis(element, axis) {
            if (!element) return false;
            const style = window.getComputedStyle(element);
            if (!axisAllowsScroll(style, axis)) return false;
            return axis === 'x'
                ? element.scrollWidth > element.clientWidth + 1
                : element.scrollHeight > element.clientHeight + 1;
        }

        function collectPointElements(clientX, clientY) {
            if (typeof document.elementsFromPoint === 'function') {
                return document.elementsFromPoint(clientX, clientY) || [];
            }
            const single = document.elementFromPoint(clientX, clientY);
            return single ? [single] : [];
        }

        function scrollabilityScore(element) {
            if (!element) return 0;
            let score = 0;
            if (elementCanScrollAxis(element, 'x')) {
                score += Math.max(1, (element.scrollWidth || 0) - (element.clientWidth || 0));
            }
            if (elementCanScrollAxis(element, 'y')) {
                score += Math.max(1, (element.scrollHeight || 0) - (element.clientHeight || 0));
            }
            return score;
        }

        function collectScrollableCandidatesFromNode(node, output, seen) {
            let element = normalizeTarget(node);
            while (element && element !== document.body && element !== document.documentElement) {
                if (!seen.has(element) && scrollabilityScore(element) > 0) {
                    seen.add(element);
                    output.push(element);
                }
                element = element.parentElement;
            }
        }

        function collectScrollableCandidates(clientX, clientY, target) {
            const output = [];
            const seen = new Set();
            collectScrollableCandidatesFromNode(target, output, seen);
            collectPointElements(clientX, clientY).forEach(element => {
                collectScrollableCandidatesFromNode(element, output, seen);
            });
            const scroller = document.scrollingElement || document.documentElement;
            if (scroller && !seen.has(scroller)) output.push(scroller);
            output.sort((left, right) => {
                const leftIsRoot = left === scroller;
                const rightIsRoot = right === scroller;
                if (leftIsRoot !== rightIsRoot) return leftIsRoot ? 1 : -1;
                return scrollabilityScore(right) - scrollabilityScore(left);
            });
            return output;
        }

        function findScrollableContainer(target, clientX, clientY) {
            const candidates = collectScrollableCandidates(clientX, clientY, target);
            return candidates.length > 0 ? candidates[0] : (document.scrollingElement || document.documentElement);
        }

        function tryScrollContainer(container, dx, dy) {
            const target = container && typeof container.scrollBy === 'function' ? container : window;
            const beforeX = container && target !== window ? container.scrollLeft : (window.scrollX || window.pageXOffset || 0);
            const beforeY = container && target !== window ? container.scrollTop : (window.scrollY || window.pageYOffset || 0);
            target.scrollBy(-dx, -dy);
            const afterX = container && target !== window ? container.scrollLeft : (window.scrollX || window.pageXOffset || 0);
            const afterY = container && target !== window ? container.scrollTop : (window.scrollY || window.pageYOffset || 0);
            return Math.abs(afterX - beforeX) > 0.5 || Math.abs(afterY - beforeY) > 0.5;
        }

        function scrollWithBestContainer(clientX, clientY, fallbackTarget, dx, dy, preferredContainer) {
            const candidates = [];
            const seen = new Set();
            if (preferredContainer) {
                candidates.push(preferredContainer);
                seen.add(preferredContainer);
            }
            collectScrollableCandidates(clientX, clientY, fallbackTarget).forEach(candidate => {
                if (!seen.has(candidate)) {
                    candidates.push(candidate);
                    seen.add(candidate);
                }
            });
            for (const candidate of candidates) {
                if (tryScrollContainer(candidate, dx, dy)) return candidate;
            }
            tryScrollContainer(null, dx, dy);
            return preferredContainer || candidates[0] || null;
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
            const midX = (first.clientX + second.clientX) / 2;
            const midY = (first.clientY + second.clientY) / 2;
            twoFingerScrollState = {
                x: midX,
                y: midY,
                container: findScrollableContainer(event.target, midX, midY),
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
            const container = scrollWithBestContainer(
                nextX,
                nextY,
                event.target,
                dx,
                dy,
                twoFingerScrollState.container || findScrollableContainer(event.target, nextX, nextY)
            );
            twoFingerScrollState.x = nextX;
            twoFingerScrollState.y = nextY;
            twoFingerScrollState.container = container;
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
            if (Math.hypot(vx, vy) < INERTIA_MIN_VELOCITY) return;
            let lastTs = performance.now();
            function tick(now) {
                const dt = Math.max(1, now - lastTs);
                lastTs = now;
                const moveX = vx * dt;
                const moveY = vy * dt;
                tryScrollContainer(container, moveX, moveY);
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

        document.addEventListener('click', event => {
            if (!document.body.contains(event.target)) return;
            activateWindow();
            const link = event.target && event.target.closest && event.target.closest('a[href]');
            if (link) {
                sendNavigationState(1, 1, true);
            }
        }, true);

        document.addEventListener('dragstart', event => {
            event.preventDefault();
        }, true);

        document.addEventListener('touchstart', event => {
            if (event.touches.length > 1) {
                dragState = null;
                resizeState = null;
                pendingStart = null;
                selectionGestureActive = false;
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
            const handle = target && target.closest && target.closest('.sgt-selection-handle');
            const interactive = target && target.closest && target.closest('a, button, input, textarea, select');
            const resizeCorner = detectResizeCorner(point.clientX, point.clientY);
            if (handle) {
                selectionHandleDrag = handle.dataset.kind || null;
                selectionGestureActive = true;
                selectionHandleFixedPoint = selectionHandleDrag === 'start'
                    ? (selectionState && selectionState.focus ? selectionState.focus : null)
                    : (selectionState && selectionState.anchor ? selectionState.anchor : null);
                pendingStart = null;
                clearHoldTimer();
                activateWindow();
                debugGesture('touchstart_selection_handle', {
                    handle: selectionHandleDrag,
                    selectedText: currentSelectedText()
                });
                if (event.cancelable) event.preventDefault();
                return;
            }
            if (resizeCorner) {
                clearCustomSelection(true);
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
                if (!(target && target.closest && target.closest('.sgt-selection-action'))) {
                    clearCustomSelection(false);
                }
                pendingStart = null;
                clearHoldTimer();
                debugGesture('touchstart_interactive', {
                    target: target ? target.tagName || target.nodeName : null
                });
                return;
            }
            if (selectionState && !(target && target.closest && target.closest('.sgt-selection-action'))) {
                clearCustomSelection(false);
            }
            const selectionTarget = isSelectionTarget(target);
            pendingStart = {
                x: point.screenX,
                clientX: point.clientX,
                clientY: point.clientY,
                y: point.screenY,
                selectionTarget: selectionTarget,
                startedAt: Date.now(),
            };
            clearHoldTimer();
            if (selectionTarget) scheduleCustomSelection(point.clientX, point.clientY);
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
            const target = normalizeTarget(event.target);
            if (pendingStart && !selectionHandleDrag && !selectionGestureActive && event.cancelable) {
                event.preventDefault();
            }
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
            if (selectionHandleDrag) {
                scheduleHandleUpdate(point.clientX, point.clientY);
                if (event.cancelable) event.preventDefault();
                return;
            }
            if (selectionState && selectionGestureActive && !(target && target.closest && target.closest('.sgt-selection-action'))) {
                if (updateCustomSelection(point.clientX, point.clientY) && event.cancelable) {
                    event.preventDefault();
                }
                return;
            }
            if (!pendingStart && !dragState) return;
            const movedEnough = pendingStart &&
                (Math.abs(point.screenX - pendingStart.x) > DRAG_THRESHOLD_PX || Math.abs(point.screenY - pendingStart.y) > DRAG_THRESHOLD_PX);
            if (!dragState && movedEnough) {
                clearHoldTimer();
                clearCustomSelection(true);
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
            } else if (selectionState) {
                selectionGestureActive = false;
                selectionHandleDrag = null;
                selectionHandleFixedPoint = null;
                updateSelectionAction();
                debugGesture('touchend_selection', { selectionText: currentSelectedText() });
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
            if (inertiaState) startInertiaScroll(inertiaState);
        }, { passive: true });

        document.addEventListener('touchcancel', () => {
            debugGesture('touchcancel', {});
            postJson({ type: 'cancelResultGesture', windowId: activeWindowId });
            dragState = null;
            resizeState = null;
            pendingStart = null;
            twoFingerScrollState = null;
            clearHoldTimer();
            if (selectionState) {
                selectionGestureActive = false;
                selectionHandleDrag = null;
                selectionHandleFixedPoint = null;
                updateSelectionAction();
            }
        }, { passive: true });

        document.addEventListener('selectionchange', () => {
            if (selectionHandleDrag) return;
            const selectionText = window.getSelection ? String(window.getSelection()) : '';
            if (selectionText && selectionText.trim().length > 0) {
                debugGesture('selectionchange', { selectionText: selectionText });
                updateSelectionAction();
            } else if (selectionState) {
                hideSelectionAction();
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
