package dev.screengoated.toolbox.mobile.preset

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.File

class PreviousVisionMatchesTest {
    @Test
    fun cachedMatchesEqualExhaustiveNonoverlappingSearch() {
        for (length in 0..7) {
            var count = 1
            repeat(length) { count *= 3 }
            for (code in 0 until count) {
                var remaining = code
                val points = IntArray(length) { intArrayOf(97, 98, 0x1F331)[remaining % 3].also { remaining /= 3 } }
                val matches = PreviousVisionMatches(points)
                val expected = IntArray(length) { start ->
                    (1..minOf(start, length - start)).filter { span ->
                        (0..start - span).any { candidate -> (0 until span).all { points[candidate + it] == points[start + it] } }
                    }.maxOrNull() ?: 0
                }
                for (start in points.indices) {
                    assertEquals(expected[start], matches.span(start))
                    for (minimum in 1..3) {
                        assertEquals(expected.drop(start).any { it >= minimum }, matches.hasAnchor(start, minimum))
                        for (end in start..length) {
                            var covered = 0
                            var index = start
                            while (index < end) {
                                val span = expected[index]
                                if (span >= minimum) { covered += minOf(span, end - index); index += span }
                                else index += 1
                            }
                            val coverage = if (end == start) 0f else covered.toFloat() / (end - start)
                            assertEquals(coverage, matches.coverage(start, end, minimum), 0f)
                        }
                    }
                }
            }
        }
    }

    @Test
    fun sharedLongRepliesKeepAllLegitimateText() {
        val cases = JSONObject(File("../../parity-fixtures/preset-system/vision-repetition-work.json").readText()).getJSONArray("cases")
        for (index in 0 until cases.length()) {
            val case = cases.getJSONObject(index)
            val text = case.getString("text").repeat(case.getInt("repeat"))
            val guard = VisionRepetitionGuard()
            text.codePoints().forEach { guard.observe(String(Character.toChars(it))) }
            assertEquals(text, guard.finish(text))
        }
    }
}
