package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.phonecontrol.session.buildPhoneControlTextPayload
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReadySession
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference

internal enum class PhoneControlUiGoalOffer {
    REJECTED,
    QUEUED,
    REPLACED,
}

internal enum class PhoneControlUiGoalFlush {
    NONE,
    WAITING,
    REJECTED,
    SENT,
}

internal enum class PhoneControlUiGoalOutcome {
    COMPLETED,
    INTERRUPTED,
}

internal data class PhoneControlUiGoalOfferResult(
    val disposition: PhoneControlUiGoalOffer,
    val id: Long?,
)

internal data class PhoneControlUiGoalCompletion(
    val id: Long,
    val outcome: PhoneControlUiGoalOutcome,
)

private data class PhoneControlQueuedUiGoal(
    val id: Long,
    val text: String,
)

internal class PhoneControlUserInterfaceGoalQueue(
    private val maximumChars: Int = MAXIMUM_CHARS,
) {
    private val nextId = AtomicLong(0L)
    private val pending = AtomicReference<PhoneControlQueuedUiGoal?>(null)
    private val inFlight = AtomicReference<PhoneControlQueuedUiGoal?>(null)
    private val terminalBoundaryId = AtomicLong(NO_GOAL)

    init {
        require(maximumChars > 0)
    }

    val awaitingSettlement: Boolean
        get() = terminalBoundaryId.get() != NO_GOAL && inFlight.get() != null

    fun offer(text: String, runtimeReady: Boolean): PhoneControlUiGoalOfferResult {
        val goal = text.trim()
        if (!runtimeReady || goal.isEmpty() || goal.length > maximumChars) {
            return PhoneControlUiGoalOfferResult(PhoneControlUiGoalOffer.REJECTED, null)
        }
        val queued = PhoneControlQueuedUiGoal(nextGoalId(), goal)
        val disposition = if (pending.getAndSet(queued) == null) {
            PhoneControlUiGoalOffer.QUEUED
        } else {
            PhoneControlUiGoalOffer.REPLACED
        }
        return PhoneControlUiGoalOfferResult(disposition, queued.id)
    }

    fun flush(
        phase: PhoneControlTurnPhase,
        pendingWorkCount: Int,
        userSpeaking: Boolean,
        send: (String) -> Boolean,
    ): PhoneControlUiGoalFlush {
        if (!canSendUserInterfaceGoal(
                phase = phase,
                pendingWorkCount = pendingWorkCount,
                userSpeaking = userSpeaking,
                goalInFlight = inFlight.get() != null,
            )
        ) {
            return PhoneControlUiGoalFlush.WAITING
        }
        val goal = pending.getAndSet(null) ?: return PhoneControlUiGoalFlush.NONE
        if (!send(buildPhoneControlTextPayload(goal.text))) {
            pending.compareAndSet(null, goal)
            return PhoneControlUiGoalFlush.REJECTED
        }
        inFlight.set(goal)
        terminalBoundaryId.set(NO_GOAL)
        return PhoneControlUiGoalFlush.SENT
    }

    fun flushAtBoundary(
        phase: PhoneControlTurnPhase,
        pendingWorkCount: Int,
        userSpeaking: Boolean,
        send: (String) -> Boolean,
        onSent: () -> Unit,
    ): Boolean = when (flush(phase, pendingWorkCount, userSpeaking, send)) {
        PhoneControlUiGoalFlush.REJECTED -> false
        PhoneControlUiGoalFlush.SENT -> {
            onSent()
            true
        }
        PhoneControlUiGoalFlush.NONE,
        PhoneControlUiGoalFlush.WAITING,
        -> true
    }

    fun observeTurnBoundary(interrupted: Boolean): PhoneControlUiGoalCompletion? {
        val active = inFlight.get() ?: return null
        if (!interrupted) {
            terminalBoundaryId.compareAndSet(NO_GOAL, active.id)
            return null
        }
        if (!inFlight.compareAndSet(active, null)) return null
        terminalBoundaryId.set(NO_GOAL)
        return PhoneControlUiGoalCompletion(active.id, PhoneControlUiGoalOutcome.INTERRUPTED)
    }

    fun settle(
        phase: PhoneControlTurnPhase,
        pendingWorkCount: Int,
        playbackDrained: Boolean,
    ): PhoneControlUiGoalCompletion? {
        val active = inFlight.get() ?: return null
        if (terminalBoundaryId.get() != active.id ||
            !phase.isQuiescent() ||
            pendingWorkCount != 0 ||
            !playbackDrained
        ) {
            return null
        }
        if (!inFlight.compareAndSet(active, null)) return null
        terminalBoundaryId.compareAndSet(active.id, NO_GOAL)
        return PhoneControlUiGoalCompletion(active.id, PhoneControlUiGoalOutcome.COMPLETED)
    }

    fun clear() {
        pending.set(null)
        inFlight.set(null)
        terminalBoundaryId.set(NO_GOAL)
    }

    private fun nextGoalId(): Long {
        while (true) {
            val current = nextId.get()
            val next = if (current == Long.MAX_VALUE) 1L else current + 1L
            if (nextId.compareAndSet(current, next)) return next
        }
    }

    internal companion object {
        const val MAXIMUM_CHARS = 1_024
        const val NO_GOAL = -1L
    }
}

internal fun canSendUserInterfaceGoal(
    phase: PhoneControlTurnPhase,
    pendingWorkCount: Int,
    userSpeaking: Boolean,
    goalInFlight: Boolean,
): Boolean = phase.isQuiescent() &&
    pendingWorkCount == 0 &&
    !userSpeaking &&
    !goalInFlight

private fun PhoneControlTurnPhase.isQuiescent(): Boolean =
    this == PhoneControlTurnPhase.IDLE || this == PhoneControlTurnPhase.LISTENING

internal fun PhoneControlUserInterfaceGoalQueue.flushRuntimeGoal(
    session: GeminiLiveReadySession,
    phase: PhoneControlTurnPhase,
    pendingWorkCount: Int,
    userSpeaking: Boolean,
    sender: PhoneControlOutboundSender,
    onSent: () -> Unit,
): Boolean = flushAtBoundary(
    phase = phase,
    pendingWorkCount = pendingWorkCount,
    userSpeaking = userSpeaking,
    send = { payload ->
        sender.send(
            session,
            payload,
            PhoneControlOutboundKind.USER_INTERFACE_GOAL,
            pendingWorkCount,
            phase,
        )
    },
    onSent = onSent,
)
