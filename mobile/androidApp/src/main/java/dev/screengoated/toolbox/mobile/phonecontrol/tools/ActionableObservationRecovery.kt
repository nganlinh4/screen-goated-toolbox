package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

internal fun interface ActionableObservationRecovery {
    suspend fun recover(execution: PhoneControlToolExecution): PhoneControlToolExecution
}

internal object NoOpActionableObservationRecovery : ActionableObservationRecovery {
    override suspend fun recover(execution: PhoneControlToolExecution): PhoneControlToolExecution =
        execution
}

internal class AndroidActionableObservationRecovery(
    private val backend: AccessibilityToolBackend = AndroidAccessibilityToolBackend,
) : ActionableObservationRecovery {
    override suspend fun recover(
        execution: PhoneControlToolExecution,
    ): PhoneControlToolExecution {
        val response = execution.response
        val code = response["code"]?.jsonPrimitive?.contentOrNull
        val noEffect = response["effect_status"]?.jsonPrimitive?.contentOrNull ==
            EffectCertainty.PROVEN_NO_EFFECT.wireName
        if (!noEffect || code !in RECOVERABLE_CODES) return execution

        val fresh = when (val observed = backend.observe()) {
            is AccessibilityProviderResult.Failure -> return execution
            is AccessibilityProviderResult.Success -> observed.value
        }
        val attemptedGeneration = response["observation_generation"]
            ?.jsonPrimitive
            ?.longOrNull
        val recovered = response.toMutableMap().apply {
            putAll(observationData(fresh))
            put("observation_generation", JsonPrimitive(fresh.observation.generation))
            put("state_reconciled", JsonPrimitive(true))
            put("fresh_observation_attached", JsonPrimitive(true))
            put("fresh_observation_required", JsonPrimitive(false))
            put("retryable", JsonPrimitive(true))
            put("retry_instruction", JsonPrimitive(RETRY_INSTRUCTION))
            attemptedGeneration?.let { put("attempted_observation_generation", JsonPrimitive(it)) }
        }
        check(recovered["effect_may_have_occurred"]?.jsonPrimitive?.booleanOrNull == false)
        return execution.copy(
            response = JsonObject(recovered),
            refreshScreenFrame = true,
        )
    }

    private companion object {
        val RECOVERABLE_CODES = setOf("stale_target", "structured_surface_available")
        const val RETRY_INSTRUCTION =
            "Use only identities from the attached current observation for at most one retry."
    }
}
