package dev.screengoated.toolbox.mobile.service.preset

internal data class PasteSnapshot(val text: String, val start: Int, val end: Int) {
    val collapsed: Boolean get() = start == end && start in 0..text.length
}

/** A target never rebinds: null snapshots and uncertain effects permanently revoke ownership. */
internal interface ProvisionalPasteTarget {
    val replaceable: Boolean
    fun snapshot(): PasteSnapshot?
    fun replace(expected: PasteSnapshot, replacement: PasteSnapshot): Boolean
}

/** Canonical session-owned tail; committed segments and surrounding user text are immutable. */
internal class ProvisionalPasteSession(private val target: ProvisionalPasteTarget?) {
    private var expected = target?.snapshot()?.takeIf { it.collapsed && it.text.length <= MAX_PASTE_TEXT_UNITS }
    private var prefix = expected?.let { it.text.substring(0, it.start) }.orEmpty()
    private val suffix = expected?.let { it.text.substring(it.end) }.orEmpty()
    private var provisional = ""
    private var draining = false
    private var closed = false
    var suspended = expected == null
        private set
    var attempted = false
        private set

    fun interim(text: String): Boolean {
        if (closed || draining || suspended || target?.replaceable != true) return false
        return replaceTail(normalizeStreamingText(text), commit = false)
    }

    fun finalSegment(text: String): Boolean {
        if (closed || suspended) return false
        return replaceTail(normalizeStreamingText(text), commit = true)
    }

    fun beginDrain() { draining = true }

    fun close() {
        if (closed) return
        closed = true
        if (!suspended && provisional.isNotEmpty()) replaceTail("", commit = false)
    }

    private fun replaceTail(text: String, commit: Boolean): Boolean {
        val before = expected ?: return false
        val backend = target ?: return false
        if (text.length > MAX_PASTE_INPUT_UNITS || prefix.length + text.length + suffix.length > MAX_PASTE_TEXT_UNITS) {
            suspended = true
            return false
        }
        if (backend.snapshot() != before) {
            suspended = true
            return false
        }
        val caret = prefix.length + text.length
        val after = PasteSnapshot(prefix + text + suffix, caret, caret)
        if (after != before) {
            attempted = true
            if (!backend.replace(before, after) || backend.snapshot() != after) {
                suspended = true
                return false
            }
        }
        expected = after
        if (commit) {
            prefix += text
            provisional = ""
        } else {
            provisional = text
        }
        return true
    }
}

internal const val MAX_PASTE_TEXT_UNITS = 262_144
internal const val MAX_PASTE_INPUT_UNITS = 16_384

internal fun normalizeStreamingText(text: String): String = buildString(text.length) {
    text.forEach { append(if (it.isISOControl() || it == '\u2028' || it == '\u2029') ' ' else it) }
}
