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
    val selectionActionBg = if (isDark) "rgba(12, 16, 24, 0.90)" else "rgba(255, 255, 255, 0.96)"
    val selectionActionBorder = if (isDark) "rgba(255, 255, 255, 0.18)" else "rgba(28, 34, 44, 0.14)"
    val selectionActionColor = if (isDark) "rgba(255, 255, 255, 0.96)" else "rgba(18, 20, 28, 0.92)"
    val selectionActionShadow = if (isDark) "0 10px 26px rgba(0, 0, 0, 0.22)" else "0 10px 24px rgba(24, 36, 54, 0.14)"
    val handleBorder = if (isDark) "rgba(255, 255, 255, 0.90)" else "rgba(255, 255, 255, 0.96)"
    val handleFill = if (isDark) "rgba(43, 122, 255, 0.96)" else "rgba(38, 112, 245, 0.94)"
    val handleInner = if (isDark) "rgba(255, 255, 255, 0.96)" else "rgba(248, 250, 255, 0.98)"
    val handleShadow = if (isDark) "0 6px 18px rgba(0, 0, 0, 0.22)" else "0 6px 16px rgba(24, 36, 54, 0.16)"
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
        .sgt-selection-action {
            position: fixed;
            z-index: 2147483646;
            left: -9999px;
            top: -9999px;
            display: inline-flex;
            align-items: center;
            justify-content: center;
            min-width: 68px;
            height: 34px;
            padding: 0 14px;
            border: 1px solid $selectionActionBorder;
            border-radius: 17px;
            background: $selectionActionBg;
            color: $selectionActionColor;
            font: 600 14px/1 "Google Sans Flex", system-ui, sans-serif;
            letter-spacing: 0.01em;
            box-shadow: $selectionActionShadow;
            opacity: 0;
            pointer-events: none;
            transform: translateY(6px);
            transition: opacity 120ms ease, transform 120ms ease;
            user-select: none;
            -webkit-user-select: none;
        }
        .sgt-selection-action.visible {
            opacity: 1;
            pointer-events: auto;
            transform: translateY(0);
        }
        .sgt-selection-handle {
            position: fixed;
            z-index: 2147483646;
            left: -9999px;
            top: -9999px;
            width: 24px;
            height: 24px;
            margin-left: -12px;
            margin-top: -12px;
            border-radius: 50%;
            border: 2px solid $handleBorder;
            background: $handleFill;
            box-shadow: $handleShadow;
            opacity: 0;
            pointer-events: none;
            transition: opacity 80ms ease;
            touch-action: none;
            user-select: none;
            -webkit-user-select: none;
        }
        .sgt-selection-handle::before {
            content: "";
            position: absolute;
            left: 50%;
            top: -14px;
            width: 4px;
            height: 16px;
            margin-left: -2px;
            border-radius: 999px;
            background: $handleFill;
        }
        .sgt-selection-handle::after {
            content: "";
            position: absolute;
            left: 50%;
            top: 50%;
            width: 8px;
            height: 8px;
            margin-left: -4px;
            margin-top: -4px;
            border-radius: 50%;
            background: $handleInner;
        }
        .sgt-selection-handle.visible {
            opacity: 1;
            pointer-events: auto;
        }
    """.trimIndent()
}

internal fun presetHostedRawPageCss(isDark: Boolean): String {
    return presetResultCss(isDark)
        .replace("overflow-y: hidden;", "overflow-y: auto;")
        .replace("overflow-x: hidden;", "overflow-x: auto;")
        .plus(
            """
            html, body, body * {
                touch-action: none !important;
                overscroll-behavior: none !important;
            }
            """.trimIndent(),
        )
}

internal fun presetHostedRawPageBootstrapScript(
    windowId: String,
    isDark: Boolean,
): String {
    val quotedCss = jsStringLiteral(presetHostedRawPageCss(isDark))
    val quotedWindowId = jsStringLiteral(windowId)
    return """
        (function() {
            window.ipc = window.ipc || {
                postMessage(message) {
                    if (window.sgtAndroid && window.sgtAndroid.postMessage) {
                        window.sgtAndroid.postMessage(String(message));
                    }
                }
            };
            const styleId = 'sgt-result-hosted-page-style';
            let style = document.getElementById(styleId);
            if (!style) {
                style = document.createElement('style');
                style.id = styleId;
                (document.head || document.documentElement).appendChild(style);
            }
            style.textContent = $quotedCss;
            document.documentElement.setAttribute('data-sgt-result-hosted', '1');
            if (document.body) {
                document.body.setAttribute('data-sgt-result-hosted', '1');
            }
            if (!window.__SGT_RESULT_INTERACTION_INSTALLED__) {
                window.__SGT_RESULT_INTERACTION_INSTALLED__ = true;
                ${presetResultInteractionJavascript()}
            }
            if (typeof window.configureResultWindow === 'function') {
                window.configureResultWindow($quotedWindowId);
            }
        })();
    """.trimIndent()
}

private fun jsStringLiteral(value: String): String {
    return buildString(value.length + 16) {
        append('"')
        value.forEach { ch ->
            when (ch) {
                '\\' -> append("\\\\")
                '"' -> append("\\\"")
                '\n' -> append("\\n")
                '\r' -> append("\\r")
                '\t' -> append("\\t")
                '\b' -> append("\\b")
                '\u000C' -> append("\\f")
                else -> append(ch)
            }
        }
        append('"')
    }
}

private fun presetResultSelectionJavascriptHelpers(): String {
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
            if (root) {
                return root;
            }
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
            if (!point || !point.node) {
                return null;
            }
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
            if (action) {
                return action;
            }
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
            if (handle) {
                return handle;
            }
            handle = document.createElement('div');
            handle.className = 'sgt-selection-handle';
            handle.dataset.kind = kind;
            selectionOverlayRoot().appendChild(handle);
            return handle;
        }

        function hideSelectionAction() {
            const action = document.querySelector('.sgt-selection-action');
            if (action) {
                action.classList.remove('visible');
            }
        }

        function hideSelectionHandles() {
            document.querySelectorAll('.sgt-selection-handle').forEach(handle => {
                handle.classList.remove('visible');
                handle.style.left = '-9999px';
                handle.style.top = '-9999px';
            });
        }

        function isUsableCaretRect(rect) {
            if (!rect) {
                return false;
            }
            const left = Number(rect.left);
            const top = Number(rect.top);
            const right = Number(rect.right);
            const bottom = Number(rect.bottom);
            if (![left, top, right, bottom].every(Number.isFinite)) {
                return false;
            }
            const width = Math.abs(right - left);
            const height = Math.abs(bottom - top);
            return width > 0.5 || height > 0.5;
        }

        function rectSnapshot(rect) {
            if (!isUsableCaretRect(rect)) {
                return null;
            }
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
            if (!range) {
                return null;
            }
            const caret = range.cloneRange();
            caret.collapse(kind === 'start');
            const rects = Array.from(caret.getClientRects());
            return rects[0] || caret.getBoundingClientRect();
        }

        function pointCaretRect(point) {
            if (!point) {
                return null;
            }
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
                debugGesture('selection_handles_missing_rect', {
                    selectedText: text
                });
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
                        left = Math.min(
                            Math.max(8, rect.left + (rect.width / 2) - (actionWidth / 2)),
                            Math.max(8, window.innerWidth - actionWidth - 8),
                        );
                        const preferredTop = rect.top - actionHeight - 10;
                        top = preferredTop >= 8
                            ? preferredTop
                            : Math.min(
                                Math.max(8, rect.bottom + 10),
                                Math.max(8, window.innerHeight - actionHeight - 8),
                            );
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
            selectionHandleLayoutCache = { start: null, end: null };
            clearHoldTimer();
            hideSelectionAction();
            hideSelectionHandles();
            if (!keepTextSelection && window.getSelection) {
                const selection = window.getSelection();
                if (selection) {
                    selection.removeAllRanges();
                }
            }
        }

        function caretPointFromClient(clientX, clientY) {
            function firstTextDescendant(node) {
                if (!node) return null;
                if (node.nodeType === Node.TEXT_NODE) {
                    return node;
                }
                let child = node.firstChild;
                while (child) {
                    const found = firstTextDescendant(child);
                    if (found) {
                        return found;
                    }
                    child = child.nextSibling;
                }
                return null;
            }

            function lastTextDescendant(node) {
                if (!node) return null;
                if (node.nodeType === Node.TEXT_NODE) {
                    return node;
                }
                let child = node.lastChild;
                while (child) {
                    const found = lastTextDescendant(child);
                    if (found) {
                        return found;
                    }
                    child = child.previousSibling;
                }
                return null;
            }

            function normalizeCaretPoint(point) {
                if (!point || !point.node) {
                    return null;
                }
                if (point.node.nodeType === Node.TEXT_NODE) {
                    const text = point.node.textContent || '';
                    return {
                        node: point.node,
                        offset: Math.max(0, Math.min(point.offset, text.length))
                    };
                }
                const container = point.node;
                const children = container.childNodes || [];
                if (children.length === 0) {
                    return null;
                }
                const clampedOffset = Math.max(0, Math.min(point.offset, children.length));
                if (clampedOffset < children.length) {
                    const nextText = firstTextDescendant(children[clampedOffset]);
                    if (nextText) {
                        return {
                            node: nextText,
                            offset: 0
                        };
                    }
                }
                if (clampedOffset > 0) {
                    const previousText = lastTextDescendant(children[clampedOffset - 1]);
                    if (previousText) {
                        return {
                            node: previousText,
                            offset: (previousText.textContent || '').length
                        };
                    }
                }
                return null;
            }

            if (document.caretPositionFromPoint) {
                const pos = document.caretPositionFromPoint(clientX, clientY);
                if (pos && pos.offsetNode) {
                    const normalized = normalizeCaretPoint({ node: pos.offsetNode, offset: pos.offset });
                    debugGesture('selection_caret_point', {
                        source: 'caretPositionFromPoint',
                        clientX: Math.round(clientX),
                        clientY: Math.round(clientY),
                        nodeType: pos.offsetNode.nodeType,
                        normalizedNodeType: normalized ? normalized.node.nodeType : null
                    });
                    if (normalized) {
                        return normalized;
                    }
                    if (pos.offsetNode.nodeType === Node.TEXT_NODE) {
                        return { node: pos.offsetNode, offset: pos.offset };
                    }
                    debugGesture('selection_caret_rejected_non_text', {
                        source: 'caretPositionFromPoint',
                        nodeType: pos.offsetNode.nodeType
                    });
                    return null;
                }
            }
            if (document.caretRangeFromPoint) {
                const range = document.caretRangeFromPoint(clientX, clientY);
                if (range && range.startContainer) {
                    const normalized = normalizeCaretPoint({ node: range.startContainer, offset: range.startOffset });
                    debugGesture('selection_caret_point', {
                        source: 'caretRangeFromPoint',
                        clientX: Math.round(clientX),
                        clientY: Math.round(clientY),
                        nodeType: range.startContainer.nodeType,
                        normalizedNodeType: normalized ? normalized.node.nodeType : null
                    });
                    if (normalized) {
                        return normalized;
                    }
                    if (range.startContainer.nodeType === Node.TEXT_NODE) {
                        return { node: range.startContainer, offset: range.startOffset };
                    }
                    debugGesture('selection_caret_rejected_non_text', {
                        source: 'caretRangeFromPoint',
                        nodeType: range.startContainer.nodeType
                    });
                    return null;
                }
            }
            debugGesture('selection_caret_missing', {
                clientX: Math.round(clientX),
                clientY: Math.round(clientY)
            });
            return null;
        }

        function comparePoints(a, b) {
            if (!a || !b) return 0;
            try {
                const probe = document.createRange();
                probe.setStart(a.node, a.offset);
                probe.collapse(true);
                return probe.comparePoint(b.node, b.offset);
            } catch (error) {
                return 0;
            }
        }

        function buildRangeBetween(a, b) {
            if (!a || !b) return null;
            try {
                const order = comparePoints(a, b);
                const range = document.createRange();
                if (order > 0) {
                    range.setStart(a.node, a.offset);
                    range.setEnd(b.node, b.offset);
                } else if (order < 0) {
                    range.setStart(b.node, b.offset);
                    range.setEnd(a.node, a.offset);
                } else {
                    range.setStart(a.node, a.offset);
                    range.setEnd(b.node, b.offset);
                }
                return range;
            } catch (error) {
                return null;
            }
        }

        function applySelectionPoints(anchorPoint, focusPoint) {
            if (!anchorPoint || !focusPoint || !window.getSelection) {
                debugGesture('selection_apply_points_skipped', {
                    hasAnchor: !!anchorPoint,
                    hasFocus: !!focusPoint,
                    hasSelectionApi: !!window.getSelection
                });
                return false;
            }
            try {
                const selection = window.getSelection();
                if (!selection) {
                    debugGesture('selection_apply_points_missing_selection', {});
                    return false;
                }
                selection.removeAllRanges();
                if (typeof selection.setBaseAndExtent === 'function') {
                    selection.setBaseAndExtent(
                        anchorPoint.node,
                        anchorPoint.offset,
                        focusPoint.node,
                        focusPoint.offset,
                    );
                } else {
                    const fallbackRange = buildRangeBetween(anchorPoint, focusPoint);
                    if (!fallbackRange) {
                        debugGesture('selection_apply_points_no_range', {
                            anchorPoint: describePoint(anchorPoint),
                            focusPoint: describePoint(focusPoint)
                        });
                        return false;
                    }
                    selection.addRange(fallbackRange);
                }
                const appliedRange = selection.rangeCount > 0 ? selection.getRangeAt(0) : null;
                debugGesture('selection_apply_points_success', {
                    selectedText: currentSelectedText(),
                    anchorPoint: describePoint(anchorPoint),
                    focusPoint: describePoint(focusPoint),
                    rangeStart: appliedRange ? describePoint({ node: appliedRange.startContainer, offset: appliedRange.startOffset }) : null,
                    rangeEnd: appliedRange ? describePoint({ node: appliedRange.endContainer, offset: appliedRange.endOffset }) : null
                });
                if (selectionHandleDrag) {
                    hideSelectionAction();
                    updateSelectionHandles();
                } else {
                    updateSelectionAction();
                }
                return true;
            } catch (error) {
                debugGesture('selection_apply_points_error', {
                    error: String(error),
                    anchorPoint: describePoint(anchorPoint),
                    focusPoint: describePoint(focusPoint)
                });
                return false;
            }
        }

        function expandCollapsedRangeToWord(range) {
            if (!range || !range.collapsed) {
                return range;
            }
            const node = range.startContainer;
            if (!node || node.nodeType !== Node.TEXT_NODE) {
                return range;
            }
            const text = node.textContent || '';
            if (!text) {
                return range;
            }
            let start = Math.max(0, Math.min(range.startOffset, text.length));
            let end = start;
            while (start > 0 && !/\s/.test(text.charAt(start - 1))) {
                start -= 1;
            }
            while (end < text.length && !/\s/.test(text.charAt(end))) {
                end += 1;
            }
            if (start === end) {
                return range;
            }
            const expanded = document.createRange();
            expanded.setStart(node, start);
            expanded.setEnd(node, end);
            return expanded;
        }

        function applySelectionRange(range) {
            if (!range || !window.getSelection) {
                debugGesture('selection_apply_skipped', {
                    hasRange: !!range,
                    hasSelectionApi: !!window.getSelection
                });
                return false;
            }
            try {
                const selection = window.getSelection();
                if (!selection) {
                    debugGesture('selection_apply_missing_selection', {});
                    return false;
                }
                selection.removeAllRanges();
                selection.addRange(range);
                debugGesture('selection_apply_success', {
                    selectedText: currentSelectedText(),
                    collapsed: !!range.collapsed,
                    rangeStart: describePoint({ node: range.startContainer, offset: range.startOffset }),
                    rangeEnd: describePoint({ node: range.endContainer, offset: range.endOffset })
                });
                if (selectionHandleDrag) {
                    hideSelectionAction();
                    updateSelectionHandles();
                } else {
                    updateSelectionAction();
                }
                return true;
            } catch (error) {
                debugGesture('selection_apply_error', {
                    error: String(error)
                });
                return false;
            }
        }

        function beginCustomSelection(clientX, clientY) {
            const point = caretPointFromClient(clientX, clientY);
            if (!point) {
                debugGesture('selection_begin_failed_no_point', {
                    clientX: Math.round(clientX),
                    clientY: Math.round(clientY)
                });
                return false;
            }
            const collapsed = buildRangeBetween(point, point);
            const initialRange = expandCollapsedRangeToWord(collapsed);
            const nextAnchor = initialRange ? {
                node: initialRange.startContainer,
                offset: initialRange.startOffset
            } : null;
            const nextFocus = initialRange ? {
                node: initialRange.endContainer,
                offset: initialRange.endOffset
            } : null;
            if (!applySelectionPoints(nextAnchor, nextFocus)) {
                debugGesture('selection_begin_failed_apply', {
                    clientX: Math.round(clientX),
                    clientY: Math.round(clientY)
                });
                return false;
            }
            selectionState = {
                anchor: nextAnchor,
                focus: nextFocus
            };
            selectionGestureActive = true;
            selectionHandleDrag = null;
            selectionHandleFixedPoint = null;
            pendingStart = null;
            clearHoldTimer();
            activateWindow();
            debugGesture('selection_mode_begin', { selectionText: currentSelectedText() });
            return true;
        }

        function updateCustomSelection(clientX, clientY) {
            if (!selectionState || !selectionGestureActive) {
                debugGesture('selection_update_no_state', {});
                return false;
            }
            const point = caretPointFromClient(clientX, clientY);
            if (!point) {
                debugGesture('selection_update_failed_no_point', {
                    clientX: Math.round(clientX),
                    clientY: Math.round(clientY)
                });
                return false;
            }
            let range = buildRangeBetween(selectionState.anchor, point);
            if (range && range.collapsed) {
                range = expandCollapsedRangeToWord(range);
            }
            const nextAnchor = selectionState.anchor;
            const nextFocus = range ? {
                node: range.endContainer,
                offset: range.endOffset
            } : point;
            if (!applySelectionPoints(nextAnchor, nextFocus)) {
                debugGesture('selection_update_failed_apply', {
                    clientX: Math.round(clientX),
                    clientY: Math.round(clientY)
                });
                return false;
            }
            selectionState.focus = nextFocus;
            debugGesture('selection_update_success', {
                clientX: Math.round(clientX),
                clientY: Math.round(clientY),
                selectedText: currentSelectedText()
            });
            return true;
        }

        function currentRangePoints() {
            if (!window.getSelection) return null;
            const selection = window.getSelection();
            if (!selection || selection.rangeCount === 0) return null;
            const range = selection.getRangeAt(0);
            return {
                start: { node: range.startContainer, offset: range.startOffset },
                end: { node: range.endContainer, offset: range.endOffset }
            };
        }

        function updateSelectionFromHandle(clientX, clientY) {
            if (!selectionState || !selectionHandleDrag || !selectionHandleFixedPoint) {
                return false;
            }
            const point = caretPointFromClient(clientX, clientY);
            if (!point) {
                return false;
            }
            const nextAnchor = selectionHandleDrag === 'start' ? point : selectionHandleFixedPoint;
            const nextFocus = selectionHandleDrag === 'start' ? selectionHandleFixedPoint : point;
            debugGesture('selection_handle_candidate', {
                handle: selectionHandleDrag,
                candidatePoint: describePoint(point),
                fixedPoint: describePoint(selectionHandleFixedPoint),
                nextAnchor: describePoint(nextAnchor),
                nextFocus: describePoint(nextFocus)
            });
            if (!applySelectionPoints(nextAnchor, nextFocus)) {
                debugGesture('selection_handle_apply_failed', {
                    handle: selectionHandleDrag,
                    clientX: Math.round(clientX),
                    clientY: Math.round(clientY)
                });
                return false;
            }
            if (selectionHandleDrag === 'start') {
                selectionState.anchor = nextAnchor;
                selectionState.focus = selectionHandleFixedPoint;
            } else {
                selectionState.anchor = selectionHandleFixedPoint;
                selectionState.focus = nextFocus;
            }
            debugGesture('selection_handle_update', {
                handle: selectionHandleDrag,
                clientX: Math.round(clientX),
                clientY: Math.round(clientY),
                selectedText: currentSelectedText()
            });
            return true;
        }

        function scheduleHandleUpdate(clientX, clientY) {
            pendingHandlePoint = { x: clientX, y: clientY };
            if (selectionHandleFrame) {
                return;
            }
            selectionHandleFrame = requestAnimationFrame(() => {
                selectionHandleFrame = null;
                const point = pendingHandlePoint;
                pendingHandlePoint = null;
                if (!point) {
                    return;
                }
                updateSelectionFromHandle(point.x, point.y);
            });
        }

        function scheduleCustomSelection(clientX, clientY) {
            clearHoldTimer();
            debugGesture('selection_schedule', {
                clientX: Math.round(clientX),
                clientY: Math.round(clientY),
                delayMs: CUSTOM_SELECTION_DELAY_MS
            });
            holdTimer = setTimeout(() => {
                holdTimer = null;
                if (!pendingStart || dragState || resizeState) {
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
            debugGesture('selection_copy', {
                selectedText: text
            });
            postJson({
                type: 'copySelectedText',
                windowId: activeWindowId,
                text: text
            });
            hideSelectionAction();
        }

        document.addEventListener('scroll', () => {
            if (selectionState) {
                updateSelectionAction();
            }
        }, true);
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
            if (userSelect === 'none') {
                return false;
            }
            if (element === document.body || element === document.documentElement) {
                return false;
            }
            return true;
        }

        function axisAllowsScroll(style, axis) {
            const axisValue = axis === 'x' ? (style.overflowX || style.overflow) : (style.overflowY || style.overflow);
            return axisValue !== 'hidden' && axisValue !== 'clip';
        }

        function elementCanScrollAxis(element, axis) {
            if (!element) {
                return false;
            }
            const style = window.getComputedStyle(element);
            if (!axisAllowsScroll(style, axis)) {
                return false;
            }
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
            if (!element) {
                return 0;
            }
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
            if (scroller && !seen.has(scroller)) {
                output.push(scroller);
            }
            output.sort((left, right) => {
                const leftIsRoot = left === scroller;
                const rightIsRoot = right === scroller;
                if (leftIsRoot !== rightIsRoot) {
                    return leftIsRoot ? 1 : -1;
                }
                return scrollabilityScore(right) - scrollabilityScore(left);
            });
            return output;
        }

        function findScrollableContainer(target, clientX, clientY) {
            const candidates = collectScrollableCandidates(clientX, clientY, target);
            return candidates.length > 0 ? candidates[0] : (document.scrollingElement || document.documentElement);
        }

        function tryScrollContainer(container, dx, dy) {
            const target = container && typeof container.scrollBy === 'function'
                ? container
                : window;
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
                if (tryScrollContainer(candidate, dx, dy)) {
                    return candidate;
                }
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
            if (Math.hypot(vx, vy) < INERTIA_MIN_VELOCITY) {
                return;
            }
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
            if (selectionTarget) {
                scheduleCustomSelection(point.clientX, point.clientY);
            }
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
                if (updateCustomSelection(point.clientX, point.clientY)) {
                    if (event.cancelable) event.preventDefault();
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
                debugGesture('touchend_selection', {
                    selectionText: currentSelectedText()
                });
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
            if (selectionState) {
                selectionGestureActive = false;
                selectionHandleDrag = null;
                selectionHandleFixedPoint = null;
                updateSelectionAction();
            }
        }, { passive: true });

        document.addEventListener('selectionchange', () => {
            if (selectionHandleDrag) {
                return;
            }
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

internal fun presetResultInteractionJavascript(): String {
    return presetResultJavascript()
}
