package dev.screengoated.toolbox.mobile.shared.live

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withTimeoutOrNull
import okhttp3.OkHttpClient
import java.io.Closeable
import java.io.IOException
import java.util.ArrayDeque
import java.util.concurrent.atomic.AtomicBoolean

internal data class GeminiLiveHandshakeTimeouts(
    val openMs: Long = 10_000L,
    val setupMs: Long = 15_000L,
) {
    init {
        require(openMs > 0L) { "openMs must be positive" }
        require(setupMs > 0L) { "setupMs must be positive" }
    }
}

internal enum class GeminiLiveWireFormat {
    TEXT,
    BINARY,
}

internal enum class GeminiLiveSessionPhase {
    OPENING,
    AWAITING_SETUP,
    ACTIVE,
    CLOSING,
    CLOSED,
}

internal sealed interface GeminiLiveSessionFailure {
    data object OpenTimedOut : GeminiLiveSessionFailure
    data object SetupTimedOut : GeminiLiveSessionFailure
    data object SetupSendRejected : GeminiLiveSessionFailure
    data object ActiveSendRejected : GeminiLiveSessionFailure

    data class Server(
        val message: String,
        val retryable: Boolean = false,
    ) : GeminiLiveSessionFailure

    data class Transport(
        val cause: Throwable,
    ) : GeminiLiveSessionFailure

    data class ClosedBeforeReady(
        val code: Int?,
        val reason: String?,
    ) : GeminiLiveSessionFailure
}

internal class GeminiLiveSessionException(
    val failure: GeminiLiveSessionFailure,
) : IOException(failure.describe(), failure.transportCause())

internal sealed interface GeminiLiveReceiveResult {
    data class Frame(
        val frame: GeminiLiveServerFrame,
        val wireFormat: GeminiLiveWireFormat,
    ) : GeminiLiveReceiveResult

    data class Unparsed(
        val payload: String,
        val wireFormat: GeminiLiveWireFormat,
    ) : GeminiLiveReceiveResult

    data object TimedOut : GeminiLiveReceiveResult

    data class Closed(
        val code: Int?,
        val reason: String?,
    ) : GeminiLiveReceiveResult

    data class Failed(
        val failure: GeminiLiveSessionFailure,
    ) : GeminiLiveReceiveResult
}

/** An opened Gemini Live transport that has not sent setup or application content. */
internal interface GeminiLiveConnectedSession : Closeable {
    val phase: GeminiLiveSessionPhase

    /** Consumes the setup capability once and transfers transport ownership to a ready session. */
    suspend fun activate(
        setupPayload: String,
        timeoutMs: Long = 15_000L,
    ): GeminiLiveReadySession

    override fun close()
}

/**
 * An already-opened, setup-acknowledged Gemini Live session.
 *
 * Reconnect, turn completion, interruption, and retry policy deliberately remain with the
 * feature adapter. This type owns only transport state and terminal cleanup.
 */
internal interface GeminiLiveReadySession : Closeable {
    val phase: GeminiLiveSessionPhase

    fun trySend(payload: String): Boolean

    suspend fun receive(timeoutMs: Long? = null): GeminiLiveReceiveResult

    override fun close()
}

internal suspend fun openGeminiLiveConnectedSession(
    httpClient: OkHttpClient,
    apiKey: String,
    openTimeoutMs: Long = 10_000L,
): GeminiLiveConnectedSession = openGeminiLiveConnectedSession(
    connector = OkHttpGeminiLiveSocketConnector(httpClient, geminiLiveWebSocketRequest(apiKey)),
    openTimeoutMs = openTimeoutMs,
)

internal suspend fun openGeminiLiveConnectedSession(
    connector: GeminiLiveSocketConnector,
    openTimeoutMs: Long = 10_000L,
): GeminiLiveConnectedSession {
    require(openTimeoutMs > 0L) { "openTimeoutMs must be positive" }
    val core = GeminiLiveSessionCore()
    val pendingResults = mutableListOf<GeminiLiveReceiveResult>()
    try {
        val socket = connector.connect(core.listener)
        core.attach(socket)
        awaitSocketOpen(core, openTimeoutMs, pendingResults)
        return DefaultGeminiLiveConnectedSession(core, pendingResults)
    } catch (cancelled: CancellationException) {
        core.cancel()
        throw cancelled
    } catch (error: GeminiLiveSessionException) {
        core.fail(error.failure)
        throw error
    } catch (error: Throwable) {
        val failure = GeminiLiveSessionFailure.Transport(error)
        core.fail(failure)
        throw GeminiLiveSessionException(failure)
    }
}

