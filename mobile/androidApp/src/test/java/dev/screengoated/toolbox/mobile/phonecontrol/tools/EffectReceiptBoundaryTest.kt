package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.InputInjectionEvidence
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Test

class EffectReceiptBoundaryTest {
    @Test
    fun sharedPayloadCasesCannotReplaceHostOwnedEffectEvidence() {
        val fixture = Json.parseToJsonElement(fixtureFile().readText()).jsonObject
        val placements = fixture.getValue("placements").jsonArray.map { it.jsonPrimitive.content }
        fixture.getValue("cases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val host = case.getValue("host").jsonObject
            val injection = host["input_injection"]?.jsonObject?.let {
                InputInjectionEvidence(
                    requested = it.getValue("requested").jsonPrimitive.long,
                    inserted = it.getValue("inserted").jsonPrimitive.long,
                    fullyInserted = it.getValue("fully_inserted").jsonPrimitive.boolean,
                )
            }
            val effect = EffectCertainty.fromSignals(
                effectVerified = host["effect_verified"]?.jsonPrimitive?.booleanOrNull == true,
                effectMayHaveOccurred = host["effect_may_have_occurred"]?.jsonPrimitive?.booleanOrNull,
                dispatchOk = host["dispatch_ok"]?.jsonPrimitive?.booleanOrNull,
                executed = host["executed"]?.jsonPrimitive?.booleanOrNull,
                inputInjection = injection,
            ).afterDispatch(case.getValue("mutating").jsonPrimitive.boolean)
            val payload = case.getValue("payload").jsonObject
            for (placement in placements + "flat") {
                val data = when (placement) {
                    "flat" -> payload
                    "items" -> JsonObject(mapOf(placement to JsonArray(listOf(payload))))
                    else -> JsonObject(mapOf(placement to payload))
                }
                val response = toolResponse(
                    job = PhoneControlToolJobContext(1, "job", 1),
                    requestedTool = "capability_action",
                    capability = "capability",
                    provider = "provider",
                    providerState = CapabilityState.READY,
                    code = "ok",
                    observationGeneration = 0,
                    effect = effect,
                    snapshotInvalidated = false,
                    data = data,
                )
                assertEquals(case.toString(), case.getValue("expected").jsonPrimitive.content,
                    response.getValue("effect_status").jsonPrimitive.content)
                assertEquals(effect.effectVerified, response.getValue("effect_verified").jsonPrimitive.boolean)
                if (effect.effectMayHaveOccurred == null) {
                    assertEquals(JsonNull, response.getValue("effect_may_have_occurred"))
                } else {
                    assertEquals(effect.effectMayHaveOccurred, response.getValue("effect_may_have_occurred").jsonPrimitive.boolean)
                }
                if (placement != "flat") assertEquals(data.getValue(placement), response.getValue(placement))
            }
        }
    }

    private fun fixtureFile(): File {
        var directory: File? = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (directory != null) {
            val candidate = File(directory, "parity-fixtures/phone-control/effect-receipt-boundary.json")
            if (candidate.isFile) return candidate
            directory = directory.parentFile
        }
        error("effect receipt boundary fixture not found")
    }
}
