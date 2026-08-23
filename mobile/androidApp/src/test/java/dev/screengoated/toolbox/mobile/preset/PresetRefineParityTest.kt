package dev.screengoated.toolbox.mobile.preset

import org.junit.Assert.assertEquals
import org.junit.Test

class PresetRefineParityTest {
    @Test
    fun `refine starts with actual successful text endpoint`() {
        val model = initialRefineModel(
            originalModelId = "nvidia-nemotron-3-5-lightning-text",
            settings = PresetRuntimeSettings(),
            apiKeys = ApiKeys(nvidiaKey = "test"),
        )

        assertEquals("nvidia-nemotron-3-5-lightning-text", model?.id)
    }

    @Test
    fun `non text result refines through the adaptive text chain`() {
        val settings = PresetRuntimeSettings(
            adaptiveModelPriority = PresetAdaptiveModelPriority(
                imageToText = false,
                textToText = false,
            ),
            modelPriorityChains = PresetModelPriorityChains(
                textToText = listOf("groq-gpt-oss-20b-text"),
            ),
        )
        val model = initialRefineModel(
            originalModelId = "google-gemini-3-5-flash-lite-vision",
            settings = settings,
            apiKeys = ApiKeys(groqKey = "test"),
        )

        assertEquals("groq-gpt-oss-20b-text", model?.id)
    }
}
