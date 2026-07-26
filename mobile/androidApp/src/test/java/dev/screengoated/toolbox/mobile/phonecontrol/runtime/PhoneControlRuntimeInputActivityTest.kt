package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlRuntimeInputActivityTest {
    @Test
    fun `orb signal matches Windows PCM16 voice floor and gain`() {
        assertEquals(0f, phoneControlOrbAudioLevel(SPEECH_RMS_THRESHOLD - 0.00001f))
        assertEquals(0.03f, phoneControlOrbAudioLevel(SPEECH_RMS_THRESHOLD), 0.00001f)
        assertEquals(1f, phoneControlOrbAudioLevel(4000f / 32768f), 0.00001f)
        assertEquals(1f, phoneControlOrbAudioLevel(1f))
        assertEquals(0f, phoneControlOrbAudioLevel(Float.NaN))
    }

    @Test
    fun `each voiced burst has one structural start and end`() {
        val starts = mutableListOf<Long>()
        val ends = mutableListOf<Triple<Long, Long, Long>>()
        val levels = mutableListOf<Float>()
        val activity = PhoneControlRuntimeInputActivity(
            onSpeechStarted = starts::add,
            onSpeechEnded = { epoch, elapsedMs, frames ->
                ends += Triple(epoch, elapsedMs, frames)
            },
            onLevel = levels::add,
        )

        activity.observe(0f, 100L)
        activity.observe(SPEECH_RMS_THRESHOLD, 200L)
        activity.observe(SPEECH_RMS_THRESHOLD * 2f, 300L)
        activity.observe(0f, 801L)

        assertEquals(listOf(1L), starts)
        assertEquals(listOf(Triple(1L, 601L, 3L)), ends)
        assertTrue(activity.speechObserved)
        assertFalse(activity.isActive(801L))
        assertTrue(levels.any { it >= 0.03f })

        activity.observe(SPEECH_RMS_THRESHOLD, 900L)
        assertEquals(listOf(1L, 2L), starts)
    }

    @Test
    fun `new burst after an unobserved gap closes the prior epoch first`() {
        val events = mutableListOf<String>()
        val activity = PhoneControlRuntimeInputActivity(
            onSpeechStarted = { epoch -> events += "start:$epoch" },
            onSpeechEnded = { epoch, elapsedMs, frames ->
                events += "end:$epoch:$elapsedMs:$frames"
            },
            onLevel = {},
        )

        activity.observe(SPEECH_RMS_THRESHOLD, 100L)
        activity.observe(SPEECH_RMS_THRESHOLD, 700L)
        activity.observe(0f, 1_201L)

        assertEquals(
            listOf(
                "start:1",
                "end:1:600:1",
                "start:2",
                "end:2:501:2",
            ),
            events,
        )
    }
}