internal suspend fun openGeminiLiveReadySession(
    httpClient: OkHttpClient,
    apiKey: String,
    setupPayload: String,
    timeouts: GeminiLiveHandshakeTimeouts = GeminiLiveHandshakeTimeouts(),
): GeminiLiveReadySession = openGeminiLiveConnectedSession(
    httpClient = httpClient,
    apiKey = apiKey,
    openTimeoutMs = timeouts.openMs,
).activate(setupPayload, timeouts.setupMs)

internal suspend fun openGeminiLiveReadySession(
    connector: GeminiLiveSocketConnector,
    setupPayload: String,
    timeouts: GeminiLiveHandshakeTimeouts = GeminiLiveHandshakeTimeouts(),
): GeminiLiveReadySession = openGeminiLiveConnectedSession(
    connector = connector,
    openTimeoutMs = timeouts.openMs,
).activate(setupPayload, timeouts.setupMs)

private class DefaultGeminiLiveConnectedSession(
    private val core: GeminiLiveSessionCore,
    private val pendingResults: MutableList<GeminiLiveReceiveResult>,
) : GeminiLiveConnectedSession {
    private val activationStarted = AtomicBoolean(false)
    private val ownershipTransferred = AtomicBoolean(false)

    override val phase: GeminiLiveSessionPhase
        get() = core.phase

    override suspend fun activate(
        setupPayload: String,
        timeoutMs: Long,
    ): GeminiLiveReadySession {
        require(timeoutMs > 0L) { "timeoutMs must be positive" }
        check(activationStarted.compareAndSet(false, true)) {
            "Gemini Live connected session was already activated"
        }
        try {
            if (!core.sendSetup(setupPayload)) {
                throw GeminiLiveSessionException(
                    core.failureBeforeReady() ?: GeminiLiveSessionFailure.SetupSendRejected,
                )
            }
            awaitSetupAcknowledgement(core, timeoutMs, pendingResults)
            if (!core.activate()) {
                throw GeminiLiveSessionException(
                    core.failureBeforeReady() ?: GeminiLiveSessionFailure.ClosedBeforeReady(null, null),
                )
            }
            ownershipTransferred.set(true)
            return DefaultGeminiLiveReadySession(core, pendingResults)
        } catch (cancelled: CancellationException) {
            core.cancel()
            throw cancelled
        } catch (error: GeminiLiveSessionException) {
            core.fail(error.failure)
            throw error
        } catch (error: Throwable) {
            val failure = GeminiLiveSessionFailure.Transport(error)
            core.fail(failure)
            throw GeminiLiveSessionException(failure)
        }
    }

    override fun close() {
        if (!ownershipTransferred.get()) {
            core.closeLocally()
        }
    }
}

private suspend fun awaitSocketOpen(
    core: GeminiLiveSessionCore,
    timeoutMs: Long,
    pendingResults: MutableList<GeminiLiveReceiveResult>,
) {
    val opened = withTimeoutOrNull(timeoutMs) {
        while (true) {
            when (val event = core.receiveEvent()) {
                GeminiLiveCoreEvent.Opened -> return@withTimeoutOrNull true
                is GeminiLiveCoreEvent.Failed -> throw GeminiLiveSessionException(event.failure)
                is GeminiLiveCoreEvent.Closed -> throw GeminiLiveSessionException(
                    GeminiLiveSessionFailure.ClosedBeforeReady(event.code, event.reason),
                )
                is GeminiLiveCoreEvent.Frame,
                is GeminiLiveCoreEvent.Unparsed,
                -> event.toObservationResult()?.let(pendingResults::add)
                null -> throw GeminiLiveSessionException(
                    core.failureBeforeReady() ?: GeminiLiveSessionFailure.ClosedBeforeReady(null, null),
                )
            }
        }
    }
    if (opened != true) {
        val failure = GeminiLiveSessionFailure.OpenTimedOut
        core.fail(failure)
        throw GeminiLiveSessionException(failure)
    }
}

