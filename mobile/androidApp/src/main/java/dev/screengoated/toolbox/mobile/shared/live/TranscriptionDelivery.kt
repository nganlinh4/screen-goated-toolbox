package dev.screengoated.toolbox.mobile.shared.live

import java.util.Locale

/** Port of api/gemini_transcribe/stabilization.rs; no destination-specific rules. */
internal class TranscriptionDelivery {
    var committed = ""
        private set
    var interim = ""
        private set
    private var frozen = 0
    private var previous = emptyList<String>()
    private var snapshot = ""
    var finished = false
        private set

    @Synchronized fun update(raw: String, final: Boolean): String? {
        if (finished) return null
        if (!final) {
            snapshot = raw
        }
        frozen = maxOf(frozen, boundary(interim))
        interim = reconcile(interim, if (final) raw else raw.substring(overlapEnd(raw)), frozen)
        if (!final) return null
        val delta = segment(interim)
        committed += delta
        previous = listOf(raw, snapshot, interim)
        interim = ""
        snapshot = ""
        frozen = 0
        return delta
    }

    @Synchronized fun finishPending(): String {
        if (finished) return ""
        finished = true
        val delta = segment(interim)
        committed += delta
        interim = ""
        snapshot = ""
        previous = emptyList()
        frozen = 0
        return delta
    }

    @Synchronized fun display(): String = committed + segment(interim)
    @Synchronized fun provisional(): String = segment(interim)

    private fun segment(text: String): String = when {
        text.isEmpty() -> ""
        committed.isEmpty() -> text.trimStart()
        committed.last().isWhitespace() || text.first().isWhitespace() -> text
        else -> " $text"
    }

    private fun overlapEnd(text: String): Int {
        val next = words(text)
        val ranges = wordRanges(text)
        if (next.isEmpty()) return 0
        return previous.maxOfOrNull { source ->
            val old = words(source)
            (0 until (old.size - 2).coerceAtLeast(0)).maxOfOrNull { start ->
                (0 until minOf(next.size, 4)).maxOfOrNull { offset ->
                    val count = (0 until minOf(old.size - start, next.size - offset)).takeWhile { i ->
                        old[start + i] == next[offset + i] ||
                            (offset + i + 1 == next.size && old[start + i].startsWith(next[offset + i]))
                    }.size
                    val scalars = next.subList(offset, offset + count).sumOf { it.codePointCount(0, it.length) }
                    when {
                        count >= 3 && scalars >= 12 && start + count == old.size ->
                            ranges.getOrNull(offset + count)?.first ?: text.length
                        offset == 0 && count == next.size -> text.length
                        else -> 0
                    }
                } ?: 0
            } ?: 0
        } ?: 0
    }

    companion object {
        const val REVISION_WORDS = 10
        const val REVISION_SCALARS = 64
        private val wordPattern = Regex("[\\p{L}\\p{N}]")
        private val clusterPattern = Regex("\\X")
        private fun words(text: String) = wordRanges(text)
            .map { text.substring(it).lowercase(Locale.ROOT) }

        private fun wordRanges(text: String): List<IntRange> {
            val result = mutableListOf<IntRange>()
            var start: Int? = null
            for (cluster in clusterPattern.findAll(text)) {
                if (wordPattern.containsMatchIn(cluster.value)) {
                    if (start == null) start = cluster.range.first
                } else if (start != null) {
                    result.add(start until cluster.range.first)
                    start = null
                }
            }
            if (start != null) result.add(start until text.length)
            return result
        }

        private fun boundary(text: String): Int {
            val words = wordRanges(text)
            val wordBoundary = if (words.size > REVISION_WORDS) words[words.size - REVISION_WORDS].first else 0
            var remaining = REVISION_SCALARS
            var scalarBoundary = text.length
            for (cluster in clusterPattern.findAll(text).toList().asReversed()) {
                val count = cluster.value.codePointCount(0, cluster.value.length)
                if (count > remaining) break
                remaining -= count
                scalarBoundary = cluster.range.first
            }
            return maxOf(wordBoundary, scalarBoundary)
        }

        private fun reconcile(old: String, incoming: String, frozen: Int): String {
            val left = units(old)
            val right = units(incoming)
            val matches = mutableListOf<Pair<Int, Int>>()
            align(left, right, 0, 0, matches)
            matches.add(left.size to right.size)
            val result = StringBuilder()
            var a = 0
            var b = 0
            var offset = 0
            for ((x, y) in matches) {
                if (x != a || y != b) {
                    val source = if (offset < frozen) left.subList(a, x) else right.subList(b, y)
                    source.forEach(result::append)
                }
                offset += left.subList(a, x).sumOf { it.length }
                if (x < left.size) {
                    result.append(left[x])
                    offset += left[x].length
                }
                a = x + 1
                b = y + 1
            }
            check(result.startsWith(old.substring(0, frozen)))
            return result.toString()
        }

        private fun units(text: String): List<String> {
            val result = mutableListOf<String>()
            var cursor = 0
            for (range in wordRanges(text)) {
                val word = text.substring(range)
                result.addAll(clusterPattern.findAll(text.substring(cursor, range.first)).map { it.value })
                if (word.codePointCount(0, word.length) <= REVISION_SCALARS) {
                    result.add(word)
                } else {
                    result.addAll(clusterPattern.findAll(word).map { it.value })
                }
                cursor = range.last + 1
            }
            result.addAll(clusterPattern.findAll(text.substring(cursor)).map { it.value })
            return result
        }

        private fun scores(left: List<String>, right: List<String>, reverse: Boolean): IntArray {
            val row = IntArray(right.size + 1)
            for (i in left.indices) {
                var diagonal = 0
                for (j in right.indices) {
                    val above = row[j + 1]
                    val a = left[if (reverse) left.lastIndex - i else i]
                    val b = right[if (reverse) right.lastIndex - j else j]
                    row[j + 1] = if (a == b) diagonal + 1 else maxOf(above, row[j])
                    diagonal = above
                }
            }
            return row
        }

        private fun align(left: List<String>, right: List<String>, a: Int, b: Int, out: MutableList<Pair<Int, Int>>) {
            val prefix = left.zip(right).takeWhile { it.first == it.second }.size
            for (i in 0 until prefix) out.add(a + i to b + i)
            val l = left.drop(prefix)
            val r = right.drop(prefix)
            val aa = a + prefix
            val bb = b + prefix
            if (l.isEmpty() || r.isEmpty()) return
            if (l.size == 1) {
                val index = r.indexOf(l[0])
                if (index >= 0) out.add(aa to bb + index)
                return
            }
            val mid = l.size / 2
            val forward = scores(l.take(mid), r, false)
            val backward = scores(l.drop(mid), r, true)
            var split = 0
            for (i in 0..r.size) {
                if (forward[i] + backward[r.size - i] >= forward[split] + backward[r.size - split]) split = i
            }
            align(l.take(mid), r.take(split), aa, bb, out)
            align(l.drop(mid), r.drop(split), aa + mid, bb + split, out)
        }
    }
}
