package dev.screengoated.toolbox.mobile.service.preset

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetResultWebViewSecurityTest {
    @Test
    fun queuedScriptIsBoundToTheDocumentThatCreatedIt() {
        val scripts = TrustedResultScripts()
        val first = scripts.prepare("<html><head></head></html>")
        val oldScript = scripts.wrap("window.receive('payload');")
        val second = scripts.prepare("<html><head></head></html>")
        val newScript = scripts.wrap("window.receive('payload');")
        val tokenPattern = Regex("value:'([^']+)'")
        val firstToken = requireNotNull(tokenPattern.find(first)).groupValues[1]
        val secondToken = requireNotNull(tokenPattern.find(second)).groupValues[1]
        assertTrue(oldScript.contains(firstToken))
        assertTrue(newScript.contains(secondToken))
        assertFalse(oldScript.contains(secondToken))
        assertFalse(newScript.contains(firstToken))
    }

    @Test
    fun isolatedGesturesFailClosedAfterNavigationOrViewReplacement() {
        val gate = ResultDocumentGestureGate()
        assertFalse(gate.allows(activeView = true, pageReady = true))
        gate.onDocumentStarted()
        assertFalse(gate.allows(activeView = true, pageReady = false))
        assertTrue(gate.allows(activeView = true, pageReady = true))
        assertFalse(gate.allows(activeView = false, pageReady = true))
        gate.onDocumentStarted()
        assertFalse(gate.allows(activeView = true, pageReady = true))
    }

    @Test
    fun repeatedManagedAudioLoadsGetFreshProvenViewsAndIgnoreRetiredNavigation() {
        val preview = ResultAudioPreview()
        var previousUrl: String? = null
        repeat(3) { index ->
            assertTrue(needsFreshResultView(isResult = true, wasUntrusted = index > 0, isUntrusted = true))
            val gate = ResultDocumentGestureGate()
            val url = requireNotNull(preview.replace(byteArrayOf(index.toByte())))
            assertTrue(gate.onDocumentStarted())
            assertTrue(gate.allows(activeView = true, pageReady = true))
            assertTrue(preview.accepts(url, "GET", false))
            previousUrl?.let { assertFalse(preview.accepts(it, "GET", false)) }
            revokePreviewOnNavigation(activeView = false, preview = preview)
            assertTrue(preview.accepts(url, "GET", false))
            previousUrl = url
        }
        revokePreviewOnNavigation(activeView = true, preview = preview)
        assertFalse(preview.accepts(requireNotNull(previousUrl), "GET", false))
        assertFalse(needsFreshResultView(isResult = false, wasUntrusted = false, isUntrusted = true))
        assertFalse(needsFreshResultView(isResult = true, wasUntrusted = false, isUntrusted = false))
        assertTrue(needsFreshResultView(isResult = true, wasUntrusted = true, isUntrusted = false))
    }

    @Test
    fun onlyTheExpectedAssetDocumentIsTrusted() {
        assertTrue(isTrustedResultOrigin(RESULT_TRUSTED_ORIGIN))
        assertTrue(isTrustedResultOrigin("$RESULT_TRUSTED_ORIGIN/"))
        assertFalse(isTrustedResultOrigin("https://sgt-result.invalid.evil.example"))
        assertFalse(isTrustedResultOrigin("null"))
        assertTrue(isTrustedResultUrl(RESULT_TRUSTED_URL))
        assertTrue(isTrustedResultUrl("$RESULT_TRUSTED_URL#section"))
        listOf(
            null,
            "file:///android_asset/preset_overlay/",
            "https://sgt-result.invalid.evil.example/assets/preset_overlay/",
            "https://sgt-result.invalid/assets/other/",
            "http://sgt-result.invalid/assets/preset_overlay/",
            "https://evil@sgt-result.invalid/assets/preset_overlay/",
            "https://sgt-result.invalid:444/assets/preset_overlay/",
        ).forEach { assertFalse(isTrustedResultUrl(it)) }
    }

    @Test
    fun trustedResultUrlIsNotTreatedAsExternalNavigation() {
        assertTrue(isInternalResultUrl(RESULT_TRUSTED_URL))
        assertFalse(isExternalNavigationUrl(RESULT_TRUSTED_URL))
        assertTrue(isExternalNavigationUrl("https://example.org/"))
    }
}
