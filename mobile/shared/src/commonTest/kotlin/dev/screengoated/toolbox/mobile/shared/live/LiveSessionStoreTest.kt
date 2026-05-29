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
    fun target_language_change_only_restarts_gemini_s2s() {
        assertTrue(
            LiveTranslateParity.targetLanguageChangeRequiresRestart(
                previousLanguage = "Vietnamese",
                nextLanguage = "Korean",
                transcriptionProviderId = "gemini-live-s2s",
            ),
        )
        assertFalse(
            LiveTranslateParity.targetLanguageChangeRequiresRestart(
                previousLanguage = "Vietnamese",
                nextLanguage = "Korean",
                transcriptionProviderId = "gemini-live-audio",
            ),
        )
        assertFalse(
            LiveTranslateParity.targetLanguageChangeRequiresRestart(
                previousLanguage = "Vietnamese",
                nextLanguage = "Vietnamese",
                transcriptionProviderId = "gemini-live-s2s",
            ),
        )
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

    @Test
    fun restart_freezes_visible_prefix_without_reprocessing_translation() {
        val store = LiveSessionStore()
        store.appendTranscript("old session.", nowMs = 100)

        store.freezeCurrentTranscript()
        store.markStarting(preserveFrozenPrefix = true)
        store.appendTranscript("new tail", nowMs = 200)

        val liveText = store.state.value.liveText
        assertEquals("old session.", liveText.frozenPrefix)
        assertEquals("new tail", liveText.fullTranscript)
        assertEquals("old session. new tail", liveText.displayTranscript)

        val request = store.claimTranslationRequest()
            ?: error("expected request for new session text")
        assertEquals(0, request.sourceStart)
        assertEquals("new tail", request.pendingSource)
    }

    @Test
    fun rejected_translation_response_reports_not_applied() {
        val store = LiveSessionStore()
        store.appendTranscript("hello. tail", nowMs = 100)
        val request = store.claimTranslationRequest()
            ?: error("expected translation request")

        val response = TranslationResponse(
            patches = listOf(
                TranslationPatch(
                    sourceStart = request.sourceStart,
                    sourceEnd = request.finalizedSourceEnd,
                    state = "final",
                    translation = "xin chao.",
                ),
                TranslationPatch(
                    sourceStart = request.draftSourceStart,
                    sourceEnd = request.sourceEnd,
                    state = "draft",
                    translation = "duoi",
                ),
            ),
        )
        assertTrue(
            store.applyTranslationResponse(
                request = request,
                response = response,
                nowMs = 130,
            ),
        )

        val applied = store.applyTranslationResponse(
            request = request,
            response = response,
            nowMs = 150,
        )

        assertFalse(applied)
        assertEquals(11, store.state.value.liveText.lastProcessedLen)
        assertEquals("xin chao. duoi", store.state.value.liveText.displayTranslation)
    }
}
