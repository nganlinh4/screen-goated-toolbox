package dev.screengoated.toolbox.mobile.phonecontrol.provider.visual

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class VisualProjectionFallbackCacheTest {
    @Test
    fun projectionFallbackIsOwnedByTheScreenshotCache() {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        val provider = File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/" +
                "provider/visual/PhoneControlVisualProvider.kt",
        ).readText()
        val screenshotFor = provider.substringAfter("private suspend fun screenshotFor(")
            .substringBefore("private const val MAX_STALE_CAPTURE_RETRIES")
        assertTrue(!screenshotFor.contains("return capturePhoneControlProjectionScreenshot("))
        val routed = screenshotFor.indexOf("val routed =")
        val fallback = screenshotFor.indexOf("capturePhoneControlProjectionScreenshot(", routed)
        val cached = screenshotFor.indexOf("cachedScreenshot = captured.value")
        assertTrue(routed in 0 until fallback)
        assertTrue(fallback < cached)
        assertEquals(1, Regex("""cachedScreenshot = captured\.value""").findAll(screenshotFor).count())
    }
}
