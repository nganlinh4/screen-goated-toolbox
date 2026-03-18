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
        <script>{{FIT_SCRIPT}}</script>
        <script src="{{GRIDJS_JS_URL}}"></script>
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
        .sgt-loading-shell {
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            flex-direction: column;
            gap: 14px;
            font: 600 15px/1.2 "Google Sans Flex", system-ui, sans-serif;
            letter-spacing: 0.01em;
            color: inherit;
            user-select: none;
            -webkit-user-select: none;
        }
        .sgt-loading-indicator {
            width: 30px;
            height: 30px;
            border-radius: 50%;
            border: 3px solid rgba(127, 127, 127, 0.18);
            border-top-color: rgba(67, 124, 255, 0.96);
            animation: sgt-loading-spin 0.9s linear infinite;
        }
        .sgt-loading-label {
            opacity: 0.78;
        }
        @keyframes sgt-loading-spin {
            from { transform: rotate(0deg); }
            to { transform: rotate(360deg); }
        }
    """.trimIndent()
}
