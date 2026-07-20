package dev.screengoated.toolbox.mobile.shared.live

import kotlinx.coroutines.CancellationException

internal data class GeminiLiveLifecycleConnection(
    val generation: Long,
    val session: GeminiLiveReadySession,
)

private data class PendingGeminiLiveConnection(
    val generation: Long,
    val session: GeminiLiveConnectedSession,
)

/** Executes transport effects from the shared clock-injected lifecycle reducer. */
internal class GeminiLiveLifecycleAdapter(
    private val policy: GeminiLiveLifecyclePolicy,
    private val clockMs: () -> Long,
    private val openConnectedSession: suspend () -> GeminiLiveConnectedSession,
    private val setupPayload: () -> String,
    private val onEffect: (GeminiLiveLifecycleEffect) -> Unit = {},
) {
    private val lifecycle = GeminiLiveSessionLifecycle(policy = policy)
    private var pendingConnection: PendingGeminiLiveConnection? = null
    private var connection: GeminiLiveLifecycleConnection? = null

    val state: GeminiLiveLifecycleState
        get() = lifecycle.state

    val activeConnection: GeminiLiveLifecycleConnection?
        get() = connection

    suspend fun ensureReady(): GeminiLiveLifecycleConnection? {
        val event = if (state.phase == GeminiLiveLifecyclePhase.IDLE) {
            GeminiLiveLifecycleEvent.Start
        } else {
            GeminiLiveLifecycleEvent.Tick
        }
        applyEffects(lifecycle.reduce(clockMs(), event))
        return connection
    }

    fun inputSent(chunks: Long = 1) {
        lifecycle.reduce(clockMs(), GeminiLiveLifecycleEvent.InputSent(chunks))
    }

    fun inputActivity() {
        lifecycle.reduce(clockMs(), GeminiLiveLifecycleEvent.InputActivity)
    }

    fun updateWorkState(
        pendingWorkCount: Long,
        bufferedInputCount: Long,
        userSpeaking: Boolean,
    ) {
        lifecycle.reduce(
            clockMs(),
            GeminiLiveLifecycleEvent.WorkState(
                pendingWorkCount = pendingWorkCount,
                bufferedInputCount = bufferedInputCount,
                userSpeaking = userSpeaking,
            ),
        )
    }

    suspend fun observeFrame(frame: GeminiLiveLifecycleFrame): List<GeminiLiveLifecycleEffect> =
        applyEffects(lifecycle.reduce(clockMs(), GeminiLiveLifecycleEvent.Frame(frame)))

    suspend fun transportFailed(
        generation: Long,
        retryable: Boolean = true,
    ): List<GeminiLiveLifecycleEffect> = applyEffects(
        lifecycle.reduce(
            clockMs(),
            GeminiLiveLifecycleEvent.TransportFailure(generation, retryable),
        ),
    )

    suspend fun serverError(
        generation: Long,
        retryable: Boolean,
        kind: String = "server",
    ): List<GeminiLiveLifecycleEffect> = observeFrame(
        GeminiLiveLifecycleFrame(
            generation = generation,
            error = GeminiLiveClassifiedError(kind = kind, retryable = retryable),
        ),
    )

    suspend fun tick(): List<GeminiLiveLifecycleEffect> =
        applyEffects(lifecycle.reduce(clockMs(), GeminiLiveLifecycleEvent.Tick))

    suspend fun cancel() {
        applyEffects(lifecycle.reduce(clockMs(), GeminiLiveLifecycleEvent.Cancel))
    }

    private suspend fun applyEffects(
        effects: List<GeminiLiveLifecycleEffect>,
    ): List<GeminiLiveLifecycleEffect> {
        val featureEffects = mutableListOf<GeminiLiveLifecycleEffect>()
        for (effect in effects) {
            onEffect(effect)
            when (effect) {
                is GeminiLiveLifecycleEffect.OpenSocket -> open(effect.generation)
                is GeminiLiveLifecycleEffect.SendSetup -> activate(effect.generation)
                is GeminiLiveLifecycleEffect.CloseSocket -> close(effect.generation)
                is GeminiLiveLifecycleEffect.ScheduleReconnect,
                is GeminiLiveLifecycleEffect.ReportFailure,
                GeminiLiveLifecycleEffect.CancelSession,
                -> Unit
                else -> featureEffects += effect
            }
        }
        return featureEffects
    }

    private suspend fun open(generation: Long) {
        val opened = try {
            openConnectedSession()
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: GeminiLiveSessionException) {
            handleOpenFailure(generation, error)
            return
        } catch (_: Throwable) {
            transportFailed(generation)
            return
        }
        val setupEffects = lifecycle.reduce(
            clockMs(),
            GeminiLiveLifecycleEvent.SocketOpened(generation),
        )
        if (setupEffects != listOf(GeminiLiveLifecycleEffect.SendSetup(generation))) {
            opened.close()
            return
        }
        pendingConnection = PendingGeminiLiveConnection(generation, opened)
        applyEffects(setupEffects)
    }

    private suspend fun handleOpenFailure(
        generation: Long,
        error: GeminiLiveSessionException,
    ) {
        val server = error.failure as? GeminiLiveSessionFailure.Server
        if (server != null) {
            serverError(generation, retryable = server.retryable)
        } else {
            transportFailed(generation)
        }
    }

    private suspend fun activate(generation: Long) {
        val pending = pendingConnection
        if (pending == null || pending.generation != generation) return
        val ready = try {
            pending.session.activate(setupPayload(), policy.setupTimeoutMs)
        } catch (cancelled: CancellationException) {
            pending.session.close()
            if (pendingConnection === pending) pendingConnection = null
            throw cancelled
        } catch (error: GeminiLiveSessionException) {
            handleActivationFailure(generation, error)
            return
        } catch (_: Throwable) {
            transportFailed(generation)
            return
        }

        if (state.generation != generation || state.phase != GeminiLiveLifecyclePhase.AWAITING_SETUP) {
            ready.close()
            if (pendingConnection === pending) pendingConnection = null
            return
        }
        pendingConnection = null
        connection = GeminiLiveLifecycleConnection(generation, ready)
        applyEffects(
            lifecycle.reduce(
                clockMs(),
                GeminiLiveLifecycleEvent.Frame(
                    GeminiLiveLifecycleFrame(generation = generation, setupComplete = true),
                ),
            ),
        )
    }

    private suspend fun handleActivationFailure(
        generation: Long,
        error: GeminiLiveSessionException,
    ) {
        when (val failure = error.failure) {
            GeminiLiveSessionFailure.SetupTimedOut -> {
                val timeoutAt = maxOf(
                    clockMs(),
                    state.setupDeadlineMs ?: clockMs(),
                )
                applyEffects(lifecycle.reduce(timeoutAt, GeminiLiveLifecycleEvent.Tick))
            }
            is GeminiLiveSessionFailure.Server ->
                serverError(generation, retryable = failure.retryable)
            else -> transportFailed(generation)
        }
    }

    private fun close(generation: Long) {
        pendingConnection?.takeIf { it.generation == generation }?.let { pending ->
            pending.session.close()
            pendingConnection = null
        }
        connection?.takeIf { it.generation == generation }?.let { current ->
            current.session.close()
            connection = null
        }
    }
}
