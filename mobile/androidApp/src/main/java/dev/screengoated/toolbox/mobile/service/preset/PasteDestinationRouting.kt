package dev.screengoated.toolbox.mobile.service.preset

import java.text.BreakIterator
import java.util.Locale

/** Source boundaries never grant authority to mutate an abandoned destination. */
internal class PasteDestinationRouting {
    private var previous = ""
    private var cut = 0
    fun detach() { cut = previous.length }
    fun project(text: String): Pair<String, Int> {
        val mapped = mappedCut(previous, text, cut)
        return text.substring(mapped) to mapped
    }
    fun accept(event: ProvisionalPasteEvent, boundary: Int, replaceable: Boolean) {
        when (event) {
            is ProvisionalPasteEvent.Interim -> if (replaceable) { previous = event.text; cut = boundary }
            else -> { previous = ""; cut = 0 }
        }
    }

    private fun mappedCut(old: String, new: String, boundary: Int): Int {
        if (boundary == 0) return 0
        val left = words(old)
        val right = words(new)
        val equal = left.zip(right).takeWhile { it.first == it.second }.size
        val prefix = left.take(equal).sumOf(String::length)
        if (boundary <= prefix) return boundary
        val suffix = left.drop(equal).asReversed().zip(right.drop(equal).asReversed())
            .takeWhile { it.first == it.second }.sumOf { it.first.length }
        val oldEnd = old.length - suffix
        val newEnd = new.length - suffix
        return if (boundary >= oldEnd && oldEnd < old.length) newEnd + boundary - oldEnd else prefix
    }

    private fun words(text: String): List<String> {
        val iterator = BreakIterator.getWordInstance(Locale.ROOT)
        iterator.setText(text)
        var start = iterator.first()
        return buildList {
            var end = iterator.next()
            while (end != BreakIterator.DONE) {
                add(text.substring(start, end))
                start = end
                end = iterator.next()
            }
        }
    }
}
