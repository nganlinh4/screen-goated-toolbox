package dev.screengoated.toolbox.mobile.phonecontrol.lifecycle

@JvmInline
internal value class PhoneControlTurnId(
    val value: Long,
) {
    init {
        require(value >= 0) { "turn id must be non-negative" }
    }
}

@JvmInline
internal value class PhoneControlGenerationId(
    val value: Long,
) {
    init {
        require(value >= 0) { "generation id must be non-negative" }
    }
}

@JvmInline
internal value class PhoneControlJobId(
    val value: String,
) {
    init {
        require(value.isNotBlank()) { "job id must not be blank" }
    }
}

@JvmInline
internal value class PhoneControlSnapshotGeneration(
    val value: Long,
) {
    init {
        require(value >= 0) { "snapshot generation must be non-negative" }
    }
}

@JvmInline
internal value class PhoneControlTargetId(
    val value: String,
) {
    init {
        require(value.isNotBlank()) { "target id must not be blank" }
    }
}

internal data class PhoneControlTargetIdentity(
    val id: PhoneControlTargetId,
    val snapshotGeneration: PhoneControlSnapshotGeneration,
)

internal enum class PhoneControlTurnPhase(
    val contractValue: String,
) {
    IDLE("idle"),
    LISTENING("listening"),
    WORKING("working"),
    FINALIZING("finalizing"),
}

internal data class PhoneControlTurnPolicy(
    val maximumFinalResponsesPerUserTurn: Int = 1,
    val maximumAdmittedToolJobs: Int = 1,
    val maximumHeldToolRejections: Int = 32,
    val rejectionOverflowAbandonsSession: Boolean = true,
    val doneIsTerminal: Boolean = true,
    val cleanupProducesOutput: Boolean = false,
    val currentGenerationAudioBlockedByTools: Boolean = false,
    val lateRetiredEventsAreAbsorbed: Boolean = true,
    val unknownMutationRequiresReconciliation: Boolean = true,
    val completionRequiresNoPendingJobs: Boolean = true,
    val reconciliationBlocksMutationAndCompletion: Boolean = true,
    val cancellationRequestIsNotTerminalAcknowledgement: Boolean = true,
    val acceptedEffectRetainsSingleFlightSlotUntilProviderTerminal: Boolean = true,
)

internal val PHONE_CONTROL_TURN_POLICY = PhoneControlTurnPolicy()

internal enum class PhoneControlEffectCertainty {
    VERIFIED,
    MAY_HAVE_OCCURRED,
    PROVEN_NO_EFFECT,
    UNKNOWN,
    ;

    val requiresReconciliation: Boolean
        get() = this == MAY_HAVE_OCCURRED || this == UNKNOWN
}

internal sealed interface PhoneControlTurnEvent {
    data class JobReceipt(
        val generation: PhoneControlGenerationId? = null,
        val jobId: PhoneControlJobId? = null,
        val certainty: PhoneControlEffectCertainty,
    ) : PhoneControlTurnEvent

    data class TerminalDone(
        val generation: PhoneControlGenerationId? = null,
        val assistantContentSeen: Boolean? = null,
    ) : PhoneControlTurnEvent

    data class JobRequested(
        val generation: PhoneControlGenerationId,
        val jobId: PhoneControlJobId? = null,
    ) : PhoneControlTurnEvent

    data class AudioReceived(
        val chunk: PhoneControlOutputChunk,
    ) : PhoneControlTurnEvent

    data class CaptionReceived(
        val chunk: PhoneControlOutputChunk,
    ) : PhoneControlTurnEvent

    data class CleanupCompleted(
        val generation: PhoneControlGenerationId? = null,
    ) : PhoneControlTurnEvent

    data class AssistantContentReceived(
        val generation: PhoneControlGenerationId,
    ) : PhoneControlTurnEvent

    data class GenerationCompleted(
        val generation: PhoneControlGenerationId,
    ) : PhoneControlTurnEvent

    data class UserBargeIn(
        val newTurn: PhoneControlTurnId,
        val newGeneration: PhoneControlGenerationId,
    ) : PhoneControlTurnEvent

    data class GenerationInterrupted(
        val generation: PhoneControlGenerationId,
    ) : PhoneControlTurnEvent

    data class MutationInterrupted(
        val certainty: PhoneControlEffectCertainty,
    ) : PhoneControlTurnEvent

    data object MutationRequested : PhoneControlTurnEvent

    data class FreshObservation(
        val stateReconciled: Boolean,
    ) : PhoneControlTurnEvent

    data class TargetObserved(
        val target: PhoneControlTargetIdentity,
    ) : PhoneControlTurnEvent

