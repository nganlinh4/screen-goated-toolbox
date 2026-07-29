package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityMutationKind
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.surfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.grounding.VisualGroundingProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.grounding.VisualGroundingResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.grounding.VisualGroundingVerifiedMark
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class VisualGroundingToolHandlers(context: Context) {
    private val provider = VisualGroundingProvider(context)
    private val elevatedInput: ElevatedPointerInput = AndroidElevatedPointerInput(context)

    suspend fun mapTargets(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val description = description(args, "description")
            ?: return invalidArgs(job, "map_targets", "map_targets requires description")
        if (!description.fitsVisualDescriptionLimit()) {
            return invalidArgs(job, "map_targets", "description is too long")
        }
        return when (val result = provider.mapCurrentSurface(description, "")) {
            is VisualGroundingResult.Failure -> visualGroundingFailure(
                job,
                "map_targets",
                result,
                provider.observationGeneration,
                "mapping",
            )
            is VisualGroundingResult.Success ->
                visualMappingExecution(job, description, result.value)
        }
    }

    suspend fun clickTarget(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val description = description(args, "description")
            ?: return invalidArgs(job, "click_target", "click_target requires description")
        if (!description.fitsVisualDescriptionLimit()) {
            return invalidArgs(job, "click_target", "description is too long")
        }
        val button = args.string("button") ?: "left"
        if (button !in SUPPORTED_VISUAL_BUTTONS) {
            return invalidArgs(job, "click_target", "button must be left or right")
        }
        val located = when (val result = provider.locate(description, "")) {
            is VisualGroundingResult.Failure -> return visualGroundingFailure(
                job,
                "click_target",
                result,
                provider.observationGeneration,
                "grounding",
            )
            is VisualGroundingResult.Success -> result.value
        }
        return executeClick(job, "click_target", button, located)
    }

    suspend fun clickMark(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val id = args.int("mark")
            ?: return invalidArgs(job, "click_mark", "click_mark requires integer mark")
        val button = args.string("button") ?: "left"
        if (button !in SUPPORTED_VISUAL_BUTTONS) {
            return invalidArgs(job, "click_mark", "button must be left or right")
        }
        val refreshed = when (val result = provider.refreshMark(id)) {
            is VisualGroundingResult.Failure -> return visualGroundingFailure(
                job,
                "click_mark",
                result,
                provider.observationGeneration,
                "pixel_revalidation",
            )
            is VisualGroundingResult.Success -> result.value
        }
        return dispatchClick(job, "click_mark", button, refreshed)
    }

    suspend fun dragTarget(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val fromDescription = description(args, "from")
            ?: return invalidArgs(job, "drag_target", "drag_target requires from")
        val toDescription = description(args, "to")
            ?: return invalidArgs(job, "drag_target", "drag_target requires to")
        if (!fromDescription.fitsVisualDescriptionLimit() ||
            !toDescription.fitsVisualDescriptionLimit()
        ) {
            return invalidArgs(job, "drag_target", "drag endpoint description is too long")
        }
        val located = when (
            val result = provider.locateDrag(fromDescription, toDescription, "")
        ) {
            is VisualGroundingResult.Failure -> return visualGroundingFailure(
                job,
                "drag_target",
                result,
                provider.observationGeneration,
                "grounding",
            )
            is VisualGroundingResult.Success -> result.value
        }
        val final = when (val result = provider.revalidateMarks(listOf(located.first, located.second))) {
            is VisualGroundingResult.Failure -> return visualGroundingFailure(
                job,
                "drag_target",
                result,
                provider.observationGeneration,
                "pixel_revalidation",
            )
            is VisualGroundingResult.Success -> result.value
        }
        val from = final.mark(located.first.mark.id)
        val to = final.mark(located.second.mark.id)
        if (from == null || to == null) {
            return visualGroundingFailure(
                job,
                "drag_target",
                staleVisualGesture("The final frame omitted a drag endpoint."),
                provider.observationGeneration,
                "pixel_revalidation",
            )
        }
        return try {
            val accessibility = dragWithAccessibility(from, to)
            val action = routePointerInput(accessibility, { provider.observationGeneration }) {
                elevatedInput.swipe(
                    job = job,
                    lease = from.frame.lease,
                    fromX = from.mark.point.centerX.toFloat(),
                    fromY = from.mark.point.centerY.toFloat(),
                    toX = to.mark.point.centerX.toFloat(),
                    toY = to.mark.point.centerY.toFloat(),
                    durationMs = DRAG_DURATION_MS,
                    kind = AccessibilityMutationKind.POINTER_ACTIVATE,
                    expectedVisualRevision = from.frame.identity.visualRevision,
                )
            }
            visualPointerExecution(
                job,
                "drag_target",
                action,
                buildJsonObject {
                    putEndpoint("from", from)
                    putEndpoint("to", to)
                    put("target_location_ms", maxOf(from.groundingMs, to.groundingMs))
                    put(
                        "target_verification_ms",
                        from.verificationMs + to.verificationMs,
                    )
                    put("pixel_revalidation_ms", final.pixelRevalidationMs)
                },
            )
        } finally {
            provider.clearMarks()
        }
    }

    private suspend fun executeClick(
        job: PhoneControlToolJobContext,
        requestedTool: String,
        button: String,
        located: VisualGroundingVerifiedMark,
    ): PhoneControlToolExecution {
        val final = when (val result = provider.revalidateMarks(listOf(located))) {
            is VisualGroundingResult.Failure -> {
                provider.clearMarks()
                return visualGroundingFailure(
                    job,
                    requestedTool,
                    result,
                    provider.observationGeneration,
                    "pixel_revalidation",
                )
            }
            is VisualGroundingResult.Success -> result.value.marks.single()
        }
        return dispatchClick(job, requestedTool, button, final)
    }

    private suspend fun dispatchClick(
        job: PhoneControlToolJobContext,
        requestedTool: String,
        button: String,
        target: VisualGroundingVerifiedMark,
    ): PhoneControlToolExecution {
        try {
            validateCurrentLease(target.frame.lease)?.let { failure ->
                return visualGroundingFailure(
                    job,
                    requestedTool,
                    failure,
                    provider.observationGeneration,
                    "pre_dispatch",
                )
            }
            val point = target.mark.point
            val accessibility = if (button == "right") {
                PhoneControlAccessibilityProvider.swipe(
                    target.frame.lease,
                    point.centerX.toFloat(),
                    point.centerY.toFloat(),
                    point.centerX.toFloat(),
                    point.centerY.toFloat(),
                    VISUAL_LONG_PRESS_MS,
                    AccessibilityMutationKind.LONG_PRESS,
                    expectedVisualRevision = target.frame.identity.visualRevision,
                )
            } else {
                PhoneControlAccessibilityProvider.click(
                    target.frame.lease,
                    point.centerX.toFloat(),
                    point.centerY.toFloat(),
                    expectedVisualRevision = target.frame.identity.visualRevision,
                )
            }
            val action = routePointerInput(
                accessibility,
                { provider.observationGeneration },
            ) {
                elevatedInput.tap(
                    job = job,
                    lease = target.frame.lease,
                    x = point.centerX.toFloat(),
                    y = point.centerY.toFloat(),
                    expectedVisualRevision = target.frame.identity.visualRevision,
                    holdMs = VISUAL_LONG_PRESS_MS.takeIf { button == "right" },
                )
            }
            return visualPointerExecution(
                job,
                requestedTool,
                action,
                buildJsonObject {
                    put("clicked_mark", target.mark.id)
                    put("button", button)
                    put("screen_x", point.centerX)
                    put("screen_y", point.centerY)
                    put("saw_at_target", point.label)
                    put("grounding_model", point.modelId)
                    put("target_location_ms", target.groundingMs)
                    put("target_verification_ms", target.verificationMs)
                    put("pixel_revalidation_ms", target.pixelRevalidationMs)
                    target.verificationModelId?.let { put("verification_model", it) }
                    target.verificationConfidence?.let { put("verification_confidence", it) }
                    target.verificationWhat?.let { put("verified_at_target", it) }
                },
            )
        } finally {
            provider.clearMarks()
        }
    }

    private suspend fun dragWithAccessibility(
        from: VisualGroundingVerifiedMark,
        to: VisualGroundingVerifiedMark,
    ): AccessibilityProviderResult<dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome> {
        if (from.frame.lease != to.frame.lease ||
            from.frame.identity.visualRevision != to.frame.identity.visualRevision
        ) {
            return AccessibilityProviderResult.Failure(
                code = "stale_target",
                message = "The drag endpoints do not share one current frame.",
                retryable = true,
                freshObservationRequired = true,
            )
        }
        return PhoneControlAccessibilityProvider.swipe(
            lease = from.frame.lease,
            fromX = from.mark.point.centerX.toFloat(),
            fromY = from.mark.point.centerY.toFloat(),
            toX = to.mark.point.centerX.toFloat(),
            toY = to.mark.point.centerY.toFloat(),
            durationMs = DRAG_DURATION_MS,
            kind = AccessibilityMutationKind.POINTER_ACTIVATE,
            expectedVisualRevision = from.frame.identity.visualRevision,
        )
    }

    private suspend fun validateCurrentLease(
        lease: AccessibilitySurfaceLease,
    ): VisualGroundingResult.Failure? {
        val observation = when (
            val result = PhoneControlAccessibilityProvider.observe(maxElements = 1)
        ) {
            is AccessibilityProviderResult.Failure -> return VisualGroundingResult.Failure(
                result.code,
                result.message,
                result.retryable,
                result.requiredUserStep,
                result.freshObservationRequired,
            )
            is AccessibilityProviderResult.Success -> result.value
        }
        return if (observation.surfaceLease(lease.displayId, lease.windowId) == lease) {
            null
        } else {
            staleVisualGesture("The verified visual surface changed before input dispatch.")
        }
    }
}

private fun description(args: JsonObject, name: String): String? =
    args.string(name)?.trim()?.takeIf(String::isNotEmpty)

private fun String.fitsVisualDescriptionLimit(): Boolean =
    codePointCount(0, length) <= MAX_VISUAL_DESCRIPTION_CHARS

private fun kotlinx.serialization.json.JsonObjectBuilder.putEndpoint(
    prefix: String,
    mark: VisualGroundingVerifiedMark,
) {
    put("${prefix}_mark", mark.mark.id)
    put("${prefix}_screen_x", mark.mark.point.centerX)
    put("${prefix}_screen_y", mark.mark.point.centerY)
    put("${prefix}_grounding_model", mark.mark.point.modelId)
    mark.verificationModelId?.let { put("${prefix}_verification_model", it) }
    mark.verificationConfidence?.let { put("${prefix}_verification_confidence", it) }
}

private const val DRAG_DURATION_MS = 550L
