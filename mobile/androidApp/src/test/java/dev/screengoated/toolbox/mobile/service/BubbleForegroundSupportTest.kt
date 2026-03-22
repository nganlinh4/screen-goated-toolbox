package dev.screengoated.toolbox.mobile.service

import android.content.pm.ServiceInfo
import dev.screengoated.toolbox.mobile.service.preset.PresetAudioForegroundMode
import org.junit.Assert.assertEquals
import org.junit.Test

class BubbleForegroundSupportTest {
    @Test
    fun `none mode maps to special use foreground type`() {
        assertEquals(
            ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE,
            resolveBubbleForegroundServiceType(PresetAudioForegroundMode.NONE),
        )
    }

    @Test
    fun `microphone mode maps to microphone foreground type`() {
        assertEquals(
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
            resolveBubbleForegroundServiceType(PresetAudioForegroundMode.MICROPHONE),
        )
    }

    @Test
    fun `media projection mode maps to media projection foreground type`() {
        assertEquals(
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION,
            resolveBubbleForegroundServiceType(PresetAudioForegroundMode.MEDIA_PROJECTION),
        )
    }
}
