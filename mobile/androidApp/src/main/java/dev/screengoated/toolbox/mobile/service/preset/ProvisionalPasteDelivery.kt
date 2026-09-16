package dev.screengoated.toolbox.mobile.service.preset

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/** One Main consumer owns Accessibility I/O; provider callbacks only enqueue bounded events. */
internal class ProvisionalPasteDelivery(
    scope: CoroutineScope,
    session: ProvisionalPasteSession,
    capture: (() -> ProvisionalPasteTarget?)? = null,
    private val onChanged: () -> Unit = {},
) {
    private val routing = RebindingPasteSession(session, capture)
    val attempted: Boolean get() = routing.attempted
    private var finishDeadline: Long? = null
    private val mailbox = ProvisionalPasteMailbox()
    private val wake = Channel<Unit>(Channel.CONFLATED)
    private val worker = scope.launch {
        try {
            while (true) {
                if (finishDeadline?.let { System.nanoTime() >= it } == true) break
                val event = mailbox.peek()
                if (event == ProvisionalPasteEvent.Finish) break
                if (event == null) { withTimeoutOrNull(50) { wake.receive() }; continue }
                if (routing.deliver(event)) mailbox.consume(event)
                onChanged()
                delay(if (event is ProvisionalPasteEvent.Interim) 100 else 50)
            }
        } finally {
            mailbox.close(discard = true)
            routing.close()
            onChanged()
            wake.close()
        }
    }

    fun interim(text: String) = enqueue(ProvisionalPasteEvent.Interim(text))
    fun finalSegment(text: String) = enqueue(ProvisionalPasteEvent.Final(text))

    fun beginDrain() {
        mailbox.beginDrain()
    }

    suspend fun finish() {
        finishDeadline = System.nanoTime() + 2_000_000_000
        mailbox.close(discard = false)
        wake.trySend(Unit)
        worker.join()
    }

    fun cancel() {
        mailbox.close(discard = true)
        worker.cancel()
        // Close synchronously before a newer capture can acquire an overlapping range.
        routing.close()
        onChanged()
    }

    private fun enqueue(event: ProvisionalPasteEvent) {
        if (mailbox.enqueue(event)) wake.trySend(Unit)
    }
}