private suspend fun awaitSetupAcknowledgement(
    core: GeminiLiveSessionCore,
    timeoutMs: Long,
    pendingResults: MutableList<GeminiLiveReceiveResult>,
) {
    val acknowledged = withTimeoutOrNull(timeoutMs) {
        while (true) {
            when (val event = core.receiveEvent()) {
                is GeminiLiveCoreEvent.Frame -> {
                    if (!event.frame.setupComplete || event.frame.hasPostSetupObservation) {
                        pendingResults.add(requireNotNull(event.toObservationResult()))
                    }
                    if (event.frame.setupComplete) {
                        return@withTimeoutOrNull true
                    }
                }
                is GeminiLiveCoreEvent.Failed -> throw GeminiLiveSessionException(event.failure)
                is GeminiLiveCoreEvent.Closed -> throw GeminiLiveSessionException(
                    GeminiLiveSessionFailure.ClosedBeforeReady(event.code, event.reason),
                )
                is GeminiLiveCoreEvent.Unparsed -> {
                    pendingResults.add(requireNotNull(event.toObservationResult()))
                }
                GeminiLiveCoreEvent.Opened -> Unit
                null -> throw GeminiLiveSessionException(
                    core.failureBeforeReady() ?: GeminiLiveSessionFailure.ClosedBeforeReady(null, null),
                )
            }
        }
    }
    if (acknowledged != true) {
        val failure = GeminiLiveSessionFailure.SetupTimedOut
        core.fail(failure)
        throw GeminiLiveSessionException(failure)
    }
}

private class DefaultGeminiLiveReadySession(
    private val core: GeminiLiveSessionCore,
    pendingResults: List<GeminiLiveReceiveResult>,
) : GeminiLiveReadySession {
    private val pendingResults = ArrayDeque(pendingResults)

    override val phase: GeminiLiveSessionPhase
        get() = core.phase

    override fun trySend(payload: String): Boolean = core.sendActive(payload)

    override suspend fun receive(timeoutMs: Long?): GeminiLiveReceiveResult {
        require(timeoutMs == null || timeoutMs >= 0L) { "timeoutMs must be non-negative" }
        pendingResults.pollFirst()?.let { return it }
        return try {
            val event = if (timeoutMs == null) {
                core.receiveEvent()
            } else {
                val received = withTimeoutOrNull(timeoutMs) {
                    TimedCoreEvent(core.receiveEvent())
                } ?: return GeminiLiveReceiveResult.TimedOut
                received.event
            }
            when (event) {
                is GeminiLiveCoreEvent.Frame -> GeminiLiveReceiveResult.Frame(event.frame, event.wireFormat)
                is GeminiLiveCoreEvent.Unparsed -> GeminiLiveReceiveResult.Unparsed(event.payload, event.wireFormat)
                is GeminiLiveCoreEvent.Closed -> GeminiLiveReceiveResult.Closed(event.code, event.reason)
                is GeminiLiveCoreEvent.Failed -> GeminiLiveReceiveResult.Failed(event.failure)
                GeminiLiveCoreEvent.Opened -> receive(timeoutMs)
                null -> core.terminalReceiveResult()
            }
        } catch (cancelled: CancellationException) {
            core.cancel()
            throw cancelled
        }
    }

    override fun close() = core.closeLocally()
}

private fun GeminiLiveCoreEvent.toObservationResult(): GeminiLiveReceiveResult? = when (this) {
    is GeminiLiveCoreEvent.Frame -> GeminiLiveReceiveResult.Frame(frame, wireFormat)
    is GeminiLiveCoreEvent.Unparsed -> GeminiLiveReceiveResult.Unparsed(payload, wireFormat)
    GeminiLiveCoreEvent.Opened,
    is GeminiLiveCoreEvent.Closed,
    is GeminiLiveCoreEvent.Failed,
    -> null
}

private data class TimedCoreEvent(
    val event: GeminiLiveCoreEvent?,
)

private fun GeminiLiveSessionFailure.describe(): String = when (this) {
    GeminiLiveSessionFailure.OpenTimedOut -> "Gemini Live websocket open timed out."
    GeminiLiveSessionFailure.SetupTimedOut -> "Gemini Live setup timed out."
    GeminiLiveSessionFailure.SetupSendRejected -> "Gemini Live setup payload was rejected."
    GeminiLiveSessionFailure.ActiveSendRejected -> "Gemini Live payload was rejected."
    is GeminiLiveSessionFailure.Server -> message
    is GeminiLiveSessionFailure.Transport -> cause.message ?: "Gemini Live transport failed."
    is GeminiLiveSessionFailure.ClosedBeforeReady ->
        "Gemini Live websocket closed before setup completed (code=${code ?: "unknown"}, reason=${reason.orEmpty()})."
}

private fun GeminiLiveSessionFailure.transportCause(): Throwable? =
    (this as? GeminiLiveSessionFailure.Transport)?.cause
