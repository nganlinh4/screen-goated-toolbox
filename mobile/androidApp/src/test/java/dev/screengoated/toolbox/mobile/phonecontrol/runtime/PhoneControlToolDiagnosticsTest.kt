package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlToolDiagnosticsTest {
    @Test
    fun `dispatch diagnostics keep shape and omit argument values`() {
        val call = GeminiLiveFunctionCall(
            id = "fc_1",
            name = "act",
            args = buildJsonObject {
                put("id", 7)
                put("value", "private input")
            },
        )

        val log = call.structuralDispatchLog(generation = 4, turnId = 3)

        assertTrue(log.contains("turn_id=3"))
        assertTrue(log.contains("job_id=fc_1"))
        assertTrue(log.contains("argument_fields=2"))
        assertTrue(log.contains("argument_keys=id:value"))
        assertTrue(log.contains("argument_bytes="))
        assertFalse(log.contains("private input"))
    }

    @Test
    fun `receipt diagnostics expose effect and recovery structure without message prose`() {
        val response = buildJsonObject {
            put("code", "stale_target")
            put("capability", "semantic_action")
            put("provider", "android_accessibility")
            put("provider_state", "degraded")
            put("failure_class", "handler")
            put("provider_route_error", "provider_not_ready")
            put("argument_field", "target")
            put("contract_reason", "invalid_surface_identity")
            put("grounding_stage", "pixel_revalidation")
            put("mapping_model_ms", 4100)
            put("pixel_revalidation_ms", 83)
            put("observation_generation", 9)
            put("attempted_observation_generation", 8)
            put("attempted_target_id", 17)
            put("effect_status", "proven_no_effect")
            put("snapshot_invalidated", false)
            put("fresh_observation_attached", true)
            put("message", "private model-facing detail")
        }
        val completed = PhoneControlCompletedTool(
            request = PhoneControlToolRequest(
                id = "fc_2",
                name = "act",
                arguments = buildJsonObject {},
                turnId = 6,
                generation = 8,
            ),
            result = PhoneControlToolExecutionResult(
                response = response,
                certainty = PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
            ),
            elapsedMs = 125,
        )

        val log = completed.structuralReceiptLog()

        assertTrue(log.contains("elapsed_ms=125"))
        assertTrue(log.contains("code=stale_target"))
        assertTrue(log.contains("failure_class=handler"))
        assertTrue(log.contains("provider_route_error=provider_not_ready"))
        assertTrue(log.contains("argument_field=target"))
        assertTrue(log.contains("contract_reason=invalid_surface_identity"))
        assertTrue(log.contains("grounding_stage=pixel_revalidation"))
        assertTrue(log.contains("mapping_model_ms=4100"))
        assertTrue(log.contains("pixel_revalidation_ms=83"))
        assertTrue(log.contains("attempted_target_id=17"))
        assertTrue(log.contains("fresh_observation_attached=true"))
        assertFalse(log.contains("private model-facing detail"))
    }

    @Test
    fun `dispatch diagnostics make unsafe provider identities opaque`() {
        val call = GeminiLiveFunctionCall(
            id = "123/private value",
            name = "observe",
            args = buildJsonObject {},
        )

        val log = call.structuralDispatchLog(generation = 2, turnId = 1)

        assertTrue(log.contains("job_id=opaque_"))
        assertFalse(log.contains("123/private value"))
    }
}
