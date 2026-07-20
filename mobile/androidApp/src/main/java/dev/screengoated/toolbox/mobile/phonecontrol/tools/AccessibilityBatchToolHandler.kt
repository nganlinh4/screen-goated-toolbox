package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityActionVerb
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlTargetIdentity
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

internal suspend fun handleDoSteps(
    job: PhoneControlToolJobContext,
    args: JsonObject,
    backend: AccessibilityToolBackend = AndroidAccessibilityToolBackend,
): PhoneControlToolExecution {
    val steps = args["steps"] as? JsonArray
        ?: return invalidArgs(job, "do_steps", "do_steps requires a steps array")
    if (steps.isEmpty() || steps.size > MAX_BATCH_STEPS) {
        return invalidArgs(job, "do_steps", "steps must contain 1 to $MAX_BATCH_STEPS actions")
    }
    val planned = steps.mapIndexed { index, element ->
        val step = element as? JsonObject
            ?: return invalidArgs(job, "do_steps", "step ${index + 1} is not an object")
        val id = step.int("id")
            ?: return invalidArgs(job, "do_steps", "step ${index + 1} has no integer id")
        val verb = parseBatchActionVerb(step.string("verb"))
            ?: return invalidArgs(job, "do_steps", "step ${index + 1} has no valid verb")
        val identity = backend.currentTargetIdentity(id)
            ?: return staleBatch(
                job = job,
                outcomes = emptyList(),
                requested = steps.size,
                requestedCapabilities = emptySet(),
                message = "Step ${index + 1} target is stale.",
                backend = backend,
            )
        PlannedBatchStep(step, identity, capabilityForVerb(verb))
    }
    val requestedCapabilities = planned.map(PlannedBatchStep::capability).toSet()
    val outcomes = mutableListOf<JsonObject>()
    var completed = 0
    for ((index, plannedStep) in planned.withIndex()) {
        val currentStep = if (index == 0) {
            plannedStep.arguments
        } else {
            val fresh = backend.observe()
            val observation = (fresh as? AccessibilityProviderResult.Success)?.value?.observation
                ?: return staleBatch(
                    job,
                    outcomes,
                    steps.size,
                    requestedCapabilities,
                    "Surface changed before step ${index + 1}.",
                    backend,
                )
            val equivalent = observation.elements.firstOrNull { element ->
                element.target.sameDurableNode(plannedStep.originalTarget)
            } ?: return staleBatch(
                job,
                outcomes,
                steps.size,
                requestedCapabilities,
                "Step ${index + 1} target no longer exists.",
                backend,
            )
            JsonObject(plannedStep.arguments + ("id" to JsonPrimitive(equivalent.id)))
        }
        val execution = handleAct(job, currentStep, "do_steps", backend)
        outcomes += execution.response
        if (execution.response["code"]?.jsonPrimitive?.contentOrNull != "ok") {
            return batchResult(
                job,
                outcomes,
                completed,
                steps.size,
                requestedCapabilities,
                stopped = true,
                backend = backend,
            )
        }
        completed += 1
    }
    return batchResult(
        job,
        outcomes,
        completed,
        steps.size,
        requestedCapabilities,
        stopped = false,
        backend = backend,
    )
}

private data class PlannedBatchStep(
    val arguments: JsonObject,
    val originalTarget: PhoneControlTargetIdentity,
    val capability: String,
)

private fun PhoneControlTargetIdentity.sameDurableNode(other: PhoneControlTargetIdentity): Boolean =
    packageOrSurface == other.packageOrSurface &&
        displayId == other.displayId &&
        windowId == other.windowId &&
        nodeOrDocumentIdentity == other.nodeOrDocumentIdentity &&
        bounds == other.bounds

private fun staleBatch(
    job: PhoneControlToolJobContext,
    outcomes: List<JsonObject>,
    requested: Int,
    requestedCapabilities: Set<String>,
    message: String,
    backend: AccessibilityToolBackend,
): PhoneControlToolExecution {
    val effect = aggregateBatchEffect(outcomes)
    val invalidated = effect.effectMayHaveOccurred == true
    return PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = "do_steps",
            capability = POINTER_CAPABILITY,
            provider = ACCESSIBILITY_PROVIDER,
            providerState = CapabilityState.READY,
            code = "stale_target",
            observationGeneration = backend.observationGeneration,
            effect = effect,
            snapshotInvalidated = invalidated,
            freshObservationRequired = true,
            data = batchData(outcomes, completedCount(outcomes), requested, true, requestedCapabilities, message),
        ),
        mutating = invalidated,
        refreshScreenFrame = true,
    )
}

private fun batchResult(
    job: PhoneControlToolJobContext,
    outcomes: List<JsonObject>,
    completed: Int,
    requested: Int,
    requestedCapabilities: Set<String>,
    stopped: Boolean,
    backend: AccessibilityToolBackend,
): PhoneControlToolExecution {
    val effect = aggregateBatchEffect(outcomes)
    val invalidated = effect.effectMayHaveOccurred == true
    return PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = "do_steps",
            capability = POINTER_CAPABILITY,
            provider = ACCESSIBILITY_PROVIDER,
            providerState = CapabilityState.READY,
            code = if (stopped) "partial" else "ok",
            observationGeneration = backend.observationGeneration,
            effect = effect,
            snapshotInvalidated = invalidated,
            freshObservationRequired = invalidated,
            data = batchData(outcomes, completed, requested, stopped, requestedCapabilities),
        ),
        mutating = invalidated,
        refreshScreenFrame = invalidated,
    )
}

private fun batchData(
    outcomes: List<JsonObject>,
    completed: Int,
    requested: Int,
    stopped: Boolean,
    requestedCapabilities: Set<String>,
    message: String? = null,
): JsonObject = buildJsonObject {
    put("completed", completed)
    put("attempted", outcomes.size)
    put("requested", requested)
    put("stopped", stopped)
    put(
        "capabilities",
        buildJsonArray {
            requestedCapabilities.sorted().forEach { add(JsonPrimitive(it)) }
        },
    )
    put("results", buildJsonArray { outcomes.forEach(::add) })
    message?.let { put("message", it) }
}

private fun completedCount(outcomes: List<JsonObject>): Int = outcomes.count { outcome ->
    outcome["code"]?.jsonPrimitive?.contentOrNull == "ok"
}

private fun aggregateBatchEffect(outcomes: List<JsonObject>): EffectCertainty {
    if (outcomes.isEmpty()) return EffectCertainty.PROVEN_NO_EFFECT
    val statuses = outcomes.mapNotNull { outcome ->
        outcome["effect_status"]?.jsonPrimitive?.contentOrNull
    }.toSet()
    return when {
        EffectCertainty.VERIFIED.wireName in statuses -> EffectCertainty.VERIFIED
        EffectCertainty.MAY_HAVE_OCCURRED.wireName in statuses -> EffectCertainty.MAY_HAVE_OCCURRED
        statuses.isNotEmpty() && statuses.all { it == EffectCertainty.PROVEN_NO_EFFECT.wireName } ->
            EffectCertainty.PROVEN_NO_EFFECT
        else -> EffectCertainty.UNKNOWN
    }
}

private fun parseBatchActionVerb(value: String?): AccessibilityActionVerb? = value
    ?.uppercase()
    ?.let { name -> runCatching { AccessibilityActionVerb.valueOf(name) }.getOrNull() }

private const val ACCESSIBILITY_PROVIDER = "accessibility"
private const val POINTER_CAPABILITY = "ui.pointer_action"
private const val MAX_BATCH_STEPS = 8
