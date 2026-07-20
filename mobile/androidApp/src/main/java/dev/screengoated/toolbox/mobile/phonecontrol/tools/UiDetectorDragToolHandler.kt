package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorDragSelection
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorMapping
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorRefreshedMark
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorRefreshedMarkSet
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorTargetSelection
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorTargetSelector
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorTargetVerification
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class UiDetectorDragToolHandler(
    private val backend: UiDetectorToolBackend,
    private val targetSelector: UiDetectorTargetSelector,
) {
    suspend fun dragTarget(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = try {
        execute(job, args)
    } finally {
        backend.clearMarks()
    }

    private suspend fun execute(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val fromDescription = endpointDescription(args, "from")
            ?: return invalidArgs(job, TOOL_NAME, "drag_target requires from")
        val toDescription = endpointDescription(args, "to")
            ?: return invalidArgs(job, TOOL_NAME, "drag_target requires to")
        if (fromDescription.length > MAX_DESCRIPTION_CHARS ||
            toDescription.length > MAX_DESCRIPTION_CHARS
        ) {
            return invalidArgs(job, TOOL_NAME, "drag endpoint description is too long")
        }
        val mapping = when (val result = backend.mapCurrentSurface()) {
            is UiDetectorProviderResult.Failure ->
                return detectorFailure(job, TOOL_NAME, result, backend.observationGeneration)
            is UiDetectorProviderResult.Success -> result.value
        }
        if (mapping.marks.marks.isEmpty()) {
            return dragSelectionFailure(
                job,
                mapping,
                UiDetectorDragSelection.Failure(
                    code = "target_not_found",
                    message = "The current detector frame contains no drag endpoints.",
                    retryable = false,
                ),
            )
        }
        val selection = when (
            val result = targetSelector.selectDrag(fromDescription, toDescription, mapping)
        ) {
            is UiDetectorDragSelection.Failure -> return dragSelectionFailure(job, mapping, result)
            is UiDetectorDragSelection.Success -> result
        }
        val refreshedSet = when (
            val result = backend.refreshMarks(listOf(selection.from.mark, selection.to.mark))
        ) {
            is UiDetectorProviderResult.Failure ->
                return detectorFailure(job, TOOL_NAME, result, backend.observationGeneration)
            is UiDetectorProviderResult.Success -> result.value
        }
        val from = refreshedSet.mark(selection.from.mark)
        val to = refreshedSet.mark(selection.to.mark)
        if (from == null || to == null) {
            return detectorFailure(
                job,
                TOOL_NAME,
                UiDetectorProviderResult.Failure(
                    code = "detector_contract_invalid",
                    message = "The fresh detector result omitted a requested drag endpoint.",
                    retryable = true,
                    freshObservationRequired = true,
                ),
                backend.observationGeneration,
            )
        }
        val fromVerification = when (val result = targetSelector.verify(fromDescription, from)) {
            is UiDetectorTargetVerification.Failure ->
                return dragVerificationFailure(job, "from", from, result)
            is UiDetectorTargetVerification.Success -> result
        }
        val toVerification = when (val result = targetSelector.verify(toDescription, to)) {
            is UiDetectorTargetVerification.Failure ->
                return dragVerificationFailure(job, "to", to, result)
            is UiDetectorTargetVerification.Success -> result
        }
        if (backend.observationGeneration != refreshedSet.observationGeneration) {
            return detectorFailure(
                job,
                TOOL_NAME,
                UiDetectorProviderResult.Failure(
                    code = "stale_target",
                    message = "The surface changed after drag endpoint verification.",
                    retryable = true,
                    freshObservationRequired = true,
                ),
                backend.observationGeneration,
            )
        }
        return dragExecution(
            job,
            refreshedSet,
            from,
            to,
            selection,
            fromVerification,
            toVerification,
            backend.drag(from, to, DRAG_DURATION_MS),
            backend.observationGeneration,
        )
    }
}

private fun endpointDescription(args: JsonObject, name: String): String? =
    args.string(name)?.trim()?.takeIf(String::isNotEmpty)

private fun dragSelectionFailure(
    job: PhoneControlToolJobContext,
    mapping: UiDetectorMapping,
    failure: UiDetectorDragSelection.Failure,
) = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = TOOL_NAME,
        capability = DETECTOR_POINTER_CAPABILITY,
        provider = DETECTOR_PROVIDER,
        providerState = when {
            failure.requiredUserStep != null -> CapabilityState.NEEDS_USER_STEP
            failure.code == "capability_unavailable" -> CapabilityState.UNAVAILABLE
            else -> CapabilityState.DEGRADED
        },
        code = failure.code,
        observationGeneration = mapping.marks.frame.observationGeneration,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = failure.retryable,
        requiredUserStep = failure.requiredUserStep,
        freshObservationRequired = false,
        data = buildJsonObject {
            put("message", failure.message)
            put("frame_identity", mapping.marks.frame.wireIdentity)
        },
    ),
    mutating = false,
)

