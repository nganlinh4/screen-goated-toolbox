package dev.screengoated.toolbox.mobile.preset

/** Cache exactly which token boundaries have broken-word evidence after them. */
internal class VisionFragmentEvidence(text: String) {
    private val tokens = Regex("\\S+").findAll(text).toList()
    private val boundaries: BooleanArray

    init {
        val words = tokens.map { it.value.lowercase() }
        val ends = IntArray(words.size + 1)
        val haystack = StringBuilder()
        words.forEachIndexed { index, word ->
            word.codePoints().forEach { point ->
                if (haystack.isEmpty() || haystack.codePointBefore(haystack.length) != point) haystack.appendCodePoint(point)
            }
            ends[index + 1] = haystack.length
        }
        val first = HashMap<String, Int>()
        val changes = IntArray(words.size + 2)
        words.zipWithNext().forEachIndexed { index, (left, right) ->
            val firstIndex = first.getOrPut(left) { index }
            val joined = squeezeVisionText(left + right)
            if (joined.codePointCount(0, joined.length) <= left.codePointCount(0, left.length)) return@forEachIndexed
            val at = haystack.indexOf(joined)
            if (at < 0) return@forEachIndexed
            val lower = ends.indexOfFirst { it >= at + joined.length }
            val upper = minOf(index, firstIndex)
            if (lower in 0..upper) { changes[lower] += 1; changes[upper + 1] -= 1 }
        }
        var active = 0
        boundaries = BooleanArray(changes.size) { active += changes[it]; active > 0 }
    }

    fun at(onset: Int): Boolean = boundaries[tokens.count { it.range.first < onset }]
}
