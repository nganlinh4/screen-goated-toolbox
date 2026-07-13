package dev.screengoated.toolbox.mobile.shared.live

import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull

class GeminiLiveSetupTest {
    @Test
    fun endpointPolicyIsAppliedByConstruction() {
        val cases = listOf(
            Triple(GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_2_5, 8192, "thinkingBudget"),
            Triple(GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_3_1, 65536, "thinkingLevel"),
        )

        cases.forEach { (model, limit, thinkingKey) ->
            val generation = buildGeminiLiveSetup(GeminiLiveSetupSpec(apiModel = model))
                .getValue("setup").jsonObject
                .getValue("generationConfig").jsonObject
            assertEquals(limit, generation.getValue("maxOutputTokens").jsonPrimitive.content.toInt())
            assertNotNull(generation.getValue("thinkingConfig").jsonObject[thinkingKey])
        }
    }

    @Test
    fun featureFieldsRemainExplicit() {
        val setup = buildGeminiLiveSetup(
            GeminiLiveSetupSpec(
                apiModel = GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_3_1,
                mediaResolution = GeminiLiveMediaResolution.HIGH,
                voiceName = "Aoede",
                systemInstruction = "instruction",
                transcriptionMode = GeminiLiveTranscriptionMode.BOTH,
                contextWindowCompression = true,
            ),
        ).getValue("setup").jsonObject

        assertEquals(
            "MEDIA_RESOLUTION_HIGH",
            setup.getValue("generationConfig").jsonObject
                .getValue("mediaResolution").jsonPrimitive.content,
        )
        assertNotNull(setup["inputAudioTranscription"])
        assertNotNull(setup["outputAudioTranscription"])
        assertNotNull(setup["contextWindowCompression"])
    }

    @Test
    fun genericExtensionsCannotReplaceEndpointIdentityOrOutputPolicy() {
        val setup = buildGeminiLiveSetup(
            GeminiLiveSetupSpec(
                apiModel = GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_3_1,
                generationOverrides = buildJsonObject { put("maxOutputTokens", 1) },
                setupExtensions = buildJsonObject { put("model", "models/wrong") },
            ),
        ).getValue("setup").jsonObject

        assertEquals(
            "models/${GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_3_1}",
            setup.getValue("model").jsonPrimitive.content,
        )
        assertEquals(
            "65536",
            setup.getValue("generationConfig").jsonObject
                .getValue("maxOutputTokens").jsonPrimitive.content,
        )
    }
}
