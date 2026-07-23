package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.shared.live.GenerationPlaybackChunk
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import java.util.concurrent.atomic.AtomicInteger

internal class PhoneControlPlaybackQueue(capacity: Int) {
    private val queued = AtomicInteger(0)
    private val channel = Channel<GenerationPlaybackChunk>(
        capacity = capacity,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
        onUndeliveredElement = { decrement() },
    )

    init {
        require(capacity > 0)
    }

    fun offer(chunk: GenerationPlaybackChunk): Boolean {
        queued.incrementAndGet()
        val accepted = channel.trySend(chunk).isSuccess
        if (!accepted) decrement()
        return accepted
    }

    suspend fun consume(play: (GenerationPlaybackChunk) -> Unit) {
        for (chunk in channel) {
            try {
                play(chunk)
            } finally {
                decrement()
            }
        }
    }

    fun discard() {
        while (channel.tryReceive().isSuccess) decrement()
    }

    fun isDrained(pendingPlayerFrames: Long): Boolean =
        queued.get() == 0 && pendingPlayerFrames == 0L

    fun close() = channel.close()

    private fun decrement() {
        queued.updateAndGet { (it - 1).coerceAtLeast(0) }
    }
}
