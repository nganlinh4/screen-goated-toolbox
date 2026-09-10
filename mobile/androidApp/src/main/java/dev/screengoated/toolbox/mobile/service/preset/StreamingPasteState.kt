package dev.screengoated.toolbox.mobile.service.preset

/** Delivery authority survives provider failure, but callbacks never survive their generation. */
internal class StreamingPasteState {
    var generation = 0L
        private set
    var inserted = false
        private set
    private var handled = false
    val suppressFinalPaste: Boolean get() = handled || inserted

    fun accept(callbackGeneration: Long): Boolean {
        if (callbackGeneration != generation) return false
        handled = true
        return true
    }

    fun recordInsertion(attempted: Boolean) { inserted = inserted || attempted }

    fun invalidate() { generation++ }

    fun reset() {
        invalidate()
        inserted = false
        handled = false
    }
}
