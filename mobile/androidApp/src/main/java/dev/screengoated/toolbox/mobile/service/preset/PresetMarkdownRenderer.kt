package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import dev.screengoated.toolbox.mobile.preset.isInputAdapterMediaHtml
import dev.screengoated.toolbox.mobile.ui.icons.MaterialSymbolSvgs
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
    private val symbols = MaterialSymbolSvgs.fromAssets(appContext)
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

    fun render(
        markdown: String,
        isDark: Boolean,
    ): PresetRenderedContent {
        val rawHtml = normalizePresetHtmlContent(markdown)
        if (rawHtml != null) {
            return PresetRenderedContent(
                html = prepareRawHtmlDocument(rawHtml, isDark),
                isRawHtmlDocument = true,
            )
        }
        val document = parser.parse(markdown)
        return PresetRenderedContent(
            html = renderer.render(document),
            isRawHtmlDocument = false,
        )
    }

    private fun prepareRawHtmlDocument(
        content: String,
        isDark: Boolean,
    ): String {
        val isInputAdapterMedia = isInputAdapterMediaHtml(content)
        val withSymbols = if (isInputAdapterMedia) symbols.resolveSlots(content) else content
        val withStorage = injectStoragePolyfill(withSymbols)
        val withGrid = injectGridJs(withStorage)
        val withScrollbars = injectScrollbarCss(withGrid)
        return injectIntoHead(
            withScrollbars,
            """
            <style>
            ${presetHostedRawPageCss(isDark, isInputAdapterMedia)}
            html, body {
                -webkit-tap-highlight-color: transparent;
                scrollbar-width: none;
            }
            html::-webkit-scrollbar, body::-webkit-scrollbar { display: none; }
            </style>
            """.trimIndent(),
        )
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
        if (!html.contains("<table", ignoreCase = true)) {
            return html
        }
        val head = """
            <style>${asset("windows_gridjs_lib.css")}</style>
            <script>${asset("windows_gridjs_lib.js")}</script>
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
        val lower = html.asciiLowercase()
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
        val lower = html.asciiLowercase()
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

internal fun normalizePresetHtmlContent(content: String): String? {
    val trimmed = content.trim { it.isDocumentSpace() }.trimStart('\uFEFF').trimStart { it.isDocumentSpace() }
    val candidate = stripOuterPresetHtmlFence(trimmed)?.trim { it.isDocumentSpace() } ?: trimmed
    val lower = candidate.asciiLowercase()
    if (startsPresetHtmlDocument(lower)) {
        return candidate
    }
    return null
}

private fun startsPresetHtmlDocument(lower: String): Boolean {
    fun skipCommentsAndSpace(content: String, start: Int): Int? {
        var cursor = start
        while (true) {
            while (content.getOrNull(cursor)?.isAsciiSpace() == true) cursor++
            if (!content.startsWith("<!--", startIndex = cursor)) return cursor
            val bodyStart = cursor + 4
            val end = content.indexOf("-->", startIndex = bodyStart)
            if (end < 0) return null
            val body = content.substring(bodyStart, end)
            // Reject malformed comments whose browser recovery can end earlier.
            if (body.startsWith(">") || body.startsWith("->") || body.contains("--!>") || body.contains("<!--")) {
                return null
            }
            cursor = end + 3
        }
    }

    fun ambiguousScript(content: String, start: Int, end: Int): Boolean {
        var cursor = start
        while (cursor < end) {
            val escapedStart = indexOfHtmlTokenWithin(content, "<!--", cursor, end)
            if (escapedStart < 0) return false
            val sectionStart = escapedStart + 4
            val commentEnd = indexOfHtmlTokenWithin(content, "-->", sectionStart, end)
                .takeIf { it >= 0 } ?: end
            var scan = sectionStart
            while (scan < commentEnd) {
                val nested = indexOfHtmlTokenWithin(content, "<script", scan, commentEnd)
                if (nested < 0) break
                val after = nested + "<script".length
                val next = content.getOrNull(after)
                if (after < commentEnd && next != null && (next.isAsciiSpace() || next == '>' || next == '/')) {
                    return true
                }
                scan = after
            }
            if (commentEnd == end) return false
            cursor = commentEnd + 3
        }
        return false
    }

    fun openingEnd(rest: String, start: Int = 0): PresetHtmlTagEnd? {
        var state = PresetHtmlTagState.BEFORE_NAME
        var delimiter = '\u0000'
        for (index in start until rest.length) {
            val character = rest[index]
            if (index == start && character == '/') continue
            if (state == PresetHtmlTagState.QUOTED) {
                if (character == delimiter) state = PresetHtmlTagState.AFTER_QUOTED
                continue
            }
            if (character == '>') return PresetHtmlTagEnd(index + 1, state == PresetHtmlTagState.SELF_CLOSING)
            state = when {
                state == PresetHtmlTagState.SELF_CLOSING -> return null
                character.isAsciiSpace() && state == PresetHtmlTagState.NAME -> PresetHtmlTagState.AFTER_NAME
                character.isAsciiSpace() && state == PresetHtmlTagState.UNQUOTED -> PresetHtmlTagState.BEFORE_NAME
                character.isAsciiSpace() && state == PresetHtmlTagState.AFTER_QUOTED -> PresetHtmlTagState.BEFORE_NAME
                character.isAsciiSpace() -> state
                character == '=' && (state == PresetHtmlTagState.NAME || state == PresetHtmlTagState.AFTER_NAME) -> PresetHtmlTagState.BEFORE_VALUE
                (character == '\'' || character == '"') && state == PresetHtmlTagState.BEFORE_VALUE -> {
                    delimiter = character
                    PresetHtmlTagState.QUOTED
                }
                character == '/' && (state == PresetHtmlTagState.BEFORE_NAME || state == PresetHtmlTagState.NAME ||
                    state == PresetHtmlTagState.AFTER_NAME || state == PresetHtmlTagState.AFTER_QUOTED) -> PresetHtmlTagState.SELF_CLOSING
                character in "<'\"`=" -> return null
                state == PresetHtmlTagState.BEFORE_NAME || state == PresetHtmlTagState.AFTER_NAME -> PresetHtmlTagState.NAME
                state == PresetHtmlTagState.NAME -> PresetHtmlTagState.NAME
                state == PresetHtmlTagState.BEFORE_VALUE || state == PresetHtmlTagState.UNQUOTED -> PresetHtmlTagState.UNQUOTED
                else -> return null
            }
        }
        return null
    }

    fun doctypeTail(input: String): String? {
        fun trim(text: String): String = text.trimStart { it.isAsciiSpace() }
        fun quoted(text: String): String? {
            val quote = text.firstOrNull()?.takeIf { it == '\'' || it == '"' } ?: return null
            val end = text.indexOf(quote, startIndex = 1).takeIf { it >= 0 } ?: return null
            if (text.substring(1, end).any { it == '<' || it == '>' }) return null
            return text.substring(end + 1)
        }
        val text = trim(input)
        if (text.startsWith(">")) return text.substring(1)
        val public = text.startsWith("public")
        val rest = when {
            public -> text.substring(6)
            text.startsWith("system") -> text.substring(6)
            else -> return null
        }
        if (rest.firstOrNull()?.isAsciiSpace() != true) return null
        val afterFirst = quoted(trim(rest)) ?: return null
        val remaining = trim(afterFirst)
        if (remaining.startsWith(">")) return remaining.substring(1)
        if (!public || afterFirst.length == remaining.length) return null
        val afterSecond = trim(quoted(remaining) ?: return null)
        return afterSecond.takeIf { it.startsWith(">") }?.substring(1)
    }

    fun terminalRootClose(content: String): Boolean {
        val rawText = listOf("script", "style", "textarea", "title", "xmp", "iframe", "noembed", "noframes", "noscript")
        val sensitive = listOf("template", "select", "table", "frameset")
        val foreignNames = listOf("svg", "math")
        val contexts = mutableListOf<String>()
        val foreign = mutableListOf<PresetForeignContext>()
        fun tokenBoundary(character: Char?): Boolean =
            character != null && (character.isAsciiSpace() || character == '>' || character == '/')
        var cursor = 0
        while (true) {
            val index = content.indexOf('<', startIndex = cursor)
            if (index < 0) return false
            if (content.startsWith("<!--", startIndex = index)) {
                cursor = skipCommentsAndSpace(content, index) ?: return false
                continue
            }
            if (content.startsWith("<![cdata[", startIndex = index)) {
                if (foreign.isEmpty()) return false
                val end = content.indexOf("]]>", startIndex = index + 9)
                if (end < 0) return false
                // Outside foreign content, the declaration can end earlier as a comment.
                if (indexOfHtmlTokenWithin(content, "<", index + 9, end) >= 0) return false
                cursor = end + 3
                continue
            }
            if (content.startsWith("</html", startIndex = index) && tokenBoundary(content.getOrNull(index + 6))) {
                if (contexts.isNotEmpty() || foreign.isNotEmpty()) return false
                var end = index + 6
                while (content.getOrNull(end)?.isAsciiSpace() == true) end++
                if (content.getOrNull(end) != '>') return false
                return skipCommentsAndSpace(content, end + 1) == content.length
            }
            val closing = content.startsWith("</", startIndex = index)
            val nameStart = index + if (closing) 2 else 1
            val foreignTag = foreignNames.firstOrNull { name ->
                content.startsWith(name, startIndex = nameStart) && tokenBoundary(content.getOrNull(nameStart + name.length))
            }
            if (foreignTag != null) {
                val ending = openingEnd(content, index + 1) ?: return false
                if (closing) {
                    if (foreign.lastOrNull()?.name != foreignTag) return false
                    foreign.removeAt(foreign.lastIndex)
                } else if (!ending.selfClosing) {
                    if (foreign.size == 64) return false
                    val ambiguous = contexts.isNotEmpty() || foreign.lastOrNull()?.ambiguous == true
                    foreign.add(PresetForeignContext(foreignTag, ambiguous))
                }
                cursor = ending.offset
                continue
            }
            val context = sensitive.firstOrNull { name ->
                content.startsWith(name, startIndex = nameStart) && tokenBoundary(content.getOrNull(nameStart + name.length))
            }
            if (context != null) {
                val ending = openingEnd(content, index + 1) ?: return false
                foreign.forEach { it.ambiguous = true }
                if (closing) {
                    if (contexts.lastOrNull() != context) return false
                    contexts.removeAt(contexts.lastIndex)
                } else {
                    if (contexts.size == 64 || ((context == "select" || context == "table") && context in contexts)) return false
                    contexts.add(context)
                }
                cursor = ending.offset
                continue
            }
            if (content.startsWith("<plaintext", startIndex = index) && tokenBoundary(content.getOrNull(index + 10))) return false
            val raw = rawText.firstOrNull { name ->
                content.startsWith("<$name", startIndex = index) && tokenBoundary(content.getOrNull(index + name.length + 1))
            }
            if (raw != null) {
                val ending = openingEnd(content, index + 1) ?: return false
                if (ending.selfClosing && foreign.isNotEmpty()) {
                    if (foreign.last().ambiguous || contexts.isNotEmpty()) return false
                    cursor = ending.offset
                    continue
                }
                cursor = ending.offset
                val rawStart = cursor
                val marker = "</$raw"
                while (true) {
                    val close = content.indexOf(marker, startIndex = cursor)
                    if (close < 0) return false
                    cursor = close + marker.length
                    if (tokenBoundary(content.getOrNull(cursor))) {
                        if (foreign.isNotEmpty()) {
                            val rawPrefix = content.substring(rawStart, close)
                            if ('<' in rawPrefix) {
                                val trimmed = rawPrefix.trim { it.isDocumentSpace() }
                                if (!trimmed.startsWith("<![cdata[") || !trimmed.endsWith("]]>")) return false
                                val inner = trimmed.substring(9, trimmed.length - 3)
                                if ("]]>" in inner || '<' in inner) return false
                            }
                        }
                        var cdata = rawStart
                        while (true) {
                            val start = indexOfHtmlTokenWithin(content, "<![cdata[", cdata, close)
                            if (start < 0) break
                            val end = indexOfHtmlTokenWithin(content, "]]>", start + 9, close)
                            if (end < 0) return false
                            cdata = end + 3
                        }
                        if (raw == "script" && ambiguousScript(content, rawStart, close)) return false
                        cursor = openingEnd(content, cursor)?.offset ?: return false
                        break
                    }
                }
                continue
            }
            val next = content.getOrNull(index + 1)
            cursor = if (next != null && (next in 'a'..'z' || next == '/' || next == '!' || next == '?')) {
                val end = openingEnd(content, index + 1)?.offset ?: return false
                foreign.forEach { it.ambiguous = true }
                end
            } else {
                index + 1
            }
        }
    }

    val start = skipCommentsAndSpace(lower, 0) ?: return false
    val beginning = lower.substring(start)
    val root = if (beginning.startsWith("<!doctype")) {
        val rest = beginning.substring(9)
        if (rest.firstOrNull()?.isAsciiSpace() != true) return false
        val name = rest.trimStart { it.isAsciiSpace() }
        if (!name.startsWith("html")) return false
        val tail = name.substring(4)
        if (tail.firstOrNull()?.let { it.isAsciiSpace() || it == '>' } != true) return false
        val afterDoctype = doctypeTail(tail) ?: return false
        afterDoctype.substring(skipCommentsAndSpace(afterDoctype, 0) ?: return false)
    } else {
        beginning
    }
    if (!root.startsWith("<html")) return false
    val tail = root.substring(5)
    if (tail.firstOrNull()?.let { it.isAsciiSpace() || it == '>' } != true) return false
    return terminalRootClose(tail.substring(openingEnd(tail)?.offset ?: return false))
}

private fun Char.isAsciiSpace(): Boolean = this == ' ' || this == '\t' || this == '\n' || this == '\u000C' || this == '\r'

private enum class PresetHtmlTagState {
    BEFORE_NAME, NAME, AFTER_NAME, BEFORE_VALUE, UNQUOTED, QUOTED, AFTER_QUOTED, SELF_CLOSING,
}

private data class PresetHtmlTagEnd(val offset: Int, val selfClosing: Boolean)

private data class PresetForeignContext(val name: String, var ambiguous: Boolean)

// Match Unicode White_Space used by Rust's str::trim, independently of the
// platform's JVM whitespace tables.
private fun Char.isDocumentSpace(): Boolean =
    this in '\u0009'..'\u000D' || this == '\u0020' || this == '\u0085' ||
        this == '\u00A0' || this == '\u1680' || this in '\u2000'..'\u200A' ||
        this == '\u2028' || this == '\u2029' || this == '\u202F' ||
        this == '\u205F' || this == '\u3000'

private fun stripOuterPresetHtmlFence(content: String): String? {
    val marker = content.firstOrNull()?.takeIf { it == '`' || it == '~' } ?: return null
    val markerLength = content.takeWhile { it == marker }.length
    if (markerLength < 3) return null
    val headerEnd = content.indexOfAny(charArrayOf('\r', '\n')).takeIf { it >= 0 } ?: return null
    val label = content.substring(markerLength, headerEnd).trim { it.isDocumentSpace() }
    if (label.isNotEmpty() && !label.equals("html", ignoreCase = true) && !label.equals("htm", ignoreCase = true)) {
        return null
    }
    val headerBreak = if (content.getOrNull(headerEnd) == '\r' && content.getOrNull(headerEnd + 1) == '\n') 2 else 1
    val bodyAndClose = content.substring(headerEnd + headerBreak)
    val closeStart = bodyAndClose.lastIndexOfAny(charArrayOf('\r', '\n')).takeIf { it >= 0 } ?: return null
    fun isClosing(line: String): Boolean {
        val indent = line.takeWhile { it == ' ' }.length
        if (indent > 3) return false
        val trimmed = line.substring(indent).trimEnd { it.isDocumentSpace() }
        return trimmed.length >= markerLength && trimmed.all { it == marker }
    }
    val bodyEnd = if (closeStart > 0 && bodyAndClose[closeStart] == '\n' && bodyAndClose[closeStart - 1] == '\r') {
        closeStart - 1
    } else {
        closeStart
    }
    val body = bodyAndClose.substring(0, bodyEnd)
    if (!isClosing(bodyAndClose.substring(closeStart + 1)) || body.lineSequence().any(::isClosing)) {
        return null
    }
    return body
}

private fun String.asciiLowercase(): String = buildString(length) {
    for (character in this@asciiLowercase) {
        append(if (character in 'A'..'Z') character + ('a' - 'A') else character)
    }
}
