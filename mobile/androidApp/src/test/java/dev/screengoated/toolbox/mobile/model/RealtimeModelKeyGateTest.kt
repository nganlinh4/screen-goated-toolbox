package dev.screengoated.toolbox.mobile.model

import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RealtimeModelKeyGateTest {
    @Test
    fun geminiTranscriptionAsksForAKey() {
        assertTrue(RealtimeModelIds.requiresGeminiApiKey(RealtimeModelIds.TRANSCRIPTION_GEMINI_3_1))
        assertFalse(RealtimeModelIds.isUnavailableOnAndroid(RealtimeModelIds.TRANSCRIPTION_GEMINI_3_1))
    }

    @Test
    fun everyListedSelectorEitherNeedsAKeyOrIsUnavailable() {
        GeneratedLiveModelCatalog.realtimeTranscriptionOptions.forEach { option ->
            val id = option.id
            val engine = GeneratedLiveModelCatalog.realtimeTranscriptionEngine(id)
            assertNotNull(id, engine)
            val routes = listOf(
                RealtimeModelIds.requiresGeminiApiKey(id),
                RealtimeModelIds.isUnavailableOnAndroid(id),
            )
            assertEquals(id, 1, routes.count { it })
            if (engine!!.onDeviceEngine != null || "android" !in engine.platforms) {
                assertTrue(id, RealtimeModelIds.isUnavailableOnAndroid(id))
            }
        }
    }
}
