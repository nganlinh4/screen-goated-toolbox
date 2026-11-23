package dev.screengoated.toolbox.mobile.phonecontrol.tools

import java.io.File
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityActionOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class BatchEffectCertaintyTest {
    @Test
    fun verifiedFillThenUncertainClickRequiresReconciliation() = runTest {
        val first = AccessibilityToolHandlersTest.target(id = 1, generation = 3)
        val second = AccessibilityToolHandlersTest.target(id = 2, generation = 3)
        val freshSecond = second.copy(snapshotGeneration = 4, observationTimestampMs = 4)
        val backend = AccessibilityToolHandlersTest.FakeAccessibilityBackend(
            AccessibilityObservationFrame(
                AccessibilityToolHandlersTest.observation(
                    generation = 4,
                    elements = listOf(AccessibilityToolHandlersTest.element(id = 22, target = freshSecond)),
                ),
            ),
        )
        backend.targets += mapOf(1 to first, 2 to second)
        for (effect in listOf(EffectCertainty.VERIFIED, EffectCertainty.MAY_HAVE_OCCURRED)) {
            backend.actionOutcomes += AccessibilityProviderResult.Success(
                AccessibilityActionOutcome(
                    code = "ok", generation = 4, effect = effect,
                    snapshotInvalidated = true, freshObservationRequired = true,
                ),
            )
        }
        val args = buildJsonObject {
            put("steps", buildJsonArray {
                add(AccessibilityToolHandlersTest.jsonArgs("id" to 1, "verb" to "fill", "value" to "hello"))
                add(AccessibilityToolHandlersTest.jsonArgs("id" to 2, "verb" to "click"))
            })
        }
        val execution = handleDoSteps(AccessibilityToolHandlersTest.JOB, args, backend)
        assertEquals("may_have_occurred", execution.response.getValue("effect_status").jsonPrimitive.content)
        assertEquals(false, execution.response.getValue("effect_verified").jsonPrimitive.boolean)
        assertTrue(execution.response.getValue("fresh_observation_required").jsonPrimitive.boolean)
        assertTrue(execution.mutating)
    }

    @Test
    fun sharedCasesDoNotLetVerifiedStepsMaskUncertainty() {
        val fixture = Json.parseToJsonElement(fixtureFile().readText()).jsonObject
        fixture.getValue("cases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val outcomes = case.getValue("statuses").jsonArray.map { status ->
                if (status.toString() == "null") JsonObject(emptyMap())
                else JsonObject(mapOf("effect_status" to status))
            }
            assertEquals(case.toString(), case.getValue("expected").jsonPrimitive.content, aggregateBatchEffect(outcomes).wireName)
        }
    }

    private fun fixtureFile(): File {
        var directory: File? = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (directory != null) {
            val candidate = File(directory, "parity-fixtures/phone-control/batch-effect-certainty.json")
            if (candidate.isFile) return candidate
            directory = directory.parentFile
        }
        error("batch effect fixture not found")
    }
}
