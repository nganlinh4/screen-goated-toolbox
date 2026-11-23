package dev.screengoated.toolbox.mobile.service

import android.content.pm.ServiceInfo
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LiveTranslateServiceForegroundTest {
    @Test
    fun settleTypeNeedsNoRuntimePermission() {
        assertEquals(ServiceInfo.FOREGROUND_SERVICE_TYPE_SHORT_SERVICE, permissionFreeForegroundType(34))
        assertEquals(0, permissionFreeForegroundType(33))
        assertEquals(0, permissionFreeForegroundType(29))
        assertNull(permissionFreeForegroundType(28))
    }

    @Test
    fun missingRecordAudio_doesNotRequestMicrophoneFgsType() {
        val source = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/LiveTranslateService.kt",
        ).readText()
        val cannotStart = source
            .substringAfter("if (!repository.canStartSession()) {")
            .substringBefore("return\n")
        assertTrue(cannotStart.contains("settleWithoutSession(notification)"))
        assertFalse(cannotStart.contains("startMicrophoneForeground"))
        val settle = source.substringAfter("private fun settleWithoutSession(").substringBefore("\n    }\n")
        assertTrue(settle.contains("startPermissionFreeForeground("))
        assertTrue(
            "the service manifest must keep shortService for the settle path",
            repoFile("mobile/androidApp/src/main/AndroidManifest.xml").readText()
                .substringAfter(".service.LiveTranslateService\"")
                .substringBefore("/>")
                .contains("shortService"),
        )
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
