package dev.screengoated.toolbox.mobile.service.preset

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetLoadingStatusTest {
    @Test
    fun retryStatusIsShownEscapedUnderTheAnimation() {
        val html = loadingHtml("(Retrying <Model & Co>...)")

        assertTrue(html.contains("id=\"sgt-m3e-canvas\""))
        assertTrue(html.contains("class=\"sgt-loading-status\""))
        assertTrue(html.contains("(Retrying &lt;Model &amp; Co&gt;...)"))
        assertTrue(html.indexOf("sgt-m3e-canvas") < html.indexOf("sgt-loading-status"))
    }

    @Test
    fun plainLoadShowsOnlyTheAnimation() {
        listOf(null, "", "   ").forEach { status ->
            val html = loadingHtml(status)
            assertTrue(html.contains("id=\"sgt-m3e-canvas\""))
            assertFalse(html.contains("sgt-loading-status"))
        }
    }
}
