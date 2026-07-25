package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import java.util.concurrent.ConcurrentHashMap

/** Records turn identity and timing without persisting transcript content. */
internal class PhoneControlDiagnosticTurnRecorder(
    private val delegate: PhoneControlTurnRecorder,
    private val nanoTime: () -> Long = System::nanoTime,
) : PhoneControlTurnRecorder {
    private val starts = ConcurrentHashMap<Long, Long>()

    override fun turnStarted(turnId: Long, generation: Long) {
        starts[turnId] = nanoTime()
        Log.i(TAG, "turn_started turn_id=$turnId generation=$generation")
        delegate.turnStarted(turnId, generation)
    }

    override fun userTranscriptUpdated(turnId: Long, text: String) {
        delegate.userTranscriptUpdated(turnId, text)
    }

    override fun assistantTranscriptUpdated(turnId: Long, text: String) {
        delegate.assistantTranscriptUpdated(turnId, text)
    }

    override fun turnCompleted(turnId: Long, userText: String, assistantText: String) {
        Log.i(
            TAG,
            "turn_completed turn_id=$turnId elapsed_ms=${elapsedMs(turnId)} " +
                "user_chars=${userText.length} assistant_chars=${assistantText.length}",
        )
        delegate.turnCompleted(turnId, userText, assistantText)
    }

    override fun turnInterrupted(turnId: Long) {
        Log.i(TAG, "turn_interrupted turn_id=$turnId elapsed_ms=${elapsedMs(turnId)}")
        delegate.turnInterrupted(turnId)
    }

    private fun elapsedMs(turnId: Long): Long {
        val started = starts.remove(turnId) ?: return 0L
        return ((nanoTime() - started) / 1_000_000L).coerceAtLeast(0L)
    }

    private companion object {
        const val TAG = "SGTPhoneControlTurn"
    }
}
