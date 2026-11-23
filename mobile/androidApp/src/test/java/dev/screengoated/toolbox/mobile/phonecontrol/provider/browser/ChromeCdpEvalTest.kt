package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class ChromeCdpEvalTest {
    @Test
    fun largeResult_isTruncated() {
        val large = buildJsonObject { put("html", "a".repeat(MAX_EVAL_RESULT_CHARS * 2)) }

        val (bounded, truncated) = boundedEvalResult(large)

        assertTrue(truncated)
        val text = bounded.jsonPrimitive.content
        assertEquals(MAX_EVAL_RESULT_CHARS, text.length)
        assertTrue(text.startsWith("{\"html\":\"aaa"))
    }

    @Test
    fun smallResult_keepsItsJsonShape() {
        val small = buildJsonObject { put("count", 3) }

        val (bounded, truncated) = boundedEvalResult(small)

        assertFalse(truncated)
        assertSame(small, bounded)
    }

    @Test
    fun truncationDoesNotSplitSurrogatePairs() {
        val emoji = JsonPrimitive("😀".repeat(MAX_EVAL_RESULT_CHARS))

        val text = boundedEvalResult(emoji).first.jsonPrimitive.content

        assertEquals(MAX_EVAL_RESULT_CHARS, text.codePointCount(0, text.length))
        assertFalse(Character.isHighSurrogate(text.last()))
    }
}
