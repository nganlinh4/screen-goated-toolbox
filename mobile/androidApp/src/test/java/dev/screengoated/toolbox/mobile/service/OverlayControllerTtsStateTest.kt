package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.service.tts.TtsConsumer
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeState
import org.junit.Assert.assertEquals
import org.junit.Test

class OverlayControllerTtsStateTest {
    @Test
    fun `overlay uses runtime effective speed while realtime tts is actively speaking`() {
        val state = overlayTtsState(
            settings = RealtimeTtsSettings(
                enabled = true,
                speedPercent = 100,
                autoSpeed = true,
                volumePercent = 90,
            ),
            runtimeState = TtsRuntimeState(
                isPlaying = true,
                activeConsumer = TtsConsumer.REALTIME,
                currentRealtimeEffectiveSpeed = 145,
            ),
        )

        assertEquals(145, state.speedPercent)
        assertEquals(true, state.enabled)
        assertEquals(true, state.autoSpeed)
        assertEquals(90, state.volumePercent)
    }

    @Test
    fun `overlay falls back to saved speed when realtime playback is inactive`() {
        val state = overlayTtsState(
            settings = RealtimeTtsSettings(
                enabled = true,
                speedPercent = 100,
                autoSpeed = true,
                volumePercent = 90,
            ),
            runtimeState = TtsRuntimeState(
                isPlaying = false,
                activeConsumer = TtsConsumer.REALTIME,
                currentRealtimeEffectiveSpeed = 145,
            ),
        )

        assertEquals(100, state.speedPercent)
    }

    @Test
    fun `overlay forces read visual on for direct s2s mode`() {
        val state = overlayTtsState(
            settings = RealtimeTtsSettings(
                enabled = false,
                speedPercent = 125,
                autoSpeed = true,
                volumePercent = 80,
            ),
            runtimeState = TtsRuntimeState(),
            forceEnabled = true,
        )

        assertEquals(true, state.enabled)
        assertEquals(125, state.speedPercent)
        assertEquals(80, state.volumePercent)
    }
}
