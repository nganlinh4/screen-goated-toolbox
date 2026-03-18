package dev.screengoated.toolbox.mobile.service.preset

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

internal fun jsStringLiteral(value: String): String {
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
