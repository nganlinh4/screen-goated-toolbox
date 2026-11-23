package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityActionOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class DoStepsStopCodeTest {
    @Test
    fun osOwnedConfirmationStepIsPromotedWithItsUserStep() = runTest {
        val backend = twoStepBackend()
        backend.actionOutcomes += verifiedOk()
        backend.actionOutcomes += AccessibilityProviderResult.Failure(
            code = "os_owned_confirmation",
            message = "Android owns this step.",
            retryable = true,
            requiredUserStep = "complete_os_owned_confirmation",
        )

        val response = handleDoSteps(AccessibilityToolHandlersTest.JOB, twoSteps(), backend).response

        assertEquals("os_owned_confirmation", response.text("code"))
        assertEquals(
            "complete_os_owned_confirmation",
            response.getValue("required_user_step").jsonObject.text("code"),
        )
        assertTrue(response.getValue("retryable").jsonPrimitive.boolean)
        assertTrue(response.getValue("stopped").jsonPrimitive.boolean)
        assertEquals(1, response.getValue("completed").jsonPrimitive.content.toInt())
        assertEquals(
            "os_owned_confirmation",
            response.getValue("results").jsonArray[1].jsonObject.text("code"),
        )
        assertEquals("verified", response.text("effect_status"))
    }

    @Test
    fun staleStepIsPromotedAndRequiresFreshObservation() = runTest {
        val backend = twoStepBackend()
        backend.actionOutcomes += AccessibilityProviderResult.Failure(
            code = "stale_target",
            message = "Target moved.",
            retryable = true,
        )
        val args = buildJsonObject {
            put("steps", buildJsonArray { add(AccessibilityToolHandlersTest.jsonArgs("id" to 1, "verb" to "click")) })
        }

        val execution = handleDoSteps(AccessibilityToolHandlersTest.JOB, args, backend)

        assertEquals("stale_target", execution.response.text("code"))
        assertEquals("proven_no_effect", execution.response.text("effect_status"))
        assertTrue(execution.response.getValue("fresh_observation_required").jsonPrimitive.boolean)
        assertTrue(execution.refreshScreenFrame)
    }

    @Test
    fun ordinaryFailedStepStaysPartial() {
        val ordinary = buildJsonObject {
            put("code", "action_rejected")
            put("retryable", false)
        }
        assertNull(promotedStopCode(ordinary))
        assertNull(promotedStopCode(null))
    }

    private fun twoStepBackend(): AccessibilityToolHandlersTest.FakeAccessibilityBackend {
        val first = AccessibilityToolHandlersTest.target(id = 1, generation = 3)
        val second = AccessibilityToolHandlersTest.target(id = 2, generation = 3)
        val freshSecond = second.copy(snapshotGeneration = 4, observationTimestampMs = 4)
        return AccessibilityToolHandlersTest.FakeAccessibilityBackend(
            AccessibilityObservationFrame(
                AccessibilityToolHandlersTest.observation(
                    generation = 4,
                    elements = listOf(AccessibilityToolHandlersTest.element(id = 22, target = freshSecond)),
                ),
            ),
        ).apply { targets += mapOf(1 to first, 2 to second) }
    }

    private fun twoSteps(): JsonObject = buildJsonObject {
        put(
            "steps",
            buildJsonArray {
                add(AccessibilityToolHandlersTest.jsonArgs("id" to 1, "verb" to "click"))
                add(AccessibilityToolHandlersTest.jsonArgs("id" to 2, "verb" to "click"))
            },
        )
    }

    private fun verifiedOk() = AccessibilityProviderResult.Success(
        AccessibilityActionOutcome(
            code = "ok",
            generation = 4,
            effect = EffectCertainty.VERIFIED,
            snapshotInvalidated = true,
            freshObservationRequired = true,
        ),
    )

    private fun JsonObject.text(name: String): String = getValue(name).jsonPrimitive.content
}
