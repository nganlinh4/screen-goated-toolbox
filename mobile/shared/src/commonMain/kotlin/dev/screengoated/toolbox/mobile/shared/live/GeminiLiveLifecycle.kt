package dev.screengoated.toolbox.mobile.shared.live

/**
 * Deterministic lifecycle reducer. The caller supplies monotonic time and executes returned effects.
 */
class GeminiLiveSessionLifecycle(
    private val policy: GeminiLiveLifecyclePolicy,
    private val backoff: GeminiLiveBackoffPolicy = GeminiLiveBackoffPolicy(),
) {
    var state: GeminiLiveLifecycleState = GeminiLiveLifecycleState()
        private set

    fun reduce(
        atMs: Long,
        event: GeminiLiveLifecycleEvent,
    ): List<GeminiLiveLifecycleEffect> {
        if (state.phase.isTerminal) return emptyList()
        require(atMs >= 0) { "atMs must be non-negative" }
        return when (event) {
            GeminiLiveLifecycleEvent.Start -> start(atMs)
            is GeminiLiveLifecycleEvent.SocketOpened -> socketOpened(atMs, event.generation)
            is GeminiLiveLifecycleEvent.Frame -> frame(atMs, event.frame)
            is GeminiLiveLifecycleEvent.TransportFailure -> {
                transportFailure(atMs, event.generation, event.retryable)
            }
            is GeminiLiveLifecycleEvent.InputSent -> {
                if (state.phase == GeminiLiveLifecyclePhase.ACTIVE) {
                    state = state.copy(
                        inputChunksSinceServerActivity = saturatingAdd(
                            state.inputChunksSinceServerActivity,
                            event.chunks,
                        ),
                    )
                }
                emptyList()
            }
            GeminiLiveLifecycleEvent.InputActivity -> {
                if (state.phase == GeminiLiveLifecyclePhase.ACTIVE) {
                    state = state.copy(lastInputActivityMs = atMs)
                }
                emptyList()
            }
            is GeminiLiveLifecycleEvent.WorkState -> {
                state = state.copy(
                    pendingWorkCount = event.pendingWorkCount,
                    bufferedInputCount = event.bufferedInputCount,
                    userSpeaking = event.userSpeaking,
                )
                emptyList()
            }
            GeminiLiveLifecycleEvent.Tick -> tick(atMs)
            GeminiLiveLifecycleEvent.Cancel -> cancel()
        }
    }

    private fun start(atMs: Long): List<GeminiLiveLifecycleEffect> {
        return when {
            state.phase == GeminiLiveLifecyclePhase.IDLE -> {
                state = state.copy(
                    phase = GeminiLiveLifecyclePhase.CONNECTING,
                    generation = 1,
                    connectionStartedAtMs = atMs,
                )
                listOf(GeminiLiveLifecycleEffect.OpenSocket(state.generation))
            }
            state.phase == GeminiLiveLifecyclePhase.BACKING_OFF &&
                deadlineReached(atMs, state.reconnectDeadlineMs) -> {
                state = state.copy(
                    phase = GeminiLiveLifecyclePhase.CONNECTING,
                    connectionStartedAtMs = atMs,
                    reconnectDeadlineMs = null,
                )
                listOf(GeminiLiveLifecycleEffect.OpenSocket(state.generation))
            }
            else -> emptyList()
        }
    }

    private fun socketOpened(
        atMs: Long,
        generation: Long,
    ): List<GeminiLiveLifecycleEffect> {
        if (!accepts(generation) || state.phase != GeminiLiveLifecyclePhase.CONNECTING) {
            return emptyList()
        }
        state = state.copy(
            phase = GeminiLiveLifecyclePhase.AWAITING_SETUP,
            socketOpen = true,
            connectedAtMs = atMs,
            lastServerActivityMs = atMs,
            lastInputActivityMs = atMs,
            setupDeadlineMs = saturatingAdd(atMs, policy.setupTimeoutMs),
        )
        return listOf(GeminiLiveLifecycleEffect.SendSetup(generation))
    }

    private fun frame(
        atMs: Long,
        frame: GeminiLiveLifecycleFrame,
    ): List<GeminiLiveLifecycleEffect> {
        if (!accepts(frame.generation)) return emptyList()
        frame.error?.let { error ->
            return if (error.retryable) {
                scheduleReconnect(atMs, GeminiLiveReconnectReason.SERVER_ERROR)
            } else {
                fail(error.kind)
            }
        }

        val effects = mutableListOf<GeminiLiveLifecycleEffect>()
        if (frame.setupComplete && state.phase == GeminiLiveLifecyclePhase.AWAITING_SETUP) {
            state = state.copy(
                phase = GeminiLiveLifecyclePhase.ACTIVE,
                setupDeadlineMs = null,
                lastServerActivityMs = atMs,
                inputChunksSinceServerActivity = 0,
                firstResponseDeadlineMs = if (policy.kind == GeminiLiveSessionKind.FINITE_REQUEST) {
                    policy.firstResponseTimeoutMs?.let { saturatingAdd(atMs, it) }
                } else {
                    state.firstResponseDeadlineMs
                },
                hardResponseDeadlineMs = if (policy.kind == GeminiLiveSessionKind.FINITE_REQUEST) {
                    policy.hardResponseTimeoutMs?.let { saturatingAdd(atMs, it) }
                } else {
                    state.hardResponseDeadlineMs
                },
            )
        }
        if (state.phase != GeminiLiveLifecyclePhase.ACTIVE) return effects

        val meaningful = frame.contentCount > 0 ||
            frame.turnComplete ||
            frame.generationComplete ||
            frame.interrupted ||
            frame.toolCallIds.isNotEmpty() ||
            frame.toolCancellationIds.isNotEmpty()
        if (meaningful) {
            state = state.copy(
                lastServerActivityMs = atMs,
                inputChunksSinceServerActivity = 0,
                reconnectAttempt = 0,
            )
        }
        if (frame.contentCount > 0) {
            state = state.copy(
                hasOutput = true,
                firstResponseDeadlineMs = null,
                contentIdleDeadlineMs = policy.contentIdleMs?.let { saturatingAdd(atMs, it) },
            )
            effects += GeminiLiveLifecycleEffect.DeliverContent(frame.contentCount)
        }
        if (frame.toolCallIds.isNotEmpty()) {
            val added = frame.toolCallIds.filterNot(state.pendingToolIds::contains).distinct()
            if (added.isNotEmpty()) {
                state = state.copy(pendingToolIds = state.pendingToolIds + added)
                effects += GeminiLiveLifecycleEffect.DispatchTools(added)
            }
        }
        if (frame.interrupted) {
            effects += GeminiLiveLifecycleEffect.StopPlayback
            effects += GeminiLiveLifecycleEffect.DiscardQueuedOutput
            effects += GeminiLiveLifecycleEffect.FinalizeInterruptedGeneration
        }
        if (frame.toolCancellationIds.isNotEmpty()) {
            val remaining = state.pendingToolIds.toMutableList()
            val removed = buildList {
                frame.toolCancellationIds.forEach { id ->
                    val index = remaining.indexOf(id)
                    if (index >= 0) add(remaining.removeAt(index))
                }
            }
            if (removed.isNotEmpty()) {
                state = state.copy(pendingToolIds = remaining)
                effects += GeminiLiveLifecycleEffect.CancelTools(removed)
            }
        }

        if (frame.turnComplete && policy.completeOnTurn) {
            effects += complete(GeminiLiveCompletionReason.TURN_COMPLETE)
        }
        if (!state.phase.isTerminal && frame.generationComplete && policy.completeOnGeneration) {
            effects += complete(GeminiLiveCompletionReason.GENERATION_COMPLETE)
        }
        if (state.phase.isTerminal) return effects
        frame.goAwayTimeLeftMs?.let { timeLeftMs ->
            state = state.copy(goAwayDeadlineMs = saturatingAdd(atMs, timeLeftMs))
        }
        return effects
    }

    private fun complete(reason: GeminiLiveCompletionReason): List<GeminiLiveLifecycleEffect> {
        return when (policy.kind) {
            GeminiLiveSessionKind.FINITE_REQUEST,
            GeminiLiveSessionKind.SEGMENTED_STREAM,
            -> {
                state = clearDeadlines(state.copy(phase = GeminiLiveLifecyclePhase.COMPLETED))
                buildList {
                    add(GeminiLiveLifecycleEffect.FinalizeResponse(reason))
                    if (state.socketOpen) {
                        add(GeminiLiveLifecycleEffect.CloseSocket(state.generation))
                        state = state.copy(socketOpen = false)
                    }
                }
            }
            GeminiLiveSessionKind.CONTINUOUS_STREAM,
            GeminiLiveSessionKind.AGENT_SESSION,
            -> listOf(
                when (reason) {
                    GeminiLiveCompletionReason.TURN_COMPLETE -> {
                        GeminiLiveLifecycleEffect.FinalizeTurn
                    }
                    GeminiLiveCompletionReason.GENERATION_COMPLETE -> {
                        GeminiLiveLifecycleEffect.FinalizeGeneration
                    }
                    GeminiLiveCompletionReason.CONTENT_IDLE -> {
                        error("content idle is finite-request policy")
                    }
                },
            )
        }
    }

    private fun transportFailure(
        atMs: Long,
        generation: Long,
        retryable: Boolean,
    ): List<GeminiLiveLifecycleEffect> {
        if (!accepts(generation)) return emptyList()
        return if (retryable) {
            scheduleReconnect(atMs, GeminiLiveReconnectReason.TRANSPORT_FAILURE)
        } else {
            fail("transportFailure")
        }
    }

    private fun tick(atMs: Long): List<GeminiLiveLifecycleEffect> {
        if (state.phase == GeminiLiveLifecyclePhase.BACKING_OFF) return start(atMs)
        if (state.phase == GeminiLiveLifecyclePhase.ACTIVE) {
            state.goAwayDeadlineMs?.let { deadline ->
                if (atMs >= deadline) {
                    return scheduleReconnect(atMs, GeminiLiveReconnectReason.GO_AWAY_DEADLINE)
                }
                if (safeGap()) {
                    return scheduleReconnect(atMs, GeminiLiveReconnectReason.GO_AWAY_SAFE_GAP)
                }
            }
        }
        if (state.phase == GeminiLiveLifecyclePhase.AWAITING_SETUP &&
            deadlineReached(atMs, state.setupDeadlineMs)
        ) {
            return if (policy.reconnectEnabled) {
                scheduleReconnect(atMs, GeminiLiveReconnectReason.SETUP_TIMEOUT)
            } else {
                fail("setupTimeout")
            }
        }
        if (state.phase != GeminiLiveLifecyclePhase.ACTIVE) return emptyList()
        if (deadlineReached(atMs, state.hardResponseDeadlineMs)) {
            return fail("hardResponseTimeout")
        }
        if (deadlineReached(atMs, state.firstResponseDeadlineMs)) {
            return fail("firstResponseTimeout")
        }
        if (deadlineReached(atMs, state.contentIdleDeadlineMs)) {
            return complete(GeminiLiveCompletionReason.CONTENT_IDLE)
        }
        val idleTimeout = policy.serverIdleTimeoutMs
        if (idleTimeout != null &&
            state.inputChunksSinceServerActivity >= policy.serverIdleMinInputChunks &&
            state.lastServerActivityMs?.let { atMs - it >= idleTimeout } == true
        ) {
            return scheduleReconnect(atMs, GeminiLiveReconnectReason.SERVER_IDLE)
        }
        if (rotationDue(atMs)) {
            return scheduleReconnect(atMs, GeminiLiveReconnectReason.PROACTIVE_ROTATION)
        }
        return emptyList()
    }

    private fun scheduleReconnect(
        atMs: Long,
        reason: GeminiLiveReconnectReason,
    ): List<GeminiLiveLifecycleEffect> {
        if (!policy.reconnectEnabled ||
            policy.maxReconnectAttempts?.let { state.reconnectAttempt >= it } == true
        ) {
            return fail(reason.fixtureName)
        }
        val previousGeneration = state.generation
        val currentAttempt = state.reconnectAttempt
        val delayMs = backoff.delayMs(currentAttempt)
        state = clearSessionDeadlines(
            state.copy(
                phase = GeminiLiveLifecyclePhase.BACKING_OFF,
                generation = saturatingAdd(state.generation, 1),
                reconnectAttempt = if (currentAttempt == Int.MAX_VALUE) {
                    Int.MAX_VALUE
                } else {
                    currentAttempt + 1
                },
                socketOpen = false,
                connectedAtMs = null,
                reconnectDeadlineMs = saturatingAdd(atMs, delayMs),
            ),
        )
        return buildList {
            if (previousGeneration > 0) {
                add(GeminiLiveLifecycleEffect.CloseSocket(previousGeneration))
            }
            add(
                GeminiLiveLifecycleEffect.ScheduleReconnect(
                    generation = state.generation,
                    attempt = state.reconnectAttempt,
                    delayMs = delayMs,
                    reason = reason,
                ),
            )
        }
    }

    private fun fail(reason: String): List<GeminiLiveLifecycleEffect> {
        val generation = state.generation
        val wasOpen = state.socketOpen
        state = clearDeadlines(
            state.copy(
                phase = GeminiLiveLifecyclePhase.FAILED,
                socketOpen = false,
            ),
        )
        return buildList {
            if (wasOpen) add(GeminiLiveLifecycleEffect.CloseSocket(generation))
            add(GeminiLiveLifecycleEffect.ReportFailure(reason))
        }
    }

    private fun cancel(): List<GeminiLiveLifecycleEffect> {
        val generation = state.generation
        val wasOpen = state.socketOpen
        state = clearDeadlines(
            state.copy(
                phase = GeminiLiveLifecyclePhase.CANCELLED,
                socketOpen = false,
            ),
        )
        return buildList {
            if (wasOpen) add(GeminiLiveLifecycleEffect.CloseSocket(generation))
            add(GeminiLiveLifecycleEffect.CancelSession)
        }
    }

    private fun rotationDue(atMs: Long): Boolean {
        val rotateAfterMs = policy.rotateAfterMs ?: return false
        val connectedAtMs = state.connectedAtMs ?: return false
        val lastInputActivityMs = state.lastInputActivityMs ?: return false
        val lastServerActivityMs = state.lastServerActivityMs ?: return false
        return atMs - connectedAtMs >= rotateAfterMs &&
            atMs - lastInputActivityMs >= policy.rotationQuietMs &&
            atMs - lastServerActivityMs >= policy.rotationQuietMs &&
            safeGap()
    }

    private fun safeGap(): Boolean {
        return state.pendingWorkCount == 0L &&
            state.bufferedInputCount == 0L &&
            !state.userSpeaking
    }

    private fun accepts(generation: Long): Boolean {
        return generation > 0 && generation == state.generation
    }
}

private fun deadlineReached(atMs: Long, deadlineMs: Long?): Boolean {
    return deadlineMs != null && atMs >= deadlineMs
}

private fun clearSessionDeadlines(state: GeminiLiveLifecycleState): GeminiLiveLifecycleState {
    return state.copy(
        setupDeadlineMs = null,
        firstResponseDeadlineMs = null,
        contentIdleDeadlineMs = null,
        hardResponseDeadlineMs = null,
        goAwayDeadlineMs = null,
    )
}

private fun clearDeadlines(state: GeminiLiveLifecycleState): GeminiLiveLifecycleState {
    return clearSessionDeadlines(state).copy(reconnectDeadlineMs = null)
}
