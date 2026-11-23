package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlResourceAuthorizer
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.unavailableResourceBoundary
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlTurnRecorder
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class PhoneControlResourceBoundary(
    private val guard: PhoneControlExactEditGuard,
    private val authorizer: PhoneControlResourceAuthorizer,
    private val declarationFor: (String) -> JsonObject?,
    private val observedContext: () -> JsonObject,
) : AutoCloseable, PhoneControlTurnRecorder by guard {
    suspend fun beforeAction(job: PhoneControlToolJobContext, tool: String, args: JsonObject): PhoneControlToolExecution? {
        guard.beforeAction(job, tool, args)?.let { return rejection(job, tool, it.code, it.message) }
        return null
    }

    suspend fun authorize(job: PhoneControlToolJobContext, tool: String, args: JsonObject): BoundaryAuthorization {
        currentCoroutineContext().ensureActive()
        if (job.effectOwner.isCancellationRequested || !guard.isCurrent(job)) return BoundaryAuthorization(rejected = cancelled(job, tool))
        if (!guard.needsScopeCheck(tool, resourceEffectful(tool))) return BoundaryAuthorization()
        val declaration = declarationFor(tool)
        if (declaration?.string("name") != tool || declaration["parameters"] !is JsonObject) {
            return unavailable(job, tool, "The selected tool's validated session declaration is unavailable.")
        }
        val observed = observedContext()
        val process = if (tool == "run_command") runCatching { parseCommandRequest(args) }.getOrElse {
            return unavailable(job, tool, it.message ?: "The exact process proposal is unavailable.")
        }.proposal() else null
        val proposal = buildJsonObject {
            put("operation", "preserve_rejected_exact_resource_boundaries")
            put("protected_resources", guard.protectedResources())
            put("proposed_action", buildJsonObject { put("tool", tool); put("arguments", args) })
            put("evidence_untrusted", buildJsonObject {
                put("tool_declaration", declaration)
                process?.let { put("process", it) }
                guard.pendingIdentity()?.let { put("dedicated_file_target", it) }
                put("observed_world", observed)
            })
        }
        val decision = if (proposal.toString().toByteArray(Charsets.UTF_8).size > 96 * 1024) {
            unavailableResourceBoundary("The complete effect proposal exceeds its bounded budget.")
        } else authorizer.evaluateBoundary(proposal)
        currentCoroutineContext().ensureActive()
        if (job.effectOwner.isCancellationRequested || !guard.isCurrent(job)) return BoundaryAuthorization(rejected = cancelled(job, tool))
        if (observedContext() != observed) return unavailable(job, tool, "The observed action context changed during assessment.")
        return if (decision.authorized) BoundaryAuthorization(evidence = decision.data) else BoundaryAuthorization(
            rejected = rejection(job, tool, decision.code, decision.message, decision.data),
        )
    }

    fun finish(job: PhoneControlToolJobContext, tool: String, args: JsonObject, execution: PhoneControlToolExecution,
               authorization: BoundaryAuthorization): PhoneControlToolExecution {
        val response = guard.recordResult(job, tool, args, execution.response)
        val evidence = authorization.evidence
        return execution.copy(response = if (evidence == null) response else JsonObject(
            response + ("resource_boundary_authorization" to evidence),
        ))
    }

    override fun close() = guard.close()

    private fun cancelled(job: PhoneControlToolJobContext, tool: String) =
        rejection(job, tool, "ERR_CANCELLED", "The owning job was cancelled before dispatch.")

    private fun unavailable(job: PhoneControlToolJobContext, tool: String, reason: String) =
        BoundaryAuthorization(rejected = rejection(job, tool, "ERR_EXACT_EDIT_EFFECT_SCOPE_UNVERIFIED", reason))

    private fun rejection(job: PhoneControlToolJobContext, tool: String, code: String, message: String,
                          evidence: JsonObject? = null) = PhoneControlToolExecution(
        response = toolResponse(job, tool, "tool_contract", "android_app_api", CapabilityState.READY,
            code, 0, EffectCertainty.PROVEN_NO_EFFECT, snapshotInvalidated = false,
            retryable = true, data = buildJsonObject {
                put("message", message)
                evidence?.let { put("resource_boundary_authorization", it) }
            }),
        mutating = false,
    )
}

internal data class BoundaryAuthorization(val evidence: JsonObject? = null, val rejected: PhoneControlToolExecution? = null)

internal fun resourceEffectful(tool: String): Boolean = when (PhoneControlToolRegistry.byName[tool]?.handler) {
    PhoneControlHandler.EXTRACT_ARTIFACT -> false
    PhoneControlHandler.BROWSER_SETUP, PhoneControlHandler.BROWSER_RESET, null -> true
    else -> PhoneControlToolRegistry.byName.getValue(tool).handler!!.mutating
}

internal fun androidResourceBoundaryContext(): JsonObject = buildJsonObject {
    put("observation_generation", PhoneControlAccessibilityProvider.observationGeneration)
    put("visual_revision", PhoneControlAccessibilityProvider.currentVisualRevision)
    val observation = PhoneControlAccessibilityProvider.currentObservation()
    if (observation != null) {
        val text = observation.toModelText()
        put("observed_controls", text.take(16_384))
        put("observation_truncated", observation.truncated || text.length > 16_384)
    }
    put("unobserved_browser_or_process_state", "unknown")
}

