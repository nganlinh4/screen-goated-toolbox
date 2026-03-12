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
}
