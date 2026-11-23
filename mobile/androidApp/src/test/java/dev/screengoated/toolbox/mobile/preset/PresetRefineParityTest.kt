package dev.screengoated.toolbox.mobile.preset

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PresetRefineParityTest {
    @Test
    fun refineRequest_containsPreviousTextOnce() {
        val previous = "PREVIOUS-RESULT-MARKER"
        val request = presetRefineTextRequest(previous, "shorter")
        // Every text provider sends prompt + blank line + inputText; Windows sends final_prompt only.
        val wireText = "${request.prompt}\n\n${request.inputText}"

        assertEquals(1, wireText.split(previous).size - 1)
        assertEquals("", request.inputText)
        assertEquals(
            "Content:\n$previous\n\nInstruction:\nshorter\n\nOutput ONLY the result.",
            request.prompt,
        )
    }

    @Test
    fun `unavailable original endpoint never redirects refinement to a cloud model`() {
        assertNull(initialRefineModel("unavailable-saved-model", PresetRuntimeSettings(), ApiKeys(groqKey = "test")))
    }
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
