package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import java.util.concurrent.ConcurrentHashMap

/**
 * Keeps app-originated control turns out of durable conversation memory.
 *
 * The predicate is structural runtime ownership, never transcript content.
 */
internal class PhoneControlPresentationAwareTurnRecorder(
    private val delegate: PhoneControlTurnRecorder,
    private val conversationSurfaceSuppressed: () -> Boolean,
) : PhoneControlTurnRecorder {
    private val suppressedTurns = ConcurrentHashMap.newKeySet<Long>()

    override fun turnStarted(turnId: Long, generation: Long) {
        if (conversationSurfaceSuppressed()) {
            suppressedTurns += turnId
        } else {
            delegate.turnStarted(turnId, generation)
        }
    }

    override fun userTranscriptUpdated(turnId: Long, text: String) {
        if (!isSuppressed(turnId)) {
            delegate.userTranscriptUpdated(turnId, text)
        }
    }

    override fun assistantTranscriptUpdated(turnId: Long, text: String) {
        if (!isSuppressed(turnId)) {
            delegate.assistantTranscriptUpdated(turnId, text)
        }
    }

    override fun turnCompleted(turnId: Long, userText: String, assistantText: String) {
        if (!suppressedTurns.remove(turnId) && !conversationSurfaceSuppressed()) {
            delegate.turnCompleted(turnId, userText, assistantText)
        }
    }

    override fun turnInterrupted(turnId: Long) {
        if (!suppressedTurns.remove(turnId) && !conversationSurfaceSuppressed()) {
            delegate.turnInterrupted(turnId)
        }
    }

    private fun isSuppressed(turnId: Long): Boolean =
        turnId in suppressedTurns || conversationSurfaceSuppressed()
}
