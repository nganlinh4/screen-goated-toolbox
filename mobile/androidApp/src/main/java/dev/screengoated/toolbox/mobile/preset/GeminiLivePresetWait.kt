package dev.screengoated.toolbox.mobile.preset

import java.util.concurrent.LinkedBlockingDeque
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlin.coroutines.coroutineContext

internal const val GEMINI_LIVE_PRESET_CANCEL_POLL_MILLIS = 50L

internal suspend fun awaitGeminiLivePresetEvent(
    events: LinkedBlockingDeque<GeminiLivePresetEvent>,
    timeoutMillis: Long,
): GeminiLivePresetEvent? {
    val started = System.nanoTime()
    while (true) {
        coroutineContext.ensureActive()
        events.poll()?.let {
            coroutineContext.ensureActive()
            return it
        }
        val remaining = timeoutMillis - (System.nanoTime() - started) / 1_000_000
        if (remaining <= 0) return null
        delay(minOf(GEMINI_LIVE_PRESET_CANCEL_POLL_MILLIS, remaining))
    }
}
