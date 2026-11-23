package dev.screengoated.toolbox.mobile.translationgummy

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEvent
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePolicy
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveHttpFailure
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionFailure
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionLifecycle
import dev.screengoated.toolbox.mobile.shared.live.geminiLiveCloseRetryable
import java.io.IOException
import java.net.ProtocolException
import javax.net.ssl.SSLHandshakeException
import javax.net.ssl.SSLPeerUnverifiedException

internal class TranslationGummyClosed(val code: Int?) : IOException("Translation Gummy websocket closed ($code).")

internal fun gummyLifecycle() = GeminiLiveSessionLifecycle(
    GeminiLiveLifecyclePolicy.continuous().copy(
        serverIdleTimeoutMs = null,
        rotateAfterMs = null,
        maxReconnectAttempts = 6,
    ),
)

internal fun gummyFailureRetryable(failure: GeminiLiveSessionFailure): Boolean = when (failure) {
    is GeminiLiveSessionFailure.Server -> failure.retryable
    is GeminiLiveSessionFailure.ClosedBeforeReady -> geminiLiveCloseRetryable(failure.code)
    is GeminiLiveSessionFailure.Transport -> gummyTransportRetryable(failure.cause)
    else -> true
}

internal fun gummyTransportRetryable(error: Throwable): Boolean = when (error) {
    is GeminiLiveHttpFailure -> error.status == 408 || error.status == 429 || error.status in 500..599
    is SSLHandshakeException, is SSLPeerUnverifiedException, is ProtocolException -> false
    else -> true
}

internal fun GeminiLiveSessionLifecycle.observeGummyFrame(now: Long, generation: Long, frame: GeminiLiveServerFrame) {
    reduce(now, GeminiLiveLifecycleEvent.Frame(GeminiLiveLifecycleFrame(
        generation = generation,
        contentCount = frame.contentCount,
        turnComplete = frame.turnComplete,
        generationComplete = frame.generationComplete,
        interrupted = frame.interrupted,
    )))
}

/** Serializes retirement with every externally visible effect of its owner. */
internal class TranslationGummyOwner {
    private var generation = 0L

    @Synchronized fun replace(): Long {
        generation = Math.addExact(generation, 1L)
        return generation
    }

    @Synchronized fun isCurrent(owner: Long): Boolean = owner == generation

    @Synchronized fun <T> ifCurrent(owner: Long, action: () -> T): T? =
        if (owner == generation) action() else null
}
