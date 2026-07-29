package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.grounding.VisualGroundingMapping
import dev.screengoated.toolbox.mobile.phonecontrol.provider.grounding.VisualGroundingResult
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal fun visualGroundingFailure(
    job: PhoneControlToolJobContext,
    requestedTool: String,
    failure: VisualGroundingResult.Failure,
    observationGeneration: Long,
    stage: String? = null,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = requestedTool,
        capability = if (requestedTool in VISUAL_POINTER_TOOLS) {
            VISUAL_POINTER_CAPABILITY
        } else {
            GROUNDING_CAPABILITY
        },
        provider = VISUAL_GROUNDING_PROVIDER,
        providerState = visualGroundingProviderState(failure),
        code = failure.code,
        observationGeneration = observationGeneration,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = failure.retryable,
        requiredUserStep = failure.requiredUserStep,
        freshObservationRequired = failure.freshObservationRequired,
        data = buildJsonObject {
            put("message", failure.message)
            stage?.let { put("grounding_stage", it) }
        },
    ),
    mutating = false,
    refreshScreenFrame = failure.freshObservationRequired,
)

internal fun visualPointerExecution(
    job: PhoneControlToolJobContext,
    requestedTool: String,
    input: PointerInputOutcome,
    evidence: JsonObject,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = requestedTool,
        capability = VISUAL_POINTER_CAPABILITY,
        provider = VISUAL_GROUNDING_PROVIDER,
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

internal fun visualMappingExecution(
    job: PhoneControlToolJobContext,
    description: String,
    mapping: VisualGroundingMapping,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = "map_targets",
        capability = GROUNDING_CAPABILITY,
        provider = VISUAL_GROUNDING_PROVIDER,
        providerState = CapabilityState.READY,
        code = if (mapping.marks.marks.isEmpty()) "no_targets" else "ok",
        observationGeneration = mapping.marks.frame.identity.observationGeneration,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = mapping.marks.marks.isEmpty(),
        data = buildJsonObject {
            put("description", description)
            put("frame_identity", mapping.marks.frame.wireIdentity)
            put("display_id", mapping.marks.frame.identity.displayId)
            put("window_id", mapping.marks.frame.identity.windowId)
            put("surface", mapping.marks.frame.identity.packageOrSurface)
            put("mapping_model_ms", mapping.groundingMs)
            put("grounding_model", mapping.modelId)
            put(
                "marks",
                buildJsonArray {
                    mapping.marks.marks.forEach { mark ->
                        add(
                            buildJsonObject {
                                put("mark", mark.id)
                                put("center_x", mark.point.centerX)
                                put("center_y", mark.point.centerY)
                                put("label", mark.point.label)
                            },
                        )
                    }
                },
            )
        },
    ),
    mutating = false,
    refreshScreenFrame = true,
)

internal fun staleVisualGesture(message: String) = VisualGroundingResult.Failure(
    code = "stale_target",
    message = message,
    retryable = true,
    freshObservationRequired = true,
)

internal fun visualGroundingProviderState(
    failure: VisualGroundingResult.Failure,
): CapabilityState = when {
    failure.requiredUserStep != null -> CapabilityState.NEEDS_USER_STEP
    failure.code == "capability_unavailable" -> CapabilityState.UNAVAILABLE
    failure.code == "unsupported_display" -> CapabilityState.UNSUPPORTED
    failure.code in VISUAL_REQUEST_OUTCOME_CODES -> CapabilityState.READY
    else -> CapabilityState.DEGRADED
}

internal const val VISUAL_GROUNDING_PROVIDER = "current_frame_vision"
internal const val VISUAL_POINTER_CAPABILITY = "ui.pointer_action"
internal const val GROUNDING_CAPABILITY = "blind_surface_grounding"
internal const val MAX_VISUAL_DESCRIPTION_CHARS = 480
internal const val VISUAL_LONG_PRESS_MS = 650L
internal val SUPPORTED_VISUAL_BUTTONS = setOf("left", "right")

private val VISUAL_POINTER_TOOLS = setOf("click_target", "click_mark", "drag_target")
private val VISUAL_REQUEST_OUTCOME_CODES = setOf(
    "stale_frame",
    "stale_target",
    "target_not_found",
    "surface_unavailable",
    "surface_authority_unknown",
    "surface_outside_capture",
)
