package dev.screengoated.toolbox.mobile.preset

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetProviderSettingsTest {
    @Test
    fun `shared selector predicate includes NVIDIA and preserves local providers`() {
        val disabled = PresetProviderSettings(
            useGroq = false,
            useGemini = false,
            useOpenRouter = false,
            useNvidia = false,
            useOllama = false,
        )

        assertFalse(presetProviderEnabled(PresetModelProvider.NVIDIA, disabled))
        assertFalse(presetProviderEnabled(PresetModelProvider.GROQ, disabled))
        assertFalse(presetProviderEnabled(PresetModelProvider.GEMINI_LIVE, disabled))
        assertTrue(presetProviderEnabled(PresetModelProvider.TAALAS, disabled))
        assertTrue(
            presetProviderEnabled(
                PresetModelProvider.NVIDIA,
                disabled.copy(useNvidia = true),
            ),
        )
    }
}
