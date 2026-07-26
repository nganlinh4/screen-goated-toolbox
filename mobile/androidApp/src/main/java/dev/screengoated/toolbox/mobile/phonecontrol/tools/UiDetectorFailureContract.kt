package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal fun detectorFailure(
    job: PhoneControlToolJobContext,
    requestedTool: String,
    failure: UiDetectorProviderResult.Failure,
    observationGeneration: Long,
    stage: String? = null,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = requestedTool,
        capability = if (requestedTool in DETECTOR_POINTER_TOOLS) {
            DETECTOR_POINTER_CAPABILITY
        } else {
            GROUNDING_CAPABILITY
        },
        provider = DETECTOR_PROVIDER,
        providerState = detectorProviderState(failure),
        code = failure.code,
        observationGeneration = observationGeneration,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = failure.retryable,
        requiredUserStep = failure.requiredUserStep,
        freshObservationRequired = failure.freshObservationRequired,
        data = buildJsonObject {
            put("message", failure.message)
            stage?.let { put("detector_stage", it) }
        },
    ),
    mutating = false,
    refreshScreenFrame = failure.freshObservationRequired,
)

internal fun detectorGestureIsMutating(effect: EffectCertainty): Boolean =
    effect.effectMayHaveOccurred != false

internal fun detectorInputProviderState(
    failure: AccessibilityProviderResult.Failure,
): CapabilityState = when {
    failure.requiredUserStep != null -> CapabilityState.NEEDS_USER_STEP
    failure.code == "capability_unavailable" -> CapabilityState.UNAVAILABLE
    else -> CapabilityState.DEGRADED
}

internal fun detectorProviderState(
    failure: UiDetectorProviderResult.Failure,
): CapabilityState = when {
    failure.requiredUserStep != null -> CapabilityState.NEEDS_USER_STEP
    failure.code == "capability_unavailable" -> CapabilityState.UNAVAILABLE
    failure.code == "unsupported_display" -> CapabilityState.UNSUPPORTED
    failure.code in DETECTOR_REQUEST_OUTCOME_CODES -> CapabilityState.READY
    else -> CapabilityState.DEGRADED
}

internal fun staleDetectorGesture(message: String) = AccessibilityProviderResult.Failure(
    code = "stale_target",
    message = message,
    retryable = true,
    freshObservationRequired = true,
)

internal const val DETECTOR_PROVIDER = "local_ui_detector"
internal const val DETECTOR_POINTER_CAPABILITY = "ui.pointer_action"
internal const val MAX_DESCRIPTION_CHARS = 480
internal const val LONG_PRESS_MS = 650L
internal val SUPPORTED_BUTTONS = setOf("left", "right")

internal const val GROUNDING_CAPABILITY = "blind_surface_grounding"
private val DETECTOR_POINTER_TOOLS = setOf("click_target", "click_mark", "drag_target")
private val DETECTOR_REQUEST_OUTCOME_CODES = setOf(
    "stale_frame",
    "stale_target",
    "surface_unavailable",
    "surface_authority_unknown",
    "surface_outside_capture",
)
