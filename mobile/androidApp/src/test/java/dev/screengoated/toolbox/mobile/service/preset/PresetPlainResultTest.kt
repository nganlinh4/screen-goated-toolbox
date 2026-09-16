package dev.screengoated.toolbox.mobile.service.preset

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetPlainResultTest {
    @Test fun sourceMarkupRemainsTextWithItsLineBreaks() {
        val html = plainPresetResult("<script>alert('x')</script>\n**bold** & tail")
        assertFalse(html.contains("<script>"))
        assertTrue(html.contains("&lt;script&gt;"))
        assertTrue(html.contains("\n**bold** &amp; tail"))
    }
}
