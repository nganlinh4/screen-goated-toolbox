package dev.screengoated.toolbox.mobile.shared.live

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class LiveSessionStoreTest {
    @Test
    fun patch_updates_target_language_without_losing_defaults() {
        val store = LiveSessionStore()

        store.updateConfig(LiveSessionPatch(targetLanguage = "Korean"))

        assertEquals("Korean", store.state.value.config.targetLanguage)
        assertEquals(SourceMode.DEVICE, store.state.value.config.sourceMode)
    }

    @Test
    fun permission_snapshot_respects_overlay_and_playback_requirements() {
        val config = LiveSessionConfig(
            sourceMode = SourceMode.DEVICE,
            displayMode = DisplayMode.OVERLAY,
        )

        val incomplete = PermissionSnapshot(
            recordAudioGranted = true,
            notificationsGranted = true,
            overlayGranted = false,
            mediaProjectionGranted = true,
        )
        val complete = incomplete.copy(overlayGranted = true)

        assertFalse(incomplete.readyFor(config, overlaySupported = true))
        assertTrue(complete.readyFor(config, overlaySupported = true))
    }

    @Test
    fun gemini_s2s_display_simulates_committed_and_draft_windows() {
        val store = LiveSessionStore()

        store.setGeminiS2sDisplay(
            sourceCommitted = "Hello world.",
            sourceDraft = "Next phrase",
            targetCommitted = "Bonjour monde.",
            targetDraft = "Next translated phrase",
            nowMs = 1_000,
        )

        val liveText = store.state.value.liveText
        assertEquals("Hello world. Next phrase", liveText.fullTranscript)
        assertEquals("Hello world. Next phrase", liveText.displayTranscript)
        assertEquals(
            "Next phrase",
            liveText.fullTranscript.substring(liveText.uncommittedSourceStart, liveText.uncommittedSourceEnd),
        )
        assertEquals("Bonjour monde.", liveText.committedTranslation)
        assertEquals("Next translated phrase", liveText.uncommittedTranslation)
        assertEquals("Bonjour monde. Next translated phrase", liveText.displayTranslation)
        assertEquals(TranscriptionMethod.GEMINI_LIVE_S2S, liveText.transcriptionMethod)
    }
}
