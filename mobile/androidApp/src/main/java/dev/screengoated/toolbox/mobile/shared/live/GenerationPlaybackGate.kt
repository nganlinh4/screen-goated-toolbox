package dev.screengoated.toolbox.mobile.shared.live

internal data class GenerationPlaybackChunk(
    val epoch: Long,
    val bytes: ByteArray,
)

/** Linearizes current-generation playback admission with interruption. */
internal class GenerationPlaybackGate {
    private val lock = Any()
    private var epoch = 0L

    val currentGeneration: Long
        get() = synchronized(lock) { epoch }

    fun tag(bytes: ByteArray): GenerationPlaybackChunk = synchronized(lock) {
        GenerationPlaybackChunk(epoch = epoch, bytes = bytes)
    }

    fun playIfCurrent(
        chunk: GenerationPlaybackChunk,
        play: (ByteArray) -> Unit,
    ): Boolean = synchronized(lock) {
        if (chunk.epoch != epoch) {
            false
        } else {
            play(chunk.bytes)
            true
        }
    }

    fun interrupt(stop: () -> Unit): Long = synchronized(lock) {
        epoch = if (epoch == Long.MAX_VALUE) 0L else epoch + 1L
        stop()
        epoch
    }
}
