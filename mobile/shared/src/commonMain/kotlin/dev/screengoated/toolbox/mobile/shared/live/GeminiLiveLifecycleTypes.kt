package dev.screengoated.toolbox.mobile.shared.live

/** Transport-neutral Gemini Live session mode. */
enum class GeminiLiveSessionKind {
    FINITE_REQUEST,
    CONTINUOUS_STREAM,
    SEGMENTED_STREAM,
    AGENT_SESSION,
}

enum class GeminiLiveLifecyclePhase {
    IDLE,
    CONNECTING,
    AWAITING_SETUP,
    ACTIVE,
    BACKING_OFF,
    COMPLETED,
    CANCELLED,
    FAILED,
    ;

    val isTerminal: Boolean
        get() = this == COMPLETED || this == CANCELLED || this == FAILED
}

data class GeminiLiveLifecyclePolicy(
    val kind: GeminiLiveSessionKind,
    val setupTimeoutMs: Long,
    val firstResponseTimeoutMs: Long? = null,
    val contentIdleMs: Long? = null,
    val hardResponseTimeoutMs: Long? = null,
    val serverIdleTimeoutMs: Long? = null,
    val serverIdleMinInputChunks: Long = 0,
    val rotateAfterMs: Long? = null,
    val rotationQuietMs: Long = 0,
    val reconnectEnabled: Boolean,
    val maxReconnectAttempts: Int? = null,
    val completeOnTurn: Boolean = true,
    val completeOnGeneration: Boolean = true,
) {
    init {
        require(setupTimeoutMs >= 0) { "setupTimeoutMs must be non-negative" }
        require(firstResponseTimeoutMs?.let { it >= 0 } != false) {
            "firstResponseTimeoutMs must be non-negative"
        }
        require(contentIdleMs?.let { it >= 0 } != false) {
            "contentIdleMs must be non-negative"
        }
        require(hardResponseTimeoutMs?.let { it >= 0 } != false) {
            "hardResponseTimeoutMs must be non-negative"
        }
        require(serverIdleTimeoutMs?.let { it >= 0 } != false) {
            "serverIdleTimeoutMs must be non-negative"
        }
        require(serverIdleMinInputChunks >= 0) {
            "serverIdleMinInputChunks must be non-negative"
        }
        require(rotateAfterMs?.let { it >= 0 } != false) {
            "rotateAfterMs must be non-negative"
        }
        require(rotationQuietMs >= 0) { "rotationQuietMs must be non-negative" }
        require(maxReconnectAttempts?.let { it >= 0 } != false) {
            "maxReconnectAttempts must be non-negative"
        }
    }

    companion object {
        fun finite(): GeminiLiveLifecyclePolicy = GeminiLiveLifecyclePolicy(
            kind = GeminiLiveSessionKind.FINITE_REQUEST,
            setupTimeoutMs = 15_000,
            firstResponseTimeoutMs = 20_000,
            contentIdleMs = 1_200,
            hardResponseTimeoutMs = 90_000,
            reconnectEnabled = false,
        )

        fun continuous(): GeminiLiveLifecyclePolicy = GeminiLiveLifecyclePolicy(
            kind = GeminiLiveSessionKind.CONTINUOUS_STREAM,
            setupTimeoutMs = 15_000,
            serverIdleTimeoutMs = 15_000,
            serverIdleMinInputChunks = 100,
            rotateAfterMs = 720_000,
            rotationQuietMs = 3_000,
            reconnectEnabled = true,
        )

        fun agent(): GeminiLiveLifecyclePolicy = continuous().copy(
            kind = GeminiLiveSessionKind.AGENT_SESSION,
            serverIdleTimeoutMs = null,
            rotateAfterMs = null,
            maxReconnectAttempts = 6,
        )
    }
}

data class GeminiLiveBackoffPolicy(
    val baseMs: Long = 250,
    val exponentCap: Int = 5,
    val jitterMinMs: Long = 20,
    val jitterSeed: Long = 7,
    val jitterStep: Long = 53,
    val jitterSpan: Long = 180,
    val maxMs: Long = 6_000,
) {
    init {
        require(baseMs >= 0) { "baseMs must be non-negative" }
        require(exponentCap >= 0) { "exponentCap must be non-negative" }
        require(jitterMinMs >= 0) { "jitterMinMs must be non-negative" }
        require(jitterSeed >= 0) { "jitterSeed must be non-negative" }
        require(jitterStep >= 0) { "jitterStep must be non-negative" }
        require(jitterSpan >= 0) { "jitterSpan must be non-negative" }
        require(maxMs >= 0) { "maxMs must be non-negative" }
    }

    fun delayMs(attempt: Int): Long {
        require(attempt >= 0) { "attempt must be non-negative" }
        val exponent = minOf(attempt, exponentCap.coerceAtMost(62))
        val base = saturatingMultiply(baseMs, 1L shl exponent)
        val jitter = if (jitterSpan <= 0) {
            jitterMinMs
        } else {
            val steppedSeed = saturatingAdd(
                jitterSeed,
                saturatingMultiply(attempt.toLong(), jitterStep),
            )
            saturatingAdd(jitterMinMs, steppedSeed.mod(jitterSpan))
        }
        return minOf(saturatingAdd(base, jitter), maxMs)
    }
}

