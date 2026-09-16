package dev.screengoated.toolbox.mobile.service.preset

internal class RebindingPasteSession(
    initial: ProvisionalPasteSession,
    private val capture: (() -> ProvisionalPasteTarget?)?,
    private val now: () -> Long = { System.nanoTime() / 1_000_000 },
) {
    private var session: ProvisionalPasteSession? = initial
    private val routing = PasteDestinationRouting()
    private var candidate: ProvisionalPasteTarget? = null
    private var candidateSnapshot: PasteSnapshot? = null
    private var candidateSince = 0L
    private var closed = false
    var attempted = initial.attempted
        private set

    fun deliver(event: ProvisionalPasteEvent): Boolean {
        if (closed) return false
        val input = when (event) {
            is ProvisionalPasteEvent.Interim -> event.text
            is ProvisionalPasteEvent.Final -> event.text
            ProvisionalPasteEvent.Finish -> return true
        }
        if (input.length > MAX_PASTE_INPUT_UNITS) {
            close()
            return true
        }
        var active = session
        if (active != null && !active.ownsDestination()) {
            if (capture == null) return consumeWithoutTarget(event)
            detach()
            active = null
        }
        if (active == null) {
            val next = capture?.invoke()
            val snapshot = next?.snapshot()
            if (next == null || snapshot == null) { candidate = null; return false }
            if (candidate?.identity != next.identity || candidateSnapshot != snapshot) {
                candidate = next
                candidateSnapshot = snapshot
                candidateSince = now()
                return false
            }
            if (now() - candidateSince < 500) return false
            active = ProvisionalPasteSession(next)
            session = active
            candidate = null
        }
        val text = when (event) {
            is ProvisionalPasteEvent.Interim -> event.text
            is ProvisionalPasteEvent.Final -> event.text
            ProvisionalPasteEvent.Finish -> return true
        }
        val (tail, boundary) = routing.project(text)
        if (event is ProvisionalPasteEvent.Interim && !active.replaceable) return true
        val accepted = when (event) {
            is ProvisionalPasteEvent.Interim -> active.interim(tail)
            else -> active.finalSegment(tail)
        }
        attempted = attempted || active.attempted
        if (accepted || active.lastMutationAttempted) {
            routing.accept(event, boundary, active.replaceable)
        }
        if (!accepted) {
            if (capture == null) return true
            val uncertain = active.lastMutationAttempted
            detach()
            return uncertain
        }
        return true
    }

    private fun consumeWithoutTarget(event: ProvisionalPasteEvent): Boolean {
        if (event == ProvisionalPasteEvent.Finish) close()
        return true
    }

    private fun detach() {
        session?.abandon()
        session = null
        candidate = null
        routing.detach()
    }

    fun close() {
        if (closed) return
        closed = true
        session?.close()
        attempted = attempted || session?.attempted == true
        session = null
        candidate = null
    }
}
