package dev.screengoated.toolbox.mobile.service

import java.io.IOException
import java.util.concurrent.LinkedBlockingDeque
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.delay

internal class LiveResumption(var handle: String? = null)

internal class LiveSetupFailure(message: String, val retryable: Boolean) : IOException(message)

internal fun sendLiveSetup(setup: CompletableDeferred<Unit>, send: () -> Boolean) {
    if (!send()) setup.completeExceptionally(IOException("Live setup write failed"))
}

internal fun closeLiveSocket(
    setup: CompletableDeferred<Unit>,
    events: LinkedBlockingDeque<GeminiLiveSocketClient.LiveSocketEvent>,
) {
    if (!setup.isCompleted) setup.completeExceptionally(IOException("Live socket closed during setup"))
    events.offer(GeminiLiveSocketClient.LiveSocketEvent.Closed)
}

/** Serializes whole callback admission against retirement of one socket generation. */
internal class LiveEventInbox {
    val events = LinkedBlockingDeque<GeminiLiveSocketClient.LiveSocketEvent>()
    private var retired = false

    @Synchronized fun admit(callback: () -> Unit) {
        if (!retired) callback()
    }

    @Synchronized fun retire() { retired = true }
}

internal suspend fun delayWhileCaptureActive(millis: Long, captureEnded: () -> Boolean) {
    var remaining = millis
    while (remaining > 0 && !captureEnded()) {
        val step = minOf(remaining, 100L)
        delay(step)
        remaining -= step
    }
}

internal suspend fun awaitLiveSetup(setup: CompletableDeferred<Unit>, captureEnded: () -> Boolean): Boolean {
    while (!setup.isCompleted) {
        if (captureEnded()) return false
        delay(100)
    }
    setup.await()
    return !captureEnded()
}

/** Deliver admitted observations before reconnecting a retired socket generation. */
internal suspend fun <T> recoverRetiredLiveSession(
    events: LinkedBlockingDeque<GeminiLiveSocketClient.LiveSocketEvent>,
    onTranscript: (String, Boolean) -> Unit,
    onResumption: (String) -> Unit,
    onProgress: () -> Unit,
    reconnect: suspend () -> T?,
): T? {
    while (true) {
        when (val event = events.poll() ?: break) {
            is GeminiLiveSocketClient.LiveSocketEvent.Transcript -> onTranscript(event.text, event.isFinal)
            is GeminiLiveSocketClient.LiveSocketEvent.Resumption -> onResumption(event.handle)
            GeminiLiveSocketClient.LiveSocketEvent.Progress -> onProgress()
            is GeminiLiveSocketClient.LiveSocketEvent.Error -> if (!event.retryable) throw IOException(event.message)
            GeminiLiveSocketClient.LiveSocketEvent.GoAway,
            GeminiLiveSocketClient.LiveSocketEvent.Closed -> Unit
        }
    }
    return reconnect()
}

/** Attempts a resumed connection once, then abandons that handle before a fresh setup. */
internal suspend fun <T> reconnectLive(
    resumption: LiveResumption,
    attempts: Int,
    captureEnded: () -> Boolean,
    connect: suspend (String?) -> T?,
    pause: suspend (Int) -> Unit,
): T? {
    repeat(attempts) { attempt ->
        if (captureEnded()) return null
        val handle = resumption.handle.takeIf { attempt == 0 }
        val next = try {
            connect(handle)
        } catch (error: LiveSetupFailure) {
            if (handle == null || error.retryable) throw error
            null
        }
        if (next != null) return next
        if (attempt == 0) resumption.handle = null
        if (captureEnded()) return null
        if (attempt + 1 < attempts) pause(attempt)
    }
    if (captureEnded()) return null
    throw IOException("Live transcription could not reconnect")
}