data class GeminiLiveLifecycleState(
    val phase: GeminiLiveLifecyclePhase = GeminiLiveLifecyclePhase.IDLE,
    val generation: Long = 0,
    val reconnectAttempt: Int = 0,
    val socketOpen: Boolean = false,
    val connectionStartedAtMs: Long? = null,
    val connectedAtMs: Long? = null,
    val setupDeadlineMs: Long? = null,
    val firstResponseDeadlineMs: Long? = null,
    val contentIdleDeadlineMs: Long? = null,
    val hardResponseDeadlineMs: Long? = null,
    val reconnectDeadlineMs: Long? = null,
    val goAwayDeadlineMs: Long? = null,
    val hasOutput: Boolean = false,
    val inputChunksSinceServerActivity: Long = 0,
    val lastInputActivityMs: Long? = null,
    val lastServerActivityMs: Long? = null,
    val pendingWorkCount: Long = 0,
    val bufferedInputCount: Long = 0,
    val userSpeaking: Boolean = false,
    val pendingToolIds: List<String> = emptyList(),
)

sealed interface GeminiLiveLifecycleEvent {
    data object Start : GeminiLiveLifecycleEvent

    data class SocketOpened(val generation: Long) : GeminiLiveLifecycleEvent

    data class Frame(val frame: GeminiLiveLifecycleFrame) : GeminiLiveLifecycleEvent

    data class TransportFailure(
        val generation: Long,
        val retryable: Boolean,
    ) : GeminiLiveLifecycleEvent

    data class InputSent(val chunks: Long) : GeminiLiveLifecycleEvent {
        init {
            require(chunks >= 0) { "chunks must be non-negative" }
        }
    }

    data object InputActivity : GeminiLiveLifecycleEvent

    data class WorkState(
        val pendingWorkCount: Long,
        val bufferedInputCount: Long,
        val userSpeaking: Boolean,
    ) : GeminiLiveLifecycleEvent {
        init {
            require(pendingWorkCount >= 0) { "pendingWorkCount must be non-negative" }
            require(bufferedInputCount >= 0) { "bufferedInputCount must be non-negative" }
        }
    }

    data object Tick : GeminiLiveLifecycleEvent

    data object Cancel : GeminiLiveLifecycleEvent
}

data class GeminiLiveLifecycleFrame(
    val generation: Long,
    val contentCount: Int = 0,
    val setupComplete: Boolean = false,
    val turnComplete: Boolean = false,
    val generationComplete: Boolean = false,
    val interrupted: Boolean = false,
    val goAwayTimeLeftMs: Long? = null,
    val toolCallIds: List<String> = emptyList(),
    val toolCancellationIds: List<String> = emptyList(),
    val error: GeminiLiveClassifiedError? = null,
) {
    init {
        require(contentCount >= 0) { "contentCount must be non-negative" }
        require(goAwayTimeLeftMs?.let { it >= 0 } != false) {
            "goAwayTimeLeftMs must be non-negative"
        }
    }
}

data class GeminiLiveClassifiedError(
    val kind: String,
    val retryable: Boolean,
)

enum class GeminiLiveCompletionReason {
    TURN_COMPLETE,
    GENERATION_COMPLETE,
    CONTENT_IDLE,
}

enum class GeminiLiveReconnectReason(val fixtureName: String) {
    SETUP_TIMEOUT("setupTimeout"),
    TRANSPORT_FAILURE("transportFailure"),
    SERVER_ERROR("serverError"),
    SERVER_IDLE("serverIdle"),
    PROACTIVE_ROTATION("proactiveRotation"),
    GO_AWAY_SAFE_GAP("goAwaySafeGap"),
    GO_AWAY_DEADLINE("goAwayDeadline"),
}

sealed interface GeminiLiveLifecycleEffect {
    data class OpenSocket(val generation: Long) : GeminiLiveLifecycleEffect

    data class SendSetup(val generation: Long) : GeminiLiveLifecycleEffect

    data class DeliverContent(val count: Int) : GeminiLiveLifecycleEffect

    data class FinalizeResponse(
        val reason: GeminiLiveCompletionReason,
    ) : GeminiLiveLifecycleEffect

    data object FinalizeGeneration : GeminiLiveLifecycleEffect

    data object FinalizeTurn : GeminiLiveLifecycleEffect

    data object StopPlayback : GeminiLiveLifecycleEffect

    data object DiscardQueuedOutput : GeminiLiveLifecycleEffect

    data object FinalizeInterruptedGeneration : GeminiLiveLifecycleEffect

    data class DispatchTools(val ids: List<String>) : GeminiLiveLifecycleEffect

    data class CancelTools(val ids: List<String>) : GeminiLiveLifecycleEffect

    data class CloseSocket(val generation: Long) : GeminiLiveLifecycleEffect

    data class ScheduleReconnect(
        val generation: Long,
        val attempt: Int,
        val delayMs: Long,
        val reason: GeminiLiveReconnectReason,
    ) : GeminiLiveLifecycleEffect

    data class ReportFailure(val reason: String) : GeminiLiveLifecycleEffect

    data object CancelSession : GeminiLiveLifecycleEffect
}

internal fun saturatingAdd(left: Long, right: Long): Long {
    if (right <= 0) return left
    return if (left > Long.MAX_VALUE - right) Long.MAX_VALUE else left + right
}

private fun saturatingMultiply(left: Long, right: Long): Long {
    if (left <= 0 || right <= 0) return 0
    return if (left > Long.MAX_VALUE / right) Long.MAX_VALUE else left * right
}
