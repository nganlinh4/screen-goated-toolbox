package dev.screengoated.toolbox.mobile.service.preset

/** Keeps complete short attachments, never a misleading fragment of a long session. */
internal class StreamingAudioAttachment(private val limit: Int = 16_000 * 60 * 10) {
    private val chunks = ArrayDeque<ShortArray>()
    private var count = 0
    var omitted = false
        private set
    fun append(chunk: ShortArray, bounded: Boolean): Boolean {
        if (omitted) return false
        if (bounded && chunk.size > limit - count) {
            chunks.clear()
            count = 0
            omitted = true
            return true
        }
        chunks.addLast(chunk.copyOf())
        count += chunk.size
        return false
    }
    fun toShortArray(): ShortArray {
        val result = ShortArray(count)
        var offset = 0
        for (chunk in chunks) { chunk.copyInto(result, offset); offset += chunk.size }
        return result
    }
    fun clear() { chunks.clear(); count = 0; omitted = false }
}
