package dev.screengoated.toolbox.mobile.service.preset

/** Bounded setup replay queue; overflow must trigger complete-recording fallback. */
internal class PendingStreamingAudio(private val limit: Int = 16_000 * 60) {
    private val chunks = ArrayDeque<ShortArray>()
    private var count = 0

    @Synchronized fun append(chunk: ShortArray) {
        check(chunk.size <= limit - count) { "Streaming setup backlog exceeded" }
        chunks.addLast(chunk.copyOf())
        count += chunk.size
    }

    @Synchronized fun removeFirstOrNull(): ShortArray? = chunks.removeFirstOrNull()?.also { count -= it.size }

    @Synchronized fun clear() {
        chunks.clear()
        count = 0
    }
}
