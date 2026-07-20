package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlOperationId
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlResultEnvelope
import dev.screengoated.toolbox.mobile.phonecontrol.result.RequiredUserStep
import dev.screengoated.toolbox.mobile.phonecontrol.result.ResultScope
import kotlinx.serialization.json.JsonObject

internal data class PhoneControlToolJobContext(
    val turnId: Long,
    val jobId: String,
    val responseGeneration: Long,
    val effectOwner: PhoneControlEffectOwner = PhoneControlEffectOwner(
        PhoneControlOperationId(turnId, responseGeneration, jobId),
    ),
) {
    init {
        require(turnId > 0)
        require(jobId.isNotBlank())
        require(responseGeneration > 0)
    }

    val operationId: String
        get() = effectOwner.operationId.wireValue
}

internal data class PhoneControlToolExecution(
    val response: JsonObject,
    val mutating: Boolean,
    val terminalSummary: String? = null,
    val refreshScreenFrame: Boolean = false,
    /** Current-generation visual evidence sent immediately before this tool receipt. */
    val screenFramePayload: String? = null,
)

internal fun toolResponse(
    job: PhoneControlToolJobContext,
    requestedTool: String,
    capability: String,
    provider: String,
    providerState: CapabilityState,
    code: String,
    observationGeneration: Long,
    effect: EffectCertainty,
    snapshotInvalidated: Boolean,
    retryable: Boolean = false,
    requiredUserStep: String? = null,
    freshObservationRequired: Boolean? = null,
    data: JsonObject = JsonObject(emptyMap()),
): JsonObject {
    val envelope = PhoneControlResultEnvelope(
        code = code,
        capability = capability,
        requestedTool = requestedTool,
        turnId = job.turnId,
        jobId = job.jobId,
        provider = provider,
        providerState = providerState,
        observationGeneration = observationGeneration,
        effect = effect,
        snapshotInvalidated = snapshotInvalidated,
        retryable = retryable,
        requiredUserStep = requiredUserStep?.let { RequiredUserStep(it) },
        freshObservationRequired = freshObservationRequired,
        scope = ResultScope(surface = "android"),
    ).toWireJson()
    return JsonObject(data + envelope)
}

internal fun unavailableToolResponse(
    job: PhoneControlToolJobContext,
    requestedTool: String,
    capability: String,
    provider: String,
    state: CapabilityState,
    requiredUserStep: String? = null,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = requestedTool,
        capability = capability,
        provider = provider,
        providerState = state,
        code = "capability_unavailable",
        observationGeneration = 0,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = state in setOf(
            CapabilityState.DEGRADED,
            CapabilityState.NEEDS_USER_STEP,
            CapabilityState.REVOKED,
            CapabilityState.UNAVAILABLE,
        ),
        requiredUserStep = requiredUserStep,
    ),
    mutating = false,
)
