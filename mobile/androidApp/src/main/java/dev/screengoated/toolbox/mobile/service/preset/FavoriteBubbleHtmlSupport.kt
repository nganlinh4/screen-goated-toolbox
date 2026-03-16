package dev.screengoated.toolbox.mobile.service.preset

internal fun favoriteBubbleBaseHtmlTemplate(): String {
    return """
        <!DOCTYPE html>
        <html>
        <head>
        <meta charset="UTF-8">
        <meta
            name="viewport"
            content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no"
        >
        <style>{{FONT_CSS}}</style>
        <style>{{PANEL_CSS}}</style>
        </head>
        <body>
        <div class="container">
            <div class="keep-open-row visible" id="keepOpenRow">
                <span class="keep-open-label{{KEEP_OPEN_CLASS}}" id="keepOpenLabel" onclick="toggleKeepOpen()">{{KEEP_OPEN_LABEL}}</span>
                <div class="size-pill">
                    <button class="size-btn" onclick="resizeBubble('desc')">-</button>
                    <button class="size-btn" onclick="resizeBubble('inc')">+</button>
                </div>
            </div>
            <div class="list" style="column-count: {{COLUMN_COUNT}};">{{FAVORITES_HTML}}</div>
        </div>
        <script>
        window.ipc = {
            postMessage(message) {
                if (window.sgtAndroid && window.sgtAndroid.postMessage) {
                    window.sgtAndroid.postMessage(String(message));
                }
            }
        };
        let keepOpen = {{KEEP_OPEN_DEFAULT}};
        {{PANEL_JS}}
        </script>
        </body>
        </html>
    """.trimIndent()
}

