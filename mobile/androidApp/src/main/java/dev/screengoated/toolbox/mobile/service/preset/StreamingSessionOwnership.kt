package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.AudioStreamingSession
import dev.screengoated.toolbox.mobile.preset.AudioStreamingTranscriptResult
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Own the acquired socket before a cancellable dispatcher handoff can discard it. */
internal suspend fun acquireStreamingSession(
    acquire: suspend () -> AudioStreamingSession?,
    install: suspend (AudioStreamingSession?) -> Unit,
    dispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    var pending: AudioStreamingSession? = null
    try {
        val session = withContext(dispatcher) { acquire().also { pending = it } }
        install(session)
        pending = null
    } finally {
        pending?.cancel()
    }
}

/** A failed live tail retains recorded WAV fallback; cancellation never submits it. */
internal suspend fun finalizeStreamingOrFallback(
    finalize: suspend () -> AudioStreamingTranscriptResult?,
    onFailure: (Throwable) -> Unit,
): AudioStreamingTranscriptResult? = try {
    finalize()
} catch (cancelled: CancellationException) {
    throw cancelled
} catch (error: Exception) {
    onFailure(error)
    null
}
