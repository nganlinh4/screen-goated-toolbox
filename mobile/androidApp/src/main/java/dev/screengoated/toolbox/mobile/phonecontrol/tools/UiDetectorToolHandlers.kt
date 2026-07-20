package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityMutationKind
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.surfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.AndroidUiDetectorTargetSelector
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorMapping
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorRefreshedMark
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorRefreshedMarkSet
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorTargetSelection
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorTargetSelector
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorTargetVerification
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class UiDetectorToolHandlers(
    private val backend: UiDetectorToolBackend,
    private val targetSelector: UiDetectorTargetSelector,
) {
    constructor(context: Context) : this(
        backend = AndroidUiDetectorToolBackend(context),
        targetSelector = AndroidUiDetectorTargetSelector(context),
    )

    suspend fun mapTargets(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val description = args.string("description")?.trim()
            ?.takeIf(String::isNotEmpty)
            ?: return invalidArgs(job, "map_targets", "map_targets requires description")
        if (description.length > MAX_DESCRIPTION_CHARS) {
            return invalidArgs(job, "map_targets", "description is too long")
        }
        return when (val result = backend.mapCurrentSurface()) {
            is UiDetectorProviderResult.Failure ->
                detectorFailure(job, "map_targets", result, backend.observationGeneration)
            is UiDetectorProviderResult.Success -> mappingExecution(
                job,
                requestedTool = "map_targets",
                description = description,
                mapping = result.value,
                code = if (result.value.marks.marks.isEmpty()) "no_targets" else "ok",
            )
        }
    }

    suspend fun clickTarget(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val description = args.string("description")?.trim()
            ?.takeIf(String::isNotEmpty)
            ?: return invalidArgs(job, "click_target", "click_target requires description")
        if (description.length > MAX_DESCRIPTION_CHARS) {
            return invalidArgs(job, "click_target", "description is too long")
        }
        val button = args.string("button") ?: "left"
        if (button !in SUPPORTED_BUTTONS) {
            return invalidArgs(job, "click_target", "button must be left or right")
        }
        val mapping = when (val result = backend.mapCurrentSurface()) {
            is UiDetectorProviderResult.Failure ->
                return detectorFailure(job, "click_target", result, backend.observationGeneration)
            is UiDetectorProviderResult.Success -> result.value
        }
        if (mapping.marks.marks.isEmpty()) {
            return mappingExecution(
                job,
                requestedTool = "click_target",
                description = description,
                mapping = mapping,
                code = "no_targets",
                requestedButton = button,
            )
        }
        val selected = try {
            targetSelector.select(description, mapping)
        } catch (error: Throwable) {
            backend.clearMarks()
            throw error
        }
        val selection = when (selected) {
            is UiDetectorTargetSelection.Failure -> {
                backend.clearMarks()
                return targetSelectionFailure(job, mapping, selected)
            }
            is UiDetectorTargetSelection.Success -> selected
        }
        val refreshResult = try {
            backend.refreshMark(selection.mark)
        } catch (error: Throwable) {
            backend.clearMarks()
            throw error
        }
        val refreshed = when (val result = refreshResult) {
            is UiDetectorProviderResult.Failure -> {
                backend.clearMarks()
                return detectorFailure(job, "click_target", result, backend.observationGeneration)
            }
            is UiDetectorProviderResult.Success -> result.value
        }
        val verified = try {
            targetSelector.verify(description, refreshed)
        } catch (error: Throwable) {
            backend.clearMarks()
            throw error
        }
        val verification = when (verified) {
            is UiDetectorTargetVerification.Failure -> {
                backend.clearMarks()
                return targetVerificationFailure(job, refreshed, verified)
            }
            is UiDetectorTargetVerification.Success -> verified
        }
        return executeRefreshedMark(
            job,
            "click_target",
            button,
            refreshed,
            selection,
            verification,
        )
    }

    suspend fun dragTarget(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution =
        UiDetectorDragToolHandler(backend, targetSelector).dragTarget(job, args)

    suspend fun clickMark(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val id = args.int("mark")
            ?: return invalidArgs(job, "click_mark", "click_mark requires integer mark")
        val button = args.string("button") ?: "left"
        if (button !in SUPPORTED_BUTTONS) {
            return invalidArgs(job, "click_mark", "button must be left or right")
        }
        val refreshResult = try {
            backend.refreshMark(id)
        } catch (error: Throwable) {
            backend.clearMarks()
            throw error
        }
        val refreshed = when (val result = refreshResult) {
            is UiDetectorProviderResult.Failure -> {
                backend.clearMarks()
                return detectorFailure(job, "click_mark", result, backend.observationGeneration)
            }
            is UiDetectorProviderResult.Success -> result.value
        }
        return executeRefreshedMark(job, "click_mark", button, refreshed)
    }

    private suspend fun executeRefreshedMark(
        job: PhoneControlToolJobContext,
        requestedTool: String,
        button: String,
        refreshed: UiDetectorRefreshedMark,
        selection: UiDetectorTargetSelection.Success? = null,
        verification: UiDetectorTargetVerification.Success? = null,
    ): PhoneControlToolExecution {
        val id = refreshed.mark.id
        val point = refreshed.mark.box
        if (backend.observationGeneration != refreshed.observationGeneration) {
            backend.clearMarks()
            return detectorFailure(
                job,
                requestedTool,
                UiDetectorProviderResult.Failure(
                    code = "stale_target",
                    message = "The surface changed after detector mark verification.",
                    retryable = true,
                    freshObservationRequired = true,
                ),
                backend.observationGeneration,
            )
        }
        val action = try {
            backend.activate(refreshed, button)
        } finally {
            backend.clearMarks()
        }
        return when (action) {
            is AccessibilityProviderResult.Failure -> PhoneControlToolExecution(
                response = toolResponse(
                    job = job,
                    requestedTool = requestedTool,
                    capability = DETECTOR_POINTER_CAPABILITY,
                    provider = DETECTOR_PROVIDER,
                    providerState = CapabilityState.READY,
                    code = action.code,
                    observationGeneration = backend.observationGeneration,
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
                    requestedTool = requestedTool,
                    capability = DETECTOR_POINTER_CAPABILITY,
                    provider = DETECTOR_PROVIDER,
                    providerState = CapabilityState.READY,
                    code = action.value.code,
                    observationGeneration = action.value.generation,
                    effect = action.value.effect,
                    snapshotInvalidated = action.value.snapshotInvalidated,
                    freshObservationRequired = action.value.snapshotInvalidated,
                    data = buildJsonObject {
                        put("clicked_mark", id)
                        put("button", button)
                        put("screen_x", point.centerX)
                        put("screen_y", point.centerY)
                        put("fresh_overlap", refreshed.overlap)
                        put("verification_inference_ms", refreshed.inferenceMs)
                        put("input_provider", "accessibility")
                        put("input_provider_state", CapabilityState.READY.wireName)
                        selection?.let {
                            put("target_selection_model", it.modelId)
                            put("target_selection_confidence", it.confidence)
                            it.what?.let { what -> put("saw_at_target", what) }
                        }
                        verification?.let {
                            put("target_verification_model", it.modelId)
                            put("target_verification_confidence", it.confidence)
                            it.what?.let { what -> put("verified_at_target", what) }
                        }
                    },
                ),
                mutating = detectorGestureIsMutating(action.value.effect),
                refreshScreenFrame = action.value.snapshotInvalidated,
            )
        }
    }
}

internal interface UiDetectorToolBackend {
    val observationGeneration: Long

    suspend fun mapCurrentSurface(): UiDetectorProviderResult<UiDetectorMapping>

    suspend fun refreshMark(id: Int): UiDetectorProviderResult<UiDetectorRefreshedMark>

    suspend fun refreshMarks(ids: List<Int>): UiDetectorProviderResult<UiDetectorRefreshedMarkSet>

    suspend fun activate(
        refreshed: UiDetectorRefreshedMark,
        button: String,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome>

    suspend fun drag(
        from: UiDetectorRefreshedMark,
        to: UiDetectorRefreshedMark,
        durationMs: Long,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome>

    fun clearMarks()
}

private class AndroidUiDetectorToolBackend(context: Context) : UiDetectorToolBackend {
    private val detector = UiDetectorProvider(context)

    override val observationGeneration: Long
        get() = PhoneControlAccessibilityProvider.observationGeneration

    override suspend fun mapCurrentSurface(): UiDetectorProviderResult<UiDetectorMapping> =
        detector.mapCurrentSurface()

    override suspend fun refreshMark(id: Int): UiDetectorProviderResult<UiDetectorRefreshedMark> =
        detector.refreshMark(id)

    override suspend fun refreshMarks(
        ids: List<Int>,
    ): UiDetectorProviderResult<UiDetectorRefreshedMarkSet> = detector.refreshMarks(ids)

    override suspend fun activate(
        refreshed: UiDetectorRefreshedMark,
        button: String,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome> {
        validateCurrentLease(refreshed.surfaceLease)?.let { return it }
        val point = refreshed.mark.box
        return if (button == "right") {
            PhoneControlAccessibilityProvider.swipe(
                refreshed.surfaceLease,
                point.centerX.toFloat(),
                point.centerY.toFloat(),
                point.centerX.toFloat(),
                point.centerY.toFloat(),
                LONG_PRESS_MS,
                AccessibilityMutationKind.LONG_PRESS,
                expectedVisualRevision = refreshed.visualRevision,
            )
        } else {
            PhoneControlAccessibilityProvider.click(
                refreshed.surfaceLease,
                point.centerX.toFloat(),
                point.centerY.toFloat(),
                expectedVisualRevision = refreshed.visualRevision,
            )
        }
    }

    override suspend fun drag(
        from: UiDetectorRefreshedMark,
        to: UiDetectorRefreshedMark,
        durationMs: Long,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome> {
        detectorDragFrameFailure(from, to)?.let { return it }
        validateCurrentLease(from.surfaceLease)?.let { return it }
        return PhoneControlAccessibilityProvider.swipe(
            lease = from.surfaceLease,
            fromX = from.mark.box.centerX.toFloat(),
            fromY = from.mark.box.centerY.toFloat(),
            toX = to.mark.box.centerX.toFloat(),
            toY = to.mark.box.centerY.toFloat(),
            durationMs = durationMs,
            kind = AccessibilityMutationKind.POINTER_ACTIVATE,
            expectedVisualRevision = from.visualRevision,
        )
    }

    private suspend fun validateCurrentLease(
        lease: AccessibilitySurfaceLease,
    ): AccessibilityProviderResult.Failure? {
        val observed = when (val result = PhoneControlAccessibilityProvider.observe(maxElements = 1)) {
            is AccessibilityProviderResult.Failure -> return result
            is AccessibilityProviderResult.Success -> result.value
        }
        val currentLease = observed.surfaceLease(lease.displayId, lease.windowId)
        return if (currentLease == lease) null else {
            staleDetectorGesture("The verified detector surface changed before gesture dispatch.")
        }
    }

    override fun clearMarks() = detector.clearMarks()
}

internal fun detectorDragFrameFailure(
    from: UiDetectorRefreshedMark,
    to: UiDetectorRefreshedMark,
): AccessibilityProviderResult.Failure? {
    if (from.surfaceLease != to.surfaceLease ||
        from.observationGeneration != to.observationGeneration
    ) {
        return staleDetectorGesture("The verified drag endpoints do not share one surface lease.")
    }
    if (from.visualRevision != to.visualRevision) {
        return AccessibilityProviderResult.Failure(
            code = "stale_frame",
            message = "The verified drag endpoints do not share one visual revision.",
            retryable = true,
            freshObservationRequired = true,
        )
    }
    return null
}

private fun targetSelectionFailure(
    job: PhoneControlToolJobContext,
    mapping: UiDetectorMapping,
    failure: UiDetectorTargetSelection.Failure,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = "click_target",
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

private fun targetVerificationFailure(
    job: PhoneControlToolJobContext,
    refreshed: UiDetectorRefreshedMark,
    failure: UiDetectorTargetVerification.Failure,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = "click_target",
        capability = DETECTOR_POINTER_CAPABILITY,
        provider = DETECTOR_PROVIDER,
        providerState = when {
            failure.requiredUserStep != null -> CapabilityState.NEEDS_USER_STEP
            failure.code == "capability_unavailable" -> CapabilityState.UNAVAILABLE
            else -> CapabilityState.DEGRADED
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
            put("mark", refreshed.mark.id)
            put("fresh_overlap", refreshed.overlap)
            put("verification_inference_ms", refreshed.inferenceMs)
        },
    ),
    mutating = false,
    refreshScreenFrame = failure.freshObservationRequired,
)

private fun mappingExecution(
    job: PhoneControlToolJobContext,
    requestedTool: String,
    description: String,
    mapping: UiDetectorMapping,
    code: String,
    requestedButton: String? = null,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = requestedTool,
        capability = if (requestedTool == "click_target") {
            DETECTOR_POINTER_CAPABILITY
        } else {
            GROUNDING_CAPABILITY
        },
        provider = DETECTOR_PROVIDER,
        providerState = CapabilityState.READY,
        code = code,
        observationGeneration = mapping.marks.frame.observationGeneration,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = code != "ok",
        freshObservationRequired = false,
        data = buildJsonObject {
            put("description", description)
            requestedButton?.let { put("requested_button", it) }
            put("frame_identity", mapping.marks.frame.wireIdentity)
            put("display_id", mapping.marks.frame.displayId)
            put("window_id", mapping.marks.frame.windowId)
            put("surface", mapping.marks.frame.packageOrSurface)
            put("inference_ms", mapping.inferenceMs)
            put("execution_provider", mapping.executionProvider)
            put("thresholded", mapping.stats.thresholded)
            put("rejected_invalid", mapping.stats.rejectedInvalid)
            put("suppressed_duplicates", mapping.stats.suppressedDuplicates)
            put("truncated", mapping.stats.truncated)
            put(
                "marks",
                buildJsonArray {
                    mapping.marks.marks.forEach { mark ->
                        add(
                            buildJsonObject {
                                put("mark", mark.id)
                                put("center_x", mark.box.centerX)
                                put("center_y", mark.box.centerY)
                                put("score", mark.box.score)
                                put("bounds", mark.box.bounds.toWireJson())
                            },
                        )
                    }
                },
            )
            put(
                "note",
                if (mapping.marks.marks.isEmpty()) {
                    "No clickable detector candidates were found on the current frame."
                } else {
                    "The same numbered candidates are drawn on the current frame; choose one with click_mark."
                },
            )
        },
    ),
    mutating = false,
    refreshScreenFrame = true,
)

internal fun detectorFailure(
    job: PhoneControlToolJobContext,
    requestedTool: String,
    failure: UiDetectorProviderResult.Failure,
    observationGeneration: Long,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = requestedTool,
        capability = if (
            requestedTool == "click_target" || requestedTool == "click_mark" ||
            requestedTool == "drag_target"
        ) {
            DETECTOR_POINTER_CAPABILITY
        } else {
            GROUNDING_CAPABILITY
        },
        provider = DETECTOR_PROVIDER,
        providerState = when {
            failure.requiredUserStep != null -> CapabilityState.NEEDS_USER_STEP
            failure.code == "structured_surface_available" -> CapabilityState.DEGRADED
            else -> CapabilityState.UNAVAILABLE
        },
        code = failure.code,
        observationGeneration = observationGeneration,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = failure.retryable,
        requiredUserStep = failure.requiredUserStep,
        freshObservationRequired = failure.freshObservationRequired,
        data = buildJsonObject { put("message", failure.message) },
    ),
    mutating = false,
    refreshScreenFrame = failure.freshObservationRequired,
)

internal const val DETECTOR_PROVIDER = "local_ui_detector"
private const val GROUNDING_CAPABILITY = "blind_surface_grounding"
internal const val DETECTOR_POINTER_CAPABILITY = "ui.pointer_action"
internal const val MAX_DESCRIPTION_CHARS = 480
private const val LONG_PRESS_MS = 650L
private val SUPPORTED_BUTTONS = setOf("left", "right")

internal fun detectorGestureIsMutating(effect: EffectCertainty): Boolean =
    effect.effectMayHaveOccurred != false

internal fun detectorInputProviderState(
    failure: AccessibilityProviderResult.Failure,
): CapabilityState = when {
    failure.requiredUserStep != null -> CapabilityState.NEEDS_USER_STEP
    failure.code == "capability_unavailable" -> CapabilityState.UNAVAILABLE
    else -> CapabilityState.DEGRADED
}

private fun staleDetectorGesture(message: String) = AccessibilityProviderResult.Failure(
    code = "stale_target",
    message = message,
    retryable = true,
    freshObservationRequired = true,
)