internal fun favoriteBubblePanelCss(isDark: Boolean): String {
    val textColor: String
    val itemBg: String
    val itemHoverBg: String
    val itemShadow: String
    val itemHoverShadow: String
    val emptyTextColor: String
    val emptyBg: String
    val emptyBorder: String
    val labelColor: String
    val toggleBg: String
    val toggleActiveBg: String
    val rowBg: String
    if (isDark) {
        textColor = "#eeeeee"
        itemBg = "rgba(20, 20, 30, 0.85)"
        itemHoverBg = "rgba(40, 40, 55, 0.95)"
        itemShadow = "0 2px 8px rgba(0, 0, 0, 0.2)"
        itemHoverShadow = "0 4px 12px rgba(0, 0, 0, 0.3)"
        emptyTextColor = "rgba(255, 255, 255, 0.6)"
        emptyBg = "rgba(20, 20, 30, 0.85)"
        emptyBorder = "rgba(255, 255, 255, 0.1)"
        labelColor = "rgba(255, 255, 255, 0.6)"
        toggleBg = "rgba(60, 60, 70, 0.8)"
        toggleActiveBg = "rgba(64, 196, 255, 0.9)"
        rowBg = "rgba(20, 20, 30, 0.85)"
    } else {
        textColor = "#222222"
        itemBg = "rgba(255, 255, 255, 0.92)"
        itemHoverBg = "rgba(240, 240, 245, 0.98)"
        itemShadow = "0 2px 8px rgba(0, 0, 0, 0.08)"
        itemHoverShadow = "0 4px 12px rgba(0, 0, 0, 0.12)"
        emptyTextColor = "rgba(0, 0, 0, 0.5)"
        emptyBg = "rgba(255, 255, 255, 0.92)"
        emptyBorder = "rgba(0, 0, 0, 0.08)"
        labelColor = "rgba(0, 0, 0, 0.6)"
        toggleBg = "rgba(200, 200, 210, 0.8)"
        toggleActiveBg = "rgba(33, 100, 200, 0.9)"
        rowBg = "rgba(255, 255, 255, 0.92)"
    }
    val itemHoverBorder = if (isDark) "rgba(255, 255, 255, 0.25)" else "rgba(0, 0, 0, 0.12)"

    return """
        * { margin: 0; padding: 0; box-sizing: border-box; }
        html, body {
            width: 100%;
            height: 100%;
            overflow: hidden;
            background: transparent;
            font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
            user-select: none;
            -webkit-user-select: none;
        }
        .container {
            display: flex;
            flex-direction: column;
            width: 100%;
            padding: 30px 20px;
            min-height: 100px;
            opacity: 0;
            pointer-events: none;
        }
        .container.ready { opacity: 1; pointer-events: auto; }
        .container.side-right { padding-left: 30px; padding-right: 10px; }
        .container.side-left { padding-left: 10px; padding-right: 30px; }
        .list {
            display: block;
            column-gap: 8px;
            width: 100%;
        }
        .preset-item, .empty {
            display: flex;
            align-items: center;
            width: 100%;
            min-width: 100%;
            max-width: 100%;
            padding: 8px 12px;
            border-radius: 12px;
            cursor: pointer;
            color: $textColor;
            font-size: 13px;
            font-variation-settings: 'wght' 500, 'wdth' 100, 'ROND' 100;
            background: $itemBg;
            backdrop-filter: blur(12px);
            box-shadow: $itemShadow;
            margin-bottom: 4px;
            break-inside: avoid;
            opacity: 0;
            transform: scale(0.95);
            will-change: transform, opacity;
            --dx: 0px;
            --dy: 0px;
        }
        @keyframes bloom {
            0% { opacity: 0; transform: translate(var(--dx), var(--dy)) scale(0.1); }
            60% { opacity: 1; }
            100% { opacity: 1; transform: translate(0, 0) scale(1); }
        }
        @keyframes retreat {
            0% { opacity: 1; transform: translate(0, 0) scale(1); }
            100% { opacity: 0; transform: translate(var(--dx), var(--dy)) scale(0.1); }
        }
        .preset-item.blooming, .empty.blooming {
            animation: bloom 0.4s cubic-bezier(0.2, 0.8, 0.2, 1) forwards;
        }
        .preset-item.retreating, .empty.retreating {
            animation: retreat 0.35s cubic-bezier(0.4, 0, 1, 1) both;
        }
        .preset-item.animate-done:hover {
            background: $itemHoverBg;
            border-color: $itemHoverBorder;
            box-shadow: $itemHoverShadow;
            font-variation-settings: 'wght' 650, 'wdth' 105, 'ROND' 100;
            transform: scale(1.03);
            transition: all 0.1s ease-out;
        }
        .preset-item.animate-done:active { transform: scale(0.98); }
        .keep-open-row {
            display: flex;
            align-items: center;
            justify-content: center;
            gap: 12px;
            padding: 8px 16px;
            margin-bottom: 12px;
            background: $rowBg;
            backdrop-filter: blur(12px);
            box-shadow: $itemShadow;
            border-radius: 20px;
            width: fit-content;
            margin-left: auto;
            margin-right: auto;
            opacity: 0;
            transform: translateY(15px) scale(0.95);
            pointer-events: none;
            transition:
                opacity 0.3s cubic-bezier(0.2, 0.8, 0.2, 1),
                transform 0.3s cubic-bezier(0.2, 0.8, 0.2, 1);
        }
        .keep-open-row.visible,
        .container:hover .keep-open-row {
            opacity: 1;
            transform: translateY(0) scale(1);
            pointer-events: auto;
        }
        .container.closing .keep-open-row,
        .container.closing:hover .keep-open-row {
            opacity: 0;
            transform: translateY(15px) scale(0.95);
            pointer-events: none;
        }
        .preset-item { position: relative; overflow: hidden; border: 1px solid transparent; }
        .progress-fill {
            position: absolute;
            top: 0;
            left: 0;
            width: 0%;
            height: 100%;
            background: rgba(64, 196, 255, 0.3);
            pointer-events: none;
            z-index: 0;
            transition: width 0.05s linear;
        }
        .preset-item .icon, .preset-item .name { position: relative; z-index: 1; }
        .icon { display: flex; align-items: center; margin-right: 10px; opacity: 0.9; }
        .name {
            flex: 1;
            min-width: 0;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }
        .empty {
            color: $emptyTextColor;
            text-align: center;
            padding: 12px;
            background: $emptyBg;
            border: 1px solid $emptyBorder;
            justify-content: center;
        }
        .condense {
            letter-spacing: -0.5px;
            font-variation-settings: 'wght' 500, 'wdth' 92, 'ROND' 100;
        }
        .condense-more {
            letter-spacing: -1px;
            font-size: 12px;
            font-variation-settings: 'wght' 500, 'wdth' 84, 'ROND' 100;
        }
        .condense-tight {
            letter-spacing: -1.2px;
            font-size: 11px;
            font-variation-settings: 'wght' 500, 'wdth' 76, 'ROND' 100;
        }
        .keep-open-label {
            color: $labelColor;
            font-size: 12px;
            font-variation-settings: 'wght' 500, 'wdth' 100;
            transition: all 0.2s;
            white-space: nowrap;
            cursor: pointer;
            padding: 4px 10px;
            border-radius: 10px;
            background: transparent;
        }
        .keep-open-label:hover { background: $toggleBg; }
        .keep-open-label.active {
            color: white;
            font-variation-settings: 'wght' 600, 'wdth' 105;
            background: $toggleActiveBg;
        }
        .size-pill {
            display: flex;
            background: $itemBg;
            border-radius: 10px;
            overflow: hidden;
            margin-left: 8px;
        }
        .size-btn {
            width: 22px;
            height: 20px;
            border: none;
            background: transparent;
            color: $textColor;
            display: flex;
            align-items: center;
            justify-content: center;
            cursor: pointer;
            transition: background 0.2s;
            font-size: 14px;
        }
        .size-btn:hover { background: $itemHoverBg; }
    """.trimIndent()
}

