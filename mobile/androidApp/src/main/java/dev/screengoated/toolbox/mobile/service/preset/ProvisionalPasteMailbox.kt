package dev.screengoated.toolbox.mobile.service.preset

internal sealed interface ProvisionalPasteEvent {
    data class Interim(val text: String) : ProvisionalPasteEvent
    data class Final(val text: String) : ProvisionalPasteEvent
    data object Finish : ProvisionalPasteEvent
}

/** Adjacent hypotheses coalesce; final segments never reorder or disappear on a normal stop. */
internal class ProvisionalPasteMailbox {
    private val events = ArrayDeque<ProvisionalPasteEvent>()
    private var accepting = true
    private var draining = false

    @Synchronized
    fun enqueue(event: ProvisionalPasteEvent): Boolean {
        if (!accepting || (draining && event is ProvisionalPasteEvent.Interim)) return false
        if (event is ProvisionalPasteEvent.Interim && events.lastOrNull() is ProvisionalPasteEvent.Interim) {
            events.removeLast()
        }
        events.addLast(event)
        val bytes = events.sumOf {
            when (it) {
                is ProvisionalPasteEvent.Interim -> it.text.toByteArray(Charsets.UTF_8).size.toLong()
                is ProvisionalPasteEvent.Final -> it.text.toByteArray(Charsets.UTF_8).size.toLong()
                ProvisionalPasteEvent.Finish -> 0L
            }
        }
        if (events.size > MAX_EVENTS || bytes > MAX_BYTES) close(discard = true)
        return true
    }

    @Synchronized
    fun beginDrain() {
        draining = true
        events.removeAll { it is ProvisionalPasteEvent.Interim }
    }

    @Synchronized
    fun close(discard: Boolean) {
        if (discard) events.clear()
        if (accepting || discard) events.addLast(ProvisionalPasteEvent.Finish)
        accepting = false
    }

    @Synchronized
    fun poll(): ProvisionalPasteEvent? = events.removeFirstOrNull()
    @Synchronized
    fun peek(): ProvisionalPasteEvent? = events.firstOrNull()
    @Synchronized
    fun consume(event: ProvisionalPasteEvent) { if (events.firstOrNull() === event) events.removeFirst() }

    private companion object {
        const val MAX_EVENTS = 128
        const val MAX_BYTES = 128 * 1024
    }
}