    data class SurfaceChanged(
        val snapshotGeneration: PhoneControlSnapshotGeneration,
    ) : PhoneControlTurnEvent

    data class TargetActionRequested(
        val target: PhoneControlTargetIdentity,
    ) : PhoneControlTurnEvent

    data class TransportFailed(
        val generation: PhoneControlGenerationId,
    ) : PhoneControlTurnEvent

    data class SocketOpened(
        val generation: PhoneControlGenerationId,
    ) : PhoneControlTurnEvent
}

internal enum class PhoneControlTurnDecisionCode(
    val contractValue: String,
) {
    ACCEPTED("accepted"),
    ABSORBED("absorbed"),
    BLOCKED_RECONCILIATION_REQUIRED("blocked_reconciliation_required"),
    TOOL_CALL_IN_FLIGHT("tool_call_in_flight"),
    STALE_TARGET("stale_target"),
    WRONG_GENERATION("wrong_generation"),
    TURN_CLOSED("turn_closed"),
    DUPLICATE_EVENT("duplicate_event"),
}

internal data class PhoneControlTurnTransition(
    val decision: PhoneControlTurnDecisionCode,
    val effects: List<PhoneControlTurnEffect> = emptyList(),
)

internal sealed interface PhoneControlTurnEffect {
    data class DispatchJob(
        val generation: PhoneControlGenerationId,
        val jobId: PhoneControlJobId?,
    ) : PhoneControlTurnEffect

    data class DeliverJobReceipt(
        val generation: PhoneControlGenerationId?,
        val jobId: PhoneControlJobId?,
        val certainty: PhoneControlEffectCertainty,
    ) : PhoneControlTurnEffect

    data class CancelJob(
        val jobId: PhoneControlJobId,
    ) : PhoneControlTurnEffect

    data class PlayAudio(
        val chunk: PhoneControlOutputChunk,
    ) : PhoneControlTurnEffect

    data class DeliverCaption(
        val chunk: PhoneControlOutputChunk,
    ) : PhoneControlTurnEffect

    data class RetireGeneration(
        val generation: PhoneControlGenerationId,
    ) : PhoneControlTurnEffect

    data class DiscardGenerationOutput(
        val generation: PhoneControlGenerationId,
    ) : PhoneControlTurnEffect

    data class FinalResponseReady(
        val turn: PhoneControlTurnId?,
    ) : PhoneControlTurnEffect

    data class FinalGenerationRequested(
        val completedGeneration: PhoneControlGenerationId?,
    ) : PhoneControlTurnEffect

    data class ReconciliationRequired(
        val generation: PhoneControlGenerationId?,
        val jobId: PhoneControlJobId?,
    ) : PhoneControlTurnEffect

    data object ReconciliationCleared : PhoneControlTurnEffect

    data object AuthorizeMutation : PhoneControlTurnEffect

    data class PerformTargetAction(
        val target: PhoneControlTargetIdentity,
    ) : PhoneControlTurnEffect

    data object FreshObservationRequired : PhoneControlTurnEffect

    data class AcceptSocket(
        val generation: PhoneControlGenerationId,
    ) : PhoneControlTurnEffect

    data class RecoverTransport(
        val generation: PhoneControlGenerationId,
    ) : PhoneControlTurnEffect

    data class ActivateTurn(
        val turn: PhoneControlTurnId,
        val generation: PhoneControlGenerationId,
    ) : PhoneControlTurnEffect

    data class CleanupAcknowledged(
        val generation: PhoneControlGenerationId?,
    ) : PhoneControlTurnEffect
}

internal enum class PhoneControlGenerationStatus {
    ACTIVE,
    RETIRED,
}

internal data class PhoneControlGenerationState(
    val id: PhoneControlGenerationId,
    val status: PhoneControlGenerationStatus,
    val assistantContentSeen: Boolean = false,
)

internal enum class PhoneControlJobStatus {
    PENDING,
    COMPLETED,
    CANCELLED,
}

internal data class PhoneControlTurnJob(
    val id: PhoneControlJobId,
    val ownerTurn: PhoneControlTurnId?,
    val ownerGeneration: PhoneControlGenerationId,
    val status: PhoneControlJobStatus = PhoneControlJobStatus.PENDING,
)

internal data class PhoneControlRetainedStateCounts(
    val activeGenerationStates: Int,
    val retiredGenerationBoundaries: Int,
    val jobRecords: Int,
    val cancelledJobRecords: Int,
    val observedTargetRecords: Int,
)

internal data class PhoneControlOutputChunk(
    val generation: PhoneControlGenerationId,
    val sequence: Long?,
) {
    init {
        require(sequence == null || sequence >= 0) { "output sequence must be non-negative" }
    }
}