internal fun favoriteBubblePanelJavascript(): String {
    return """
        function fitText() {
            requestAnimationFrame(() => {
                document.querySelectorAll('.name').forEach(el => {
                    el.className = 'name';
                    if (el.scrollWidth > el.clientWidth) {
                        el.classList.add('condense');
                        if (el.scrollWidth > el.clientWidth) {
                            el.classList.remove('condense');
                            el.classList.add('condense-more');
                            if (el.scrollWidth > el.clientWidth) {
                                el.classList.remove('condense-more');
                                el.classList.add('condense-tight');
                            }
                        }
                    }
                });
                sendHeight();
            });
        }
        function resizeBubble(dir) {
            if (dir === 'inc') window.ipc.postMessage('increase_size');
            else window.ipc.postMessage('decrease_size');
        }
        window.onload = function() {
            fitText();
            window.ipc.postMessage('panel_ready');
        };
        function sendHeight() {
            const container = document.querySelector('.container');
            if (container) {
                window.ipc.postMessage('resize:' + Math.max(container.scrollHeight, container.offsetHeight));
            }
        }
        document.addEventListener('mousedown', () => window.ipc.postMessage('focus_bubble'));
        function toggleKeepOpen() {
            keepOpen = !keepOpen;
            const label = document.getElementById('keepOpenLabel');
            label.classList.toggle('active', keepOpen);
            window.ipc.postMessage('set_keep_open:' + (keepOpen ? '1' : '0'));
        }
        let holdTimer = null;
        const HOLD_THRESHOLD = 500;
        function onMouseDown(idx) {
            const item = event.currentTarget;
            const fill = item.querySelector('.progress-fill');
            if (fill) {
                fill.style.width = '0%';
                fill.style.transition = 'width ' + HOLD_THRESHOLD + 'ms linear';
                requestAnimationFrame(() => fill.style.width = '100%');
            }
            holdTimer = setTimeout(() => {
                holdTimer = null;
                triggerContinuous(idx);
            }, HOLD_THRESHOLD);
        }
        function onMouseUp(idx) {
            if (holdTimer) {
                clearTimeout(holdTimer);
                holdTimer = null;
                triggerNormal(idx);
            }
            resetFill();
        }
        function onMouseLeave() {
            if (holdTimer) {
                clearTimeout(holdTimer);
                holdTimer = null;
            }
            resetFill();
        }
        function resetFill() {
            document.querySelectorAll('.progress-fill').forEach(f => {
                f.style.transition = 'none';
                f.style.width = '0%';
            });
        }
        function triggerNormal(idx) {
            if (keepOpen) window.ipc.postMessage('trigger_only:' + idx);
            else { closePanel(); window.ipc.postMessage('trigger:' + idx); }
        }
        function triggerContinuous(idx) {
            if (keepOpen) window.ipc.postMessage('trigger_continuous_only:' + idx);
            else { closePanel(); window.ipc.postMessage('trigger_continuous:' + idx); }
        }
        document.querySelectorAll('.preset-item').forEach((item, idx) => {
            item.addEventListener('touchstart', function(e) {
                window.event = e;
                onMouseDown(idx);
            }, { passive: true });
            item.addEventListener('touchend', function(e) {
                window.event = e;
                onMouseUp(idx);
            }, { passive: true });
            item.addEventListener('touchcancel', onMouseLeave, { passive: true });
        });
        let currentTimeout = null;
        let bubbleCX = 0;
        let bubbleCY = 0;
        window.updateBubbleCenter = function(bx, by) {
            bubbleCX = bx;
            bubbleCY = by;
        };
        function animateIn(bx, by) {
            bubbleCX = bx;
            bubbleCY = by;
            if (currentTimeout) {
                clearTimeout(currentTimeout);
                currentTimeout = null;
            }
            const container = document.querySelector('.container');
            container.classList.add('ready');
            container.classList.remove('closing');
            const items = document.querySelectorAll('.preset-item, .empty');
            if (items.length === 0) return;
            const metrics = [];
            for (let i = 0; i < items.length; i++) {
                const item = items[i];
                const rect = item.getBoundingClientRect();
                if (rect.width === 0) {
                    metrics.push(null);
                    continue;
                }
                const iy = rect.top + rect.height / 2;
                const ix = rect.left + rect.width / 2;
                metrics.push({ dx: bx - ix, dy: by - iy });
            }
            requestAnimationFrame(() => {
                items.forEach((item, i) => {
                    const m = metrics[i];
                    if (!m) return;
                    item.classList.remove('retreating', 'animate-done');
                    item.style.setProperty('--dx', m.dx + 'px');
                    item.style.setProperty('--dy', m.dy + 'px');
                    item.style.animationDelay = (i * 10) + 'ms';
                    item.classList.add('blooming');
                    setTimeout(() => item.classList.add('animate-done'), 400 + (i * 10));
                });
            });
        }
        function showItemsImmediately() {
            const container = document.querySelector('.container');
            if (container) {
                container.classList.add('ready');
                container.classList.remove('closing');
            }
            document.querySelectorAll('.preset-item, .empty').forEach(item => {
                item.classList.remove('retreating', 'blooming');
                item.classList.add('animate-done');
                item.style.animationDelay = '0ms';
                item.style.opacity = '1';
                item.style.transform = 'translate(0, 0) scale(1)';
            });
            sendHeight();
        }
        function closePanel() {
            if (currentTimeout) clearTimeout(currentTimeout);
            const container = document.querySelector('.container');
            container.classList.add('closing');
            const items = Array.from(document.querySelectorAll('.preset-item, .empty'));
            items.forEach((item, i) => {
                const rect = item.getBoundingClientRect();
                if (rect.width > 0) {
                    const ix = rect.left + rect.width / 2;
                    const iy = rect.top + rect.height / 2;
                    item.style.setProperty('--dx', (bubbleCX - ix) + 'px');
                    item.style.setProperty('--dy', (bubbleCY - iy) + 'px');
                }
                item.style.animationDelay = ((items.length - 1 - i) * 6) + 'ms';
                item.classList.remove('blooming', 'animate-done');
                item.classList.add('retreating');
            });
            currentTimeout = setTimeout(() => {
                window.ipc.postMessage('close_now');
                currentTimeout = null;
            }, items.length * 6 + 350);
        }
        window.setSide = function(side, bubbleOverlap) {
            const container = document.querySelector('.container');
            container.classList.remove('side-left', 'side-right');
            container.classList.add('side-' + side);
            if (side === 'right') {
                container.style.paddingLeft = '30px';
                container.style.paddingRight = (10 + bubbleOverlap) + 'px';
            } else {
                container.style.paddingLeft = (10 + bubbleOverlap) + 'px';
                container.style.paddingRight = '30px';
            }
        };
    """.trimIndent()
}
