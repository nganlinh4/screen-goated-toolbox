package dev.screengoated.toolbox.mobile.service.preset

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetHtmlTokenSearchTest {
    private class CountingSequence(private val value: String) : CharSequence {
        var reads = 0
        override val length: Int get() = value.length
        override fun get(index: Int): Char {
            reads++
            return value[index]
        }
        override fun subSequence(startIndex: Int, endIndex: Int): CharSequence =
            value.subSequence(startIndex, endIndex)
    }

    @Test
    fun searchesDoNotReadBeyondEachScriptBoundary() {
        val script = "<script>const x = 1;</script>"
        val source = CountingSequence("<html>" + script.repeat(4_000) + "</html><!--")
        val bodyStart = "<html>".length + "<script>".length
        repeat(4_000) { index ->
            val start = bodyStart + index * script.length
            val end = start + "const x = 1;".length
            assertEquals(-1, indexOfHtmlTokenWithin(source, "<!--", start, end))
        }
        assertTrue("search work must scale with script bytes", source.reads <= 4_000 * script.length)
        assertNotNull(normalizePresetHtmlContent("<html>" + script.repeat(4_000) + "</html>"))
    }

    @Test
    fun searchDoesNotMatchATokenCrossingTheRangeEnd() {
        assertEquals(-1, indexOfHtmlTokenWithin("xx<!--yy", "<!--", 0, 5))
        assertEquals(2, indexOfHtmlTokenWithin("xx<!--yy", "<!--", 0, 6))
    }
}
