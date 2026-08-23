package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.imageBlock
import dev.screengoated.toolbox.mobile.shared.preset.textBlock
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetRuntimeCapabilitiesTest {
    @Test
    fun `NVIDIA and Gemini Live use the runtime dispatch capability table`() {
        assertTrue(PresetModelProvider.NVIDIA.hasTextPresetRuntime())
        assertTrue(PresetModelProvider.NVIDIA.hasVisionPresetRuntime())
        assertTrue(PresetModelProvider.GEMINI_LIVE.hasTextPresetRuntime())
        assertTrue(PresetModelProvider.GEMINI_LIVE.hasVisionPresetRuntime())
        assertTrue(PresetModelProvider.GEMINI_LIVE.hasAudioPresetRuntime())
        assertTrue(PresetModelProvider.TAALAS.hasTextPresetRuntime())
    }

    @Test
    fun `repository accepts catalog models implemented by Android clients`() {
        val resolver = PresetExecutionCapabilityResolver()
        val textPreset = Preset(
            id = "runtime-text",
            nameEn = "Runtime text",
            nameVi = "Runtime text",
            nameKo = "Runtime text",
            presetType = PresetType.TEXT_INPUT,
            blocks = listOf(textBlock("nvidia-nemotron-3-5-lightning-text", "")),
        )
        val visionPreset = Preset(
            id = "runtime-vision",
            nameEn = "Runtime vision",
            nameVi = "Runtime vision",
            nameKo = "Runtime vision",
            presetType = PresetType.IMAGE,
            blocks = listOf(imageBlock("google-gemini-3-1-live-vision", "")),
        )

        assertTrue(resolver.resolveExecutionCapability(textPreset).supported)
        assertTrue(resolver.resolveExecutionCapability(visionPreset).supported)
    }
}
