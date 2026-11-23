package dev.screengoated.toolbox.mobile.service.preset

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetOverlayWindowSupportTest {
    @Test
    fun fileUrlWithGesture_isBlocked() {
        assertEquals(OverlayNavigationPolicy.BLOCK, overlayNavigationPolicy("file:///sdcard/Download/a.pdf", true))
        assertEquals(OverlayNavigationPolicy.BLOCK, overlayNavigationPolicy("FILE:///data/x", true))
    }

    @Test
    fun contentUrlWithGesture_isBlocked() {
        assertEquals(
            OverlayNavigationPolicy.BLOCK,
            overlayNavigationPolicy("content://dev.screengoated.toolbox.fileprovider/x", true),
        )
    }

    @Test
    fun bundledAssetAndExternalSchemesKeepTheirPolicy() {
        assertEquals(
            OverlayNavigationPolicy.MANAGED,
            overlayNavigationPolicy("file:///android_asset/preset_overlay/result.html", false),
        )
        assertEquals(OverlayNavigationPolicy.EXTERNAL_INTENT, overlayNavigationPolicy("mailto:a@b.c", true))
        assertEquals(OverlayNavigationPolicy.BLOCK, overlayNavigationPolicy("mailto:a@b.c", false))
    }

    @Test
    fun bridgedLegacyWindowNeverLoadsAWebPageWithTheBridge() {
        // Without the flag (result windows) web pages stay managed by the isolated result views.
        assertEquals(OverlayNavigationPolicy.MANAGED, overlayNavigationPolicy("https://evil.example", false))
        assertEquals(OverlayNavigationPolicy.BLOCK, overlayNavigationPolicy("https://evil.example", false, true))
        assertEquals(OverlayNavigationPolicy.BLOCK, overlayNavigationPolicy("HTTP://evil.example", false, true))
        assertEquals(OverlayNavigationPolicy.EXTERNAL_INTENT, overlayNavigationPolicy("https://example.com", true, true))
        assertEquals(
            OverlayNavigationPolicy.MANAGED,
            overlayNavigationPolicy("file:///android_asset/preset_overlay/text_input.html", false, true),
        )
        assertEquals(OverlayNavigationPolicy.MANAGED, overlayNavigationPolicy("about:blank", false, true))
    }

    @Test
    fun overlayWindowPassesTheBridgeFlagForNonResultWindows() {
        var directory = java.io.File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!java.io.File(directory, ".claude/parity").exists()) directory = requireNotNull(directory.parentFile)
        val window = java.io.File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/PresetOverlayWindow.kt",
        ).readText()
        assertTrue(window.contains("pageWorldBridge = resultWindowId == null,"))
        // The page-world bridge is installed exactly for those windows.
        assertTrue(window.contains("} else installPageWorldBridge(this, ::dispatchBridgeMessage)"))
    }
}
