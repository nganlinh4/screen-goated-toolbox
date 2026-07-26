package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal fun detectorPointerExecution(
    job: PhoneControlToolJobContext,
    requestedTool: String,
    input: PointerInputOutcome,
    evidence: JsonObject,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = requestedTool,
        capability = DETECTOR_POINTER_CAPABILITY,
        provider = DETECTOR_PROVIDER,
        providerState = CapabilityState.READY,
        code = input.code,
        observationGeneration = input.generation,
        effect = input.effect,
        snapshotInvalidated = input.snapshotInvalidated,
        retryable = input.retryable,
        requiredUserStep = input.requiredUserStep,
        freshObservationRequired = input.freshObservationRequired,
        data = buildJsonObject {
            evidence.forEach { (key, value) -> put(key, value) }
            put("input_provider", input.providerId)
            put("input_provider_state", input.providerState.wireName)
            input.message?.let { put("message", it) }
        },
    ),
    mutating = input.effect != EffectCertainty.PROVEN_NO_EFFECT,
    refreshScreenFrame = input.snapshotInvalidated,
)
