package dev.screengoated.toolbox.mobile.service.preset

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** One Main consumer owns Accessibility I/O; provider callbacks only enqueue bounded events. */
internal class ProvisionalPasteDelivery(
    scope: CoroutineScope,
    private val session: ProvisionalPasteSession,
    private val onChanged: () -> Unit = {},
) {
    private val mailbox = ProvisionalPasteMailbox()
    private val wake = Channel<Unit>(Channel.CONFLATED)
    private val worker = scope.launch {
        try {
            for (signal in wake) {
                while (true) {
                    when (val event = mailbox.poll() ?: break) {
                        is ProvisionalPasteEvent.Interim -> {
                            session.interim(event.text)
                            onChanged()
                            delay(100)
                        }
                        is ProvisionalPasteEvent.Final -> session.finalSegment(event.text)
                        ProvisionalPasteEvent.Finish -> return@launch
                    }
                    onChanged()
                }
            }
        } finally {
            mailbox.close(discard = true)
            session.close()
            onChanged()
            wake.close()
        }
    }

    fun interim(text: String) = enqueue(ProvisionalPasteEvent.Interim(text))
    fun finalSegment(text: String) = enqueue(ProvisionalPasteEvent.Final(text))

    fun beginDrain() {
        mailbox.beginDrain()
        session.beginDrain()
    }

    suspend fun finish() {
        mailbox.close(discard = false)
        wake.trySend(Unit)
        worker.join()
    }

    fun cancel() {
        mailbox.close(discard = true)
        worker.cancel()
        // Close synchronously before a newer capture can acquire an overlapping range.
        session.close()
        onChanged()
    }

    private fun enqueue(event: ProvisionalPasteEvent) {
        if (mailbox.enqueue(event)) wake.trySend(Unit)
    }
}
