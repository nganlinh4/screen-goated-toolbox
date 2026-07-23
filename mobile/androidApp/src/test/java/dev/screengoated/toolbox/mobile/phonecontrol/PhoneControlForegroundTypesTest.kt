package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.pm.ServiceInfo
import android.os.Build
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlForegroundTypesTest {
    @Test
    fun `every supported API includes media projection authority`() {
        (Build.VERSION_CODES.Q..Build.VERSION_CODES.BAKLAVA).forEach { apiLevel ->
            assertTrue(
                phoneControlForegroundServiceTypes(apiLevel) and
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION != 0,
            )
        }
    }

    @Test
    fun `foreground types expand only when the platform supports them`() {
        assertEquals(
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK or
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION,
            phoneControlForegroundServiceTypes(Build.VERSION_CODES.Q),
        )
        assertEquals(
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK or
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION or
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
            phoneControlForegroundServiceTypes(Build.VERSION_CODES.R),
        )
        assertEquals(
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK or
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION or
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE or
                ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE,
            phoneControlForegroundServiceTypes(Build.VERSION_CODES.UPSIDE_DOWN_CAKE),
        )
    }
}