private fun dragVerificationFailure(
    job: PhoneControlToolJobContext,
    endpoint: String,
    refreshed: UiDetectorRefreshedMark,
    failure: UiDetectorTargetVerification.Failure,
) = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = TOOL_NAME,
        capability = DETECTOR_POINTER_CAPABILITY,
        provider = DETECTOR_PROVIDER,
        providerState = if (failure.requiredUserStep == null) {
            CapabilityState.DEGRADED
        } else {
            CapabilityState.NEEDS_USER_STEP
        },
        code = failure.code,
        observationGeneration = refreshed.observationGeneration,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = failure.retryable,
        requiredUserStep = failure.requiredUserStep,
        freshObservationRequired = failure.freshObservationRequired,
        data = buildJsonObject {
            put("message", failure.message)
            put("endpoint", endpoint)
            put("mark", refreshed.mark.id)
            put("fresh_overlap", refreshed.overlap)
        },
    ),
    mutating = false,
    refreshScreenFrame = failure.freshObservationRequired,
)

private fun dragExecution(
    job: PhoneControlToolJobContext,
    refreshedSet: UiDetectorRefreshedMarkSet,
    from: UiDetectorRefreshedMark,
    to: UiDetectorRefreshedMark,
    selection: UiDetectorDragSelection.Success,
    fromVerification: UiDetectorTargetVerification.Success,
    toVerification: UiDetectorTargetVerification.Success,
    action: AccessibilityProviderResult<dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome>,
    observationGeneration: Long,
): PhoneControlToolExecution = when (action) {
    is AccessibilityProviderResult.Failure -> PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = TOOL_NAME,
            capability = DETECTOR_POINTER_CAPABILITY,
            provider = DETECTOR_PROVIDER,
            providerState = CapabilityState.READY,
            code = action.code,
            observationGeneration = observationGeneration,
            effect = action.effect,
            snapshotInvalidated = action.effect != EffectCertainty.PROVEN_NO_EFFECT,
            retryable = action.retryable,
            requiredUserStep = action.requiredUserStep,
            freshObservationRequired = action.freshObservationRequired,
            data = buildJsonObject {
                put("message", action.message)
                put("input_provider", "accessibility")
                put("input_provider_state", detectorInputProviderState(action).wireName)
            },
        ),
        mutating = detectorGestureIsMutating(action.effect),
        refreshScreenFrame = detectorGestureIsMutating(action.effect),
    )
    is AccessibilityProviderResult.Success -> PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = TOOL_NAME,
            capability = DETECTOR_POINTER_CAPABILITY,
            provider = DETECTOR_PROVIDER,
            providerState = CapabilityState.READY,
            code = action.value.code,
            observationGeneration = action.value.generation,
            effect = action.value.effect,
            snapshotInvalidated = action.value.snapshotInvalidated,
            freshObservationRequired = action.value.snapshotInvalidated,
            data = buildJsonObject {
                putEndpoint("from", from, selection.from, fromVerification)
                putEndpoint("to", to, selection.to, toVerification)
                put("verification_inference_ms", refreshedSet.inferenceMs)
                put("input_provider", "accessibility")
                put("input_provider_state", CapabilityState.READY.wireName)
            },
        ),
        mutating = detectorGestureIsMutating(action.value.effect),
        refreshScreenFrame = action.value.snapshotInvalidated,
    )
}

private fun kotlinx.serialization.json.JsonObjectBuilder.putEndpoint(
    prefix: String,
    refreshed: UiDetectorRefreshedMark,
    selection: UiDetectorTargetSelection.Success,
    verification: UiDetectorTargetVerification.Success,
) {
    put("${prefix}_mark", refreshed.mark.id)
    put("${prefix}_screen_x", refreshed.mark.box.centerX)
    put("${prefix}_screen_y", refreshed.mark.box.centerY)
    put("${prefix}_fresh_overlap", refreshed.overlap)
    put("${prefix}_selection_model", selection.modelId)
    put("${prefix}_selection_confidence", selection.confidence)
    put("${prefix}_verification_model", verification.modelId)
    put("${prefix}_verification_confidence", verification.confidence)
}

private const val TOOL_NAME = "drag_target"
private const val DRAG_DURATION_MS = 550L
