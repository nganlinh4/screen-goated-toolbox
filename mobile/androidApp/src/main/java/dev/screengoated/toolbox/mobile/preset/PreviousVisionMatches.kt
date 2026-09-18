package dev.screengoated.toolbox.mobile.preset

/** Exact non-overlapping previous matches, shared by anchor and coverage queries. */
internal class PreviousVisionMatches(points: IntArray) {
    private val spans = IntArray(points.size)
    private val suffixMax = IntArray(points.size + 1)

    init {
        val common = IntArray(points.size + 1)
        for (start in points.indices.reversed()) {
            // Ascending candidates retain the next row's common[candidate + 1].
            for (candidate in 0 until start) {
                common[candidate] = if (points[start] == points[candidate]) common[candidate + 1] + 1 else 0
                spans[start] = maxOf(spans[start], minOf(common[candidate], start - candidate))
            }
            suffixMax[start] = maxOf(suffixMax[start + 1], spans[start])
        }
    }

    fun span(start: Int): Int = spans[start]
    fun hasAnchor(start: Int, minimum: Int): Boolean = suffixMax[start] >= minimum

    fun coverage(start: Int, end: Int, minimum: Int): Float {
        if (end <= start) return 0f
        var covered = 0
        var index = start
        while (index < end) {
            val span = spans[index]
            if (span >= minimum) {
                covered += minOf(span, end - index)
                index += span
            } else index += 1
        }
        return covered.toFloat() / (end - start)
    }
}
