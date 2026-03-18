package dev.screengoated.toolbox.mobile.service.preset

internal fun presetResultSelectionJavascriptHelpers(): String {
    return listOf(
        presetResultSelectionOverlayJavascript(),
        presetResultSelectionRangeJavascript(),
    ).joinToString("\n")
}

internal fun presetResultSelectionOverlayJavascript(): String {
    return """
        const CUSTOM_SELECTION_DELAY_MS = 380;
        let selectionState = null;
        let selectionGestureActive = false;
        let selectionHandleDrag = null;
        let selectionHandleFixedPoint = null;
        let selectionHandleFrame = null;
        let pendingHandlePoint = null;
        let selectionHandleLayoutCache = { start: null, end: null };

        function selectionOverlayRoot() {
            let root = document.documentElement.querySelector('.sgt-selection-overlay-root');
            if (root) return root;
            root = document.createElement('div');
            root.className = 'sgt-selection-overlay-root';
            root.style.position = 'fixed';
            root.style.left = '0';
            root.style.top = '0';
            root.style.width = '100%';
            root.style.height = '100%';
            root.style.pointerEvents = 'none';
            root.style.zIndex = '2147483646';
            document.documentElement.appendChild(root);
            return root;
        }

        function currentSelectedText() {
            if (!window.getSelection) return '';
            return String(window.getSelection() || '').trim();
        }

        function describePoint(point) {
            if (!point || !point.node) return null;
            const node = point.node;
            const parent = node.parentElement || node.parentNode;
            const text = node.nodeType === Node.TEXT_NODE ? (node.textContent || '') : '';
            return {
                nodeType: node.nodeType,
                offset: point.offset,
                textSample: text ? text.slice(0, 40) : '',
                parentTag: parent && parent.tagName ? parent.tagName : null,
            };
        }

        function selectionActionElement() {
            let action = document.querySelector('.sgt-selection-action');
            if (action) return action;
            action = document.createElement('button');
            action.type = 'button';
            action.className = 'sgt-selection-action';
            action.textContent = 'Copy';
            action.addEventListener('click', event => {
                event.preventDefault();
                event.stopPropagation();
                copyCustomSelection();
            });
            selectionOverlayRoot().appendChild(action);
            return action;
        }

        function selectionHandleElement(kind) {
            let handle = document.querySelector('.sgt-selection-handle[data-kind="' + kind + '"]');
            if (handle) return handle;
            handle = document.createElement('div');
            handle.className = 'sgt-selection-handle';
            handle.dataset.kind = kind;
            selectionOverlayRoot().appendChild(handle);
            return handle;
        }

        function hideSelectionAction() {
            const action = document.querySelector('.sgt-selection-action');
            if (action) action.classList.remove('visible');
        }

        function hideSelectionHandles() {
            document.querySelectorAll('.sgt-selection-handle').forEach(handle => {
                handle.classList.remove('visible');
                handle.style.left = '-9999px';
                handle.style.top = '-9999px';
            });
        }

        function isUsableCaretRect(rect) {
            if (!rect) return false;
            const left = Number(rect.left);
            const top = Number(rect.top);
            const right = Number(rect.right);
            const bottom = Number(rect.bottom);
            if (![left, top, right, bottom].every(Number.isFinite)) return false;
            const width = Math.abs(right - left);
            const height = Math.abs(bottom - top);
            return width > 0.5 || height > 0.5;
        }

        function rectSnapshot(rect) {
            if (!isUsableCaretRect(rect)) return null;
            return {
                left: Number(rect.left),
                top: Number(rect.top),
                right: Number(rect.right),
                bottom: Number(rect.bottom),
                width: Number(rect.width || (rect.right - rect.left)),
                height: Number(rect.height || (rect.bottom - rect.top))
            };
        }

        function resolveHandleRect(kind, point, range, edgeKind) {
            const pointRect = rectSnapshot(pointCaretRect(point));
            if (pointRect) {
                selectionHandleLayoutCache[kind] = pointRect;
                return pointRect;
            }
            const edgeRect = rectSnapshot(edgeCaretRect(range, edgeKind));
            if (edgeRect) {
                selectionHandleLayoutCache[kind] = edgeRect;
                return edgeRect;
            }
            if (selectionHandleLayoutCache[kind]) {
                return selectionHandleLayoutCache[kind];
            }
            const rangeRect = rectSnapshot(range ? range.getBoundingClientRect() : null);
            if (rangeRect) {
                selectionHandleLayoutCache[kind] = rangeRect;
                return rangeRect;
            }
            return null;
        }

        function edgeCaretRect(range, kind) {
            if (!range) return null;
            const caret = range.cloneRange();
            caret.collapse(kind === 'start');
            const rects = Array.from(caret.getClientRects());
            return rects[0] || caret.getBoundingClientRect();
        }

        function pointCaretRect(point) {
            if (!point) return null;
            try {
                const caret = document.createRange();
                caret.setStart(point.node, point.offset);
                caret.collapse(true);
                const rects = Array.from(caret.getClientRects());
                return rects[0] || caret.getBoundingClientRect();
            } catch (error) {
                debugGesture('selection_point_rect_error', { error: String(error) });
                return null;
            }
        }

        function updateSelectionHandles() {
            const text = currentSelectedText();
            if (!text || !window.getSelection) {
                hideSelectionHandles();
                return;
            }
            const selection = window.getSelection();
            if (!selection || selection.rangeCount === 0) {
                hideSelectionHandles();
                return;
            }
            const range = selection.getRangeAt(0);
            const semanticStart = selectionState && selectionState.anchor ? selectionState.anchor : null;
            const semanticEnd = selectionState && selectionState.focus ? selectionState.focus : null;
            const startRect = resolveHandleRect('start', semanticStart, range, 'start');
            const endRect = resolveHandleRect('end', semanticEnd, range, 'end');
            if (!startRect || !endRect) {
                debugGesture('selection_handles_missing_rect', { selectedText: text });
                hideSelectionHandles();
                return;
            }
            const startHandle = selectionHandleElement('start');
            const endHandle = selectionHandleElement('end');
            debugGesture('selection_handles_layout', {
                scrollX: Math.round(window.scrollX || window.pageXOffset || 0),
                scrollY: Math.round(window.scrollY || window.pageYOffset || 0),
                startLeft: Math.round(startRect.left),
                startTop: Math.round(startRect.top),
                startBottom: Math.round(startRect.bottom),
                endLeft: Math.round(endRect.left),
                endTop: Math.round(endRect.top),
                endBottom: Math.round(endRect.bottom),
                innerWidth: Math.round(window.innerWidth || 0),
                innerHeight: Math.round(window.innerHeight || 0),
                selectedText: text
            });
            startHandle.style.left = Math.round(startRect.left) + 'px';
            startHandle.style.top = Math.round(startRect.bottom + 2) + 'px';
            endHandle.style.left = Math.round(endRect.right) + 'px';
            endHandle.style.top = Math.round(endRect.bottom + 2) + 'px';
            startHandle.classList.add('visible');
            endHandle.classList.add('visible');
        }

        function updateSelectionAction() {
            const text = currentSelectedText();
            if (!text) {
                hideSelectionAction();
                hideSelectionHandles();
                return;
            }
            const action = selectionActionElement();
            let top = 10;
            let left = 10;
            if (window.getSelection) {
                const selection = window.getSelection();
                if (selection && selection.rangeCount > 0) {
                    const rect = selection.getRangeAt(0).getBoundingClientRect();
                    if (rect && Number.isFinite(rect.left) && Number.isFinite(rect.top)) {
                        const actionWidth = action.offsetWidth || 68;
                        const actionHeight = action.offsetHeight || 34;
                        left = Math.min(Math.max(8, rect.left + (rect.width / 2) - (actionWidth / 2)), Math.max(8, window.innerWidth - actionWidth - 8));
                        const preferredTop = rect.top - actionHeight - 10;
                        top = preferredTop >= 8
                            ? preferredTop
                            : Math.min(Math.max(8, rect.bottom + 10), Math.max(8, window.innerHeight - actionHeight - 8));
                    }
                }
            }
            action.style.left = Math.round(left) + 'px';
            action.style.top = Math.round(top) + 'px';
            action.classList.add('visible');
            updateSelectionHandles();
        }

        function clearCustomSelection(keepTextSelection) {
            debugGesture('selection_clear', {
                keepTextSelection: !!keepTextSelection,
                hadSelectionState: !!selectionState,
                selectedText: currentSelectedText()
            });
            selectionState = null;
            selectionGestureActive = false;
            selectionHandleDrag = null;
            selectionHandleFixedPoint = null;
            pendingHandlePoint = null;
            if (selectionHandleFrame) {
                cancelAnimationFrame(selectionHandleFrame);
                selectionHandleFrame = null;
            }
            hideSelectionAction();
            hideSelectionHandles();
            if (!keepTextSelection && window.getSelection) {
                const selection = window.getSelection();
                if (selection) selection.removeAllRanges();
            }
        }
    """.trimIndent()
}
