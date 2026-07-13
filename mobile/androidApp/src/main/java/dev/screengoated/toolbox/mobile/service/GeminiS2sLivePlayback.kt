package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect

internal data class LiveTranslatePlaybackChunk(
    val epoch: Long,
    val bytes: ByteArray,
)

/**
 * Serializes playback admission with interruption.
 *
 * A chunk removed from the channel can still be waiting to enter the player. Holding this gate
 * through the blocking player call makes interruption linearizable: either playback wins first
 * and is stopped afterward, or interruption wins and the stale chunk is rejected.
 */
internal class LiveTranslatePlaybackEpoch {
    private val lock = Any()
    private var epoch = 0L

    fun tag(bytes: ByteArray): LiveTranslatePlaybackChunk = synchronized(lock) {
        LiveTranslatePlaybackChunk(epoch = epoch, bytes = bytes)
    }

    fun playIfCurrent(
        chunk: LiveTranslatePlaybackChunk,
        play: (ByteArray) -> Unit,
    ): Boolean = synchronized(lock) {
        if (chunk.epoch != epoch) {
            false
        } else {
            play(chunk.bytes)
            true
        }
    }

    fun interrupt(stop: () -> Unit) = synchronized(lock) {
        check(epoch < Long.MAX_VALUE) { "Live Translate playback epoch exhausted" }
        epoch++
        stop()
    }
}

internal fun executeLiveTranslateFeatureEffects(
    effects: List<GeminiLiveLifecycleEffect>,
    deliverContent: (Int) -> Unit,
    stopPlayback: () -> Unit,
    discardQueuedOutput: () -> Unit,
) {
    effects.forEach { effect ->
        when (effect) {
            is GeminiLiveLifecycleEffect.DeliverContent -> deliverContent(effect.count)
            GeminiLiveLifecycleEffect.StopPlayback -> stopPlayback()
            GeminiLiveLifecycleEffect.DiscardQueuedOutput -> discardQueuedOutput()
            GeminiLiveLifecycleEffect.FinalizeGeneration,
            GeminiLiveLifecycleEffect.FinalizeTurn,
            GeminiLiveLifecycleEffect.FinalizeInterruptedGeneration,
            -> Unit
            is GeminiLiveLifecycleEffect.DispatchTools,
            is GeminiLiveLifecycleEffect.CancelTools,
            -> error("Live Translate does not support Gemini Live tool effects")
            is GeminiLiveLifecycleEffect.OpenSocket,
            is GeminiLiveLifecycleEffect.SendSetup,
            is GeminiLiveLifecycleEffect.CloseSocket,
            is GeminiLiveLifecycleEffect.ScheduleReconnect,
            is GeminiLiveLifecycleEffect.ReportFailure,
            GeminiLiveLifecycleEffect.CancelSession,
            is GeminiLiveLifecycleEffect.FinalizeResponse,
            -> error("Transport lifecycle effect escaped the Live Translate adapter")
        }
    }
}
