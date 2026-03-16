package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import org.commonmark.Extension
import org.commonmark.ext.gfm.strikethrough.StrikethroughExtension
import org.commonmark.ext.gfm.tables.TablesExtension
import org.commonmark.ext.task.list.items.TaskListItemsExtension
import org.commonmark.parser.Parser
import org.commonmark.renderer.html.HtmlRenderer

internal data class PresetRenderedContent(
    val html: String,
    val isRawHtmlDocument: Boolean,
)

internal class PresetMarkdownRenderer(
    context: Context,
) {
    private val appContext = context.applicationContext
    private val extensions: List<Extension> = listOf(
        TablesExtension.create(),
        StrikethroughExtension.create(),
        TaskListItemsExtension.create(),
    )
    private val parser = Parser.builder()
        .extensions(extensions)
        .build()
    private val renderer = HtmlRenderer.builder()
        .extensions(extensions)
        .escapeHtml(false)
        .softbreak("<br />")
        .build()

    fun render(markdown: String): PresetRenderedContent {
        if (isHtmlContent(markdown)) {
            return PresetRenderedContent(
                html = prepareRawHtmlDocument(markdown),
                isRawHtmlDocument = true,
            )
        }
        val document = parser.parse(markdown)
        return PresetRenderedContent(
            html = renderer.render(document),
            isRawHtmlDocument = false,
        )
    }

    private fun prepareRawHtmlDocument(content: String): String {
        val wrapped = if (isHtmlFragment(content)) {
            wrapHtmlFragment(content)
        } else {
            content
        }
        val withStorage = injectStoragePolyfill(wrapped)
        val withGrid = injectGridJs(withStorage)
        val withScrollbars = injectScrollbarCss(withGrid)
        val withBridge = injectIntoHead(
            withScrollbars,
            """
            <style>
            html, body {
                -webkit-tap-highlight-color: transparent;
                scrollbar-width: none;
            }
            html::-webkit-scrollbar, body::-webkit-scrollbar { display: none; }
            </style>
            """.trimIndent(),
        )
        return injectBeforeBodyClose(
            withBridge,
            """
            <script>
            window.ipc = {
                postMessage(message) {
                    if (window.sgtAndroid && window.sgtAndroid.postMessage) {
                        window.sgtAndroid.postMessage(String(message));
                    }
                }
            };
            ${presetResultInteractionJavascript()}
            </script>
            """.trimIndent(),
        )
    }

    private fun isHtmlContent(content: String): Boolean {
        val trimmed = content.trim()
        return trimmed.startsWith("<!DOCTYPE", ignoreCase = true) ||
            trimmed.startsWith("<html", ignoreCase = true) ||
            (trimmed.contains("<html", ignoreCase = true) && trimmed.contains("</html>", ignoreCase = true)) ||
            (trimmed.contains("<head", ignoreCase = true) && trimmed.contains("</head>", ignoreCase = true)) ||
            isHtmlFragment(content)
    }

    private fun isHtmlFragment(content: String): Boolean {
        val lower = content.lowercase()
        return (lower.contains("<script") || lower.contains("<style")) &&
            !lower.contains("<!doctype") &&
            !lower.contains("<html")
    }

    private fun wrapHtmlFragment(fragment: String): String {
        return """
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
            </head>
            <body>
            $fragment
            </body>
            </html>
        """.trimIndent()
    }

    private fun injectStoragePolyfill(html: String): String {
        val polyfill = """
            <script>
            (function() {
                try {
                    var test = '__storage_test__';
                    localStorage.setItem(test, test);
                    localStorage.removeItem(test);
                } catch (e) {
                    var createStorage = function() {
                        return {
                            _data: {},
                            length: 0,
                            getItem: function(key) { return this._data.hasOwnProperty(key) ? this._data[key] : null; },
                            setItem: function(key, value) { this._data[key] = String(value); this.length = Object.keys(this._data).length; },
                            removeItem: function(key) { delete this._data[key]; this.length = Object.keys(this._data).length; },
                            clear: function() { this._data = {}; this.length = 0; },
                            key: function(i) { var keys = Object.keys(this._data); return keys[i] || null; }
                        };
                    };
                    try {
                        Object.defineProperty(window, 'localStorage', { value: createStorage(), writable: false });
                        Object.defineProperty(window, 'sessionStorage', { value: createStorage(), writable: false });
                    } catch (e2) {
                        window.localStorage = createStorage();
                        window.sessionStorage = createStorage();
                    }
                }
            })();
            </script>
        """.trimIndent()
        return injectIntoHead(html, polyfill)
    }

    private fun injectGridJs(html: String): String {
        if (!html.contains("<table")) {
            return html
        }
        val head = """
            <link href="https://unpkg.com/gridjs/dist/theme/mermaid.min.css" rel="stylesheet" />
            <script src="https://unpkg.com/gridjs/dist/gridjs.umd.js"></script>
            <style>${asset("windows_gridjs.css")}</style>
        """.trimIndent()
        val body = "<script>${asset("windows_gridjs_init.js")}</script>"
        return injectBeforeBodyClose(injectIntoHead(html, head), body)
    }

    private fun injectScrollbarCss(html: String): String {
        return injectIntoHead(html, "<style>::-webkit-scrollbar { display: none; }</style>")
    }

    private fun injectIntoHead(
        html: String,
        payload: String,
    ): String {
        val lower = html.lowercase()
        val headClose = lower.indexOf("</head>")
        if (headClose >= 0) {
            return html.substring(0, headClose) + payload + html.substring(headClose)
        }
        val bodyStart = lower.indexOf("<body")
        if (bodyStart >= 0) {
            return html.substring(0, bodyStart) + payload + html.substring(bodyStart)
        }
        return payload + html
    }

    private fun injectBeforeBodyClose(
        html: String,
        payload: String,
    ): String {
        val lower = html.lowercase()
        val bodyClose = lower.indexOf("</body>")
        if (bodyClose >= 0) {
            return html.substring(0, bodyClose) + payload + html.substring(bodyClose)
        }
        return html + payload
    }

    private fun asset(name: String): String {
        return appContext.assets.open("preset_overlay/$name").bufferedReader().use { it.readText() }
    }
}
