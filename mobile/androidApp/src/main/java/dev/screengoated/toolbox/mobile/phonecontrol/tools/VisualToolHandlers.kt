package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.VisualFrame
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.GRID_COLUMNS
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.GRID_ROWS
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.VisualProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class VisualToolHandlers(
    private val backend: VisualToolBackend = AndroidVisualToolBackend,
) {
    suspend fun zoom(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val cell = args.int("cell")
            ?: return invalidArgs(job, "zoom", "zoom requires integer cell")
        if (cell !in 1..GRID_COLUMNS * GRID_ROWS) {
            return invalidArgs(job, "zoom", "cell must be between 1 and ${GRID_COLUMNS * GRID_ROWS}")
        }
        return result(job, "zoom", backend.zoom(cell))
    }

    suspend fun resetView(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        result(job, "reset_view", backend.resetView())

    suspend fun seeWholeScreen(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        result(job, "see_whole_screen", backend.seeWholeScreen())

    suspend fun look(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val question = args.string("question")?.trim()?.takeIf(String::isNotEmpty)
            ?: return invalidArgs(job, "look", "look requires question")
        if (question.length > MAX_QUESTION_CHARS) {
            return invalidArgs(job, "look", "question is too long")
        }
        return result(job, "look", backend.look(), question)
    }

    private fun result(
        job: PhoneControlToolJobContext,
        tool: String,
        result: VisualProviderResult<VisualFrame>,
        question: String? = null,
    ): PhoneControlToolExecution = when (result) {
        is VisualProviderResult.Failure -> failure(job, tool, result)
        is VisualProviderResult.Success -> success(job, tool, result.value, question)
    }

    private fun success(
        job: PhoneControlToolJobContext,
        tool: String,
        frame: VisualFrame,
        question: String?,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = tool,
            capability = VISUAL_CAPABILITY,
            provider = VISUAL_PROVIDER,
            providerState = CapabilityState.READY,
            code = "ok",
            observationGeneration = frame.identity.observationGeneration,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            data = buildJsonObject {
                question?.let { put("question", it) }
                put("frame", frame.identity.toWireJson())
                put(
                    "model_instruction",
                    if (tool == "look") {
                        "Answer the question from the clean current frame delivered before this receipt."
                    } else {
                        "The generation-bound numbered frame was delivered before this receipt."
                    },
                )
            },
        ),
        mutating = false,
        screenFramePayload = frame.screenPayload,
    )

    private fun failure(
        job: PhoneControlToolJobContext,
        tool: String,
        failure: VisualProviderResult.Failure,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = tool,
            capability = VISUAL_CAPABILITY,
            provider = VISUAL_PROVIDER,
            providerState = when {
                failure.requiredUserStep != null -> CapabilityState.NEEDS_USER_STEP
                failure.code == "unsupported_display" -> CapabilityState.UNSUPPORTED
                else -> CapabilityState.DEGRADED
            },
            code = failure.code,
            observationGeneration = backend.observationGeneration,
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

    private companion object {
        const val VISUAL_CAPABILITY = "ui.visual_observe"
        const val VISUAL_PROVIDER = "accessibility"
        const val MAX_QUESTION_CHARS = 1_000
    }
}
