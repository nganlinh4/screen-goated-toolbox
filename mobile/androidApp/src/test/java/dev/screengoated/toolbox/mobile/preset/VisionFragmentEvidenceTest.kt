package dev.screengoated.toolbox.mobile.preset

import org.junit.Assert.assertEquals
import org.junit.Test

class VisionFragmentEvidenceTest {
    @Test
    fun cachedEvidenceEqualsOriginalTokenPredicate() {
        for (code in 0 until 7776) {
            var remaining = code
            val text = buildString {
                repeat(5) {
                    append(arrayOf("ab", "a", "b", "ba", "🌱a", "🌱")[remaining % 6])
                    append(' ')
                    remaining /= 6
                }
            }
            val evidence = VisionFragmentEvidence(text)
            val tokens = Regex("\\S+").findAll(text).toList()
            for (onset in 0..text.length) {
                val before = tokens.filter { it.range.first < onset }.map { it.value.lowercase() }
                val after = tokens.filter { it.range.first >= onset }.map { it.value.lowercase() }
                val haystack = squeezeVisionText(before.joinToString(""))
                val expected = after.zipWithNext().any { (first, second) ->
                    val joined = squeezeVisionText(first + second)
                    joined.codePointCount(0, joined.length) > first.codePointCount(0, first.length) && first !in before && joined in haystack
                }
                assertEquals("$text at $onset", expected, evidence.at(onset))
            }
        }
    }
}
