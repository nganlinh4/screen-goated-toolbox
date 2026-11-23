package dev.screengoated.toolbox.mobile.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DjWebViewPolicyTest {
    @Test
    fun externalNavigation_isNotLoadedInWebView() {
        assertTrue(isDjAssetUrl("https://promptdj.screengoated.local/promptdj/index.html"))
        assertFalse(isDjAssetUrl("https://example.com/promptdj/index.html"))
        assertFalse(isDjAssetUrl("http://promptdj.screengoated.local/promptdj/index.html"))
        assertFalse(isDjAssetUrl("https://promptdj.screengoated.local.evil.test/"))
        assertFalse(isDjAssetUrl("https://promptdj.screengoated.local:8443/"))
        assertTrue(isExternalWebUrl("https://example.com/help"))
        assertFalse(isExternalWebUrl("intent://x#Intent;end"))
    }

    @Test
    fun keyInjection_requiresAssetOrigin() {
        assertEquals("https://promptdj.screengoated.local", DJ_ORIGIN)
        assertFalse(isDjAssetUrl(null))
        assertFalse(isDjAssetUrl("about:blank"))
        assertFalse(isDjAssetUrl("https://user@promptdj.screengoated.local/"))
    }
}
