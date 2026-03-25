package dev.screengoated.toolbox.mobile.updater

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AppUpdateSupportTest {
    @Test
    fun canonicalVersionIgnoresAndroidSuffixes() {
        assertEquals("4.8.3", canonicalAppVersion("4.8.3-full-debug"))
        assertEquals("4.8.3", canonicalAppVersion("4.8.3-play"))
    }

    @Test
    fun remoteVersionComparisonMatchesWindowsParityExpectation() {
        assertFalse(isRemoteVersionNewer("4.8.3-full-debug", "4.8.3"))
        assertTrue(isRemoteVersionNewer("4.8.2", "4.8.3"))
        assertFalse(isRemoteVersionNewer("4.8.3", "4.8.2"))
    }

    @Test
    fun androidAssetSelectionPrefersApkAndFallsBackToNull() {
        val assets = listOf(
            "ScreenGoatedToolbox_v4.8.3.exe" to "https://example.com/app.exe",
            "ScreenGoatedToolbox_v4.8.3.apk" to "https://example.com/app.apk",
        )
        assertEquals("https://example.com/app.apk", selectAndroidAssetUrl(assets))
        assertEquals(null, selectAndroidAssetUrl(listOf("a.exe" to "https://example.com/a.exe")))
    }
}
