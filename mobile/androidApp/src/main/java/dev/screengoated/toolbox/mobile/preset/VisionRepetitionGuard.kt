package dev.screengoated.toolbox.mobile.preset

private const val MIN_REPEAT_SPAN = 8
private const val MIN_FRAGMENT_SPAN = 3
private const val ONSET_WINDOW = 32
private const val LOCAL_COVERAGE = 0.92f
private const val TAIL_COVERAGE = 0.80f
private const val MIN_JUDGED_CODE_POINTS = MIN_REPEAT_SPAN * 2
private const val MAX_SCANNED_CODE_POINTS = 2_048
private const val CHECK_INTERVAL = 48
private const val STREAMING_MIN_EVIDENCE = 64

internal sealed interface RepetitionAction {
    data object Paint : RepetitionAction
    data class Replace(val text: String) : RepetitionAction
    data object Suppress : RepetitionAction
}

/** Endpoint-scoped salvage for replies that start restating themselves at broken token seams. */
internal class VisionRepetitionGuard {
    private val seen = StringBuilder()
    private var salvaged: String? = null
    private var checkedLength = 0

    fun observe(chunk: String): RepetitionAction {
        if (salvaged != null) return RepetitionAction.Suppress
        seen.append(chunk)
        if (seen.length < checkedLength + CHECK_INTERVAL) return RepetitionAction.Paint
        checkedLength = seen.length
        val complete = seen.toString().substringBeforeLastWhitespace()
        val onset = repetitionOnset(complete, STREAMING_MIN_EVIDENCE)
            ?: return RepetitionAction.Paint
        return RepetitionAction.Replace(complete.substring(0, onset).trimEnd()).also {
            salvaged = it.text
        }
    }

    fun restart(text: String) {
        seen.clear()
        seen.append(text)
        salvaged = null
        checkedLength = 0
    }

    fun finish(streamed: String): String = salvaged
        ?: repetitionOnset(streamed)?.let { streamed.substring(0, it).trimEnd() }
        ?: streamed
}

internal fun salvageVisionRestatement(text: String): String =
    repetitionOnset(text)?.let { text.substring(0, it).trimEnd() } ?: text

private data class ScanText(
    val points: IntArray,
    val offsets: IntArray,
    val written: IntArray,
    val totalWritten: Int,
)

private fun repetitionOnset(
    text: String,
    minimumEvidence: Int = MIN_REPEAT_SPAN,
): Int? {
    val scan = scanText(text)
    if (scan.points.size < MIN_JUDGED_CODE_POINTS) return null
    val last = scan.points.size - MIN_REPEAT_SPAN
    for (start in MIN_REPEAT_SPAN..last) {
        if (!occursEarlier(scan.points, start, MIN_FRAGMENT_SPAN)) continue
        if (scan.totalWritten - scan.written[start] < minimumEvidence) continue
        if (!hasAnchor(scan.points, start)) continue
        val windowEnd = minOf(start + ONSET_WINDOW, scan.points.size)
        if (coverage(scan.points, start, windowEnd) < LOCAL_COVERAGE) continue
        if (coverage(scan.points, start, scan.points.size) < TAIL_COVERAGE) continue
        val offset = scan.offsets[start]
        if (!isFragmented(text, offset)) continue
        return snapToTokenEnd(text, offset)
    }
    return null
}

private fun scanText(text: String): ScanText {
    val points = ArrayList<Int>(MAX_SCANNED_CODE_POINTS)
    val offsets = ArrayList<Int>(MAX_SCANNED_CODE_POINTS)
    val written = ArrayList<Int>(MAX_SCANNED_CODE_POINTS)
    var totalWritten = 0
    var offset = 0
    while (offset < text.length && points.size < MAX_SCANNED_CODE_POINTS) {
        val point = text.codePointAt(offset)
        val nextOffset = offset + Character.charCount(point)
        if (!Character.isWhitespace(point)) {
            val folded = Character.toLowerCase(point)
            totalWritten += 1
            if (points.lastOrNull() == folded) {
                offsets[offsets.lastIndex] = offset
            } else {
                points.add(folded)
                offsets.add(offset)
                written.add(totalWritten - 1)
            }
        }
        offset = nextOffset
    }
    return ScanText(
        points = points.toIntArray(),
        offsets = offsets.toIntArray(),
        written = written.toIntArray(),
        totalWritten = totalWritten,
    )
}

private fun occursEarlier(points: IntArray, start: Int, length: Int): Boolean {
    if (start + length > points.size || length > start) return false
    for (candidate in 0..start - length) {
        var matches = true
        for (index in 0 until length) {
            if (points[candidate + index] != points[start + index]) {
                matches = false
                break
            }
        }
        if (matches) return true
    }
    return false
}

private fun hasAnchor(points: IntArray, start: Int): Boolean =
    (start..points.size - MIN_REPEAT_SPAN).any { occursEarlier(points, it, MIN_REPEAT_SPAN) }

private fun coverage(points: IntArray, start: Int, end: Int): Float {
    if (end <= start) return 0f
    var covered = 0
    var index = start
    while (index < end) {
        var span = 0
        var length = MIN_FRAGMENT_SPAN
        while (index + length <= points.size && occursEarlier(points, index, length)) {
            span = length
            length += 1
        }
        if (span >= MIN_FRAGMENT_SPAN) {
            covered += minOf(span, end - index)
            index += span
        } else {
            index += 1
        }
    }
    return covered.toFloat() / (end - start)
}

private fun isFragmented(text: String, onset: Int): Boolean {
    val matches = Regex("\\S+").findAll(text).toList()
    val before = matches.filter { it.range.first < onset }.map { it.value.lowercase() }
    val after = matches.filter { it.range.first >= onset }.map { it.value.lowercase() }
    val haystack = squeeze(before.joinToString(separator = ""))
    return after.zipWithNext().any { (first, second) ->
        val joined = squeeze(first + second)
        joined.codePointCount(0, joined.length) > first.codePointCount(0, first.length) &&
            first !in before &&
            joined in haystack
    }
}

private fun squeeze(text: String): String = buildString(text.length) {
    var previous = -1
    var offset = 0
    while (offset < text.length) {
        val point = text.codePointAt(offset)
        if (point != previous) appendCodePoint(point)
        previous = point
        offset += Character.charCount(point)
    }
}

private fun snapToTokenEnd(text: String, cut: Int): Int {
    if (cut == 0) return cut
    val previous = text.codePointBefore(cut)
    if (Character.isWhitespace(previous)) return cut
    var offset = cut
    while (offset < text.length) {
        val point = text.codePointAt(offset)
        if (Character.isWhitespace(point)) return offset
        offset += Character.charCount(point)
    }
    return text.length
}

private fun String.substringBeforeLastWhitespace(): String {
    var offset = length
    while (offset > 0) {
        val point = codePointBefore(offset)
        offset -= Character.charCount(point)
        if (Character.isWhitespace(point)) return substring(0, offset)
    }
    return ""
}
