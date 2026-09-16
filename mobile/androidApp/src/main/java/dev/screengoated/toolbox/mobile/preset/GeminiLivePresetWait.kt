package dev.screengoated.toolbox.mobile.preset

import java.util.concurrent.LinkedBlockingDeque
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlin.coroutines.coroutineContext

internal suspend fun awaitGeminiLivePresetEvent(
    events: LinkedBlockingDeque<GeminiLivePresetEvent>,
    timeoutMillis: Long,
): GeminiLivePresetEvent? {
    val started = System.nanoTime()
    while (true) {
        coroutineContext.ensureActive()
        events.poll()?.let { return it }
        val remaining = timeoutMillis - (System.nanoTime() - started) / 1_000_000
        if (remaining <= 0) return null
        delay(minOf(50L, remaining))
    }
}
