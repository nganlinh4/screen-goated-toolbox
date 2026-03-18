package dev.screengoated.toolbox.mobile.service.preset

internal fun presetResultSelectionRangeJavascript(): String {
    return """
        function comparePoints(left, right) {
            if (!left || !right || !left.node || !right.node) return 0;
            if (left.node === right.node) return left.offset - right.offset;
            const position = left.node.compareDocumentPosition(right.node);
            if (position & Node.DOCUMENT_POSITION_FOLLOWING) return -1;
            if (position & Node.DOCUMENT_POSITION_PRECEDING) return 1;
            return 0;
        }

        function normalizeSelectionEndpoints(anchor, focus) {
            if (!anchor || !focus) return null;
            if (comparePoints(anchor, focus) <= 0) {
                return { anchor: anchor, focus: focus };
            }
            return { anchor: focus, focus: anchor };
        }

        function normalizeCaretPoint(point) {
            if (!point || !point.node) return null;
            if (point.node.nodeType === Node.TEXT_NODE) return point;
            if (point.node.nodeType === Node.ELEMENT_NODE) {
                return elementPointToNearestTextPoint(point.node, point.offset);
            }
            return null;
        }

        function firstTextPoint(node) {
            if (!node) return null;
            if (node.nodeType === Node.TEXT_NODE) return { node: node, offset: 0 };
            let child = node.firstChild;
            while (child) {
                const point = firstTextPoint(child);
                if (point) return point;
                child = child.nextSibling;
            }
            return null;
        }

        function lastTextPoint(node) {
            if (!node) return null;
            if (node.nodeType === Node.TEXT_NODE) return { node: node, offset: (node.textContent || '').length };
            let child = node.lastChild;
            while (child) {
                const point = lastTextPoint(child);
                if (point) return point;
                child = child.previousSibling;
            }
            return null;
        }

        function elementPointToNearestTextPoint(element, offset) {
            if (!element) return null;
            const children = element.childNodes || [];
            const index = Math.max(0, Math.min(children.length, Number(offset) || 0));
            for (let i = index; i < children.length; i += 1) {
                const point = firstTextPoint(children[i]);
                if (point) return point;
            }
            for (let i = index - 1; i >= 0; i -= 1) {
                const point = lastTextPoint(children[i]);
                if (point) return point;
            }
            return null;
        }

        function currentRangePoints() {
            if (!window.getSelection) return null;
            const selection = window.getSelection();
            if (!selection || selection.rangeCount === 0) return null;
            const range = selection.getRangeAt(0);
            return {
                anchor: { node: range.startContainer, offset: range.startOffset },
                focus: { node: range.endContainer, offset: range.endOffset },
            };
        }

        function applySelectionRange(anchorPoint, focusPoint) {
            if (!window.getSelection || !anchorPoint || !focusPoint) return false;
            const anchor = normalizeCaretPoint(anchorPoint);
            const focus = normalizeCaretPoint(focusPoint);
            if (!anchor || !focus) {
                debugGesture('selection_apply_invalid', {
                    anchorPoint: describePoint(anchorPoint),
                    focusPoint: describePoint(focusPoint),
                    anchorNormalized: describePoint(anchor),
                    focusNormalized: describePoint(focus)
                });
                return false;
            }
            try {
                const selection = window.getSelection();
                if (selection.setBaseAndExtent) {
                    selection.removeAllRanges();
                    selection.setBaseAndExtent(anchor.node, anchor.offset, focus.node, focus.offset);
                } else {
                    const normalized = normalizeSelectionEndpoints(anchor, focus);
                    if (!normalized) return false;
                    const range = document.createRange();
                    range.setStart(normalized.anchor.node, normalized.anchor.offset);
                    range.setEnd(normalized.focus.node, normalized.focus.offset);
                    selection.removeAllRanges();
                    selection.addRange(range);
                }
                selectionState = { anchor: anchor, focus: focus };
                debugGesture('selection_apply_success', {
                    anchorPoint: describePoint(anchor),
                    focusPoint: describePoint(focus),
                    selectedText: currentSelectedText(),
                    rangeStart: currentRangePoints() ? describePoint(currentRangePoints().anchor) : null,
                    rangeEnd: currentRangePoints() ? describePoint(currentRangePoints().focus) : null,
                });
                updateSelectionAction();
                return true;
            } catch (error) {
                debugGesture('selection_apply_error', {
                    error: String(error),
                    anchorPoint: describePoint(anchor),
                    focusPoint: describePoint(focus)
                });
                return false;
            }
        }

        function caretPointFromClient(clientX, clientY) {
            let point = null;
            if (document.caretPositionFromPoint) {
                const pos = document.caretPositionFromPoint(clientX, clientY);
                if (pos) point = { node: pos.offsetNode, offset: pos.offset };
            }
            if (!point && document.caretRangeFromPoint) {
                const range = document.caretRangeFromPoint(clientX, clientY);
                if (range) point = { node: range.startContainer, offset: range.startOffset };
            }
            const normalized = normalizeCaretPoint(point);
            debugGesture('selection_caret_lookup', {
                clientX: Math.round(clientX),
                clientY: Math.round(clientY),
                candidatePoint: describePoint(point),
                normalizedPoint: describePoint(normalized)
            });
            return normalized;
        }

        function beginCustomSelection(clientX, clientY) {
            const anchorPoint = caretPointFromClient(clientX, clientY);
            if (!anchorPoint) return;
            debugGesture('selection_mode_begin', { anchorPoint: describePoint(anchorPoint) });
            selectionGestureActive = true;
            selectionHandleDrag = null;
            selectionHandleFixedPoint = null;
            if (selectionHandleFrame) {
                cancelAnimationFrame(selectionHandleFrame);
                selectionHandleFrame = null;
            }
            pendingHandlePoint = null;
            selectionHandleLayoutCache = { start: null, end: null };
            applySelectionRange(anchorPoint, anchorPoint);
        }

        function updateCustomSelection(clientX, clientY) {
            if (!selectionState) return false;
            const nextFocus = caretPointFromClient(clientX, clientY);
            if (!nextFocus) return false;
            return applySelectionRange(selectionState.anchor, nextFocus);
        }

        function scheduleHandleUpdate(clientX, clientY) {
            pendingHandlePoint = { clientX, clientY };
            if (selectionHandleFrame) return;
            selectionHandleFrame = requestAnimationFrame(() => {
                selectionHandleFrame = null;
                const pending = pendingHandlePoint;
                pendingHandlePoint = null;
                if (!pending || !selectionHandleDrag || !selectionHandleFixedPoint) return;
                const candidate = caretPointFromClient(pending.clientX, pending.clientY);
                if (!candidate) return;
                const nextAnchor = selectionHandleDrag === 'start' ? candidate : selectionHandleFixedPoint;
                const nextFocus = selectionHandleDrag === 'start' ? selectionHandleFixedPoint : candidate;
                debugGesture('selection_handle_apply', {
                    handle: selectionHandleDrag,
                    candidatePoint: describePoint(candidate),
                    fixedPoint: describePoint(selectionHandleFixedPoint),
                    nextAnchor: describePoint(nextAnchor),
                    nextFocus: describePoint(nextFocus)
                });
                applySelectionRange(nextAnchor, nextFocus);
            });
        }

        function scheduleCustomSelection(clientX, clientY) {
            clearHoldTimer();
            holdTimer = setTimeout(() => {
                holdTimer = null;
                if (!pendingStart || dragState || resizeState || selectionHandleDrag) {
                    debugGesture('selection_schedule_aborted', {
                        hasPendingStart: !!pendingStart,
                        hasDrag: !!dragState,
                        hasResize: !!resizeState
                    });
                    return;
                }
                debugGesture('selection_schedule_fire', {
                    clientX: Math.round(clientX),
                    clientY: Math.round(clientY)
                });
                beginCustomSelection(clientX, clientY);
            }, CUSTOM_SELECTION_DELAY_MS);
        }

        function copyCustomSelection() {
            const text = currentSelectedText();
            if (!text || !activeWindowId) {
                debugGesture('selection_copy_skipped', {
                    hasText: !!text,
                    hasWindowId: !!activeWindowId
                });
                return;
            }
            debugGesture('selection_copy', { selectedText: text });
            postJson({
                type: 'copySelectedText',
                windowId: activeWindowId,
                text: text
            });
            hideSelectionAction();
        }

        document.addEventListener('scroll', () => {
            if (selectionState) updateSelectionAction();
        }, true);
    """.trimIndent()
}
