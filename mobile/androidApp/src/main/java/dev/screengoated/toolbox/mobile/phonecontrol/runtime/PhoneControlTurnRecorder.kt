package dev.screengoated.toolbox.mobile.phonecontrol.runtime

/** Structural transcript hooks. Implementations persist identity, never infer it from text. */
internal interface PhoneControlTurnRecorder {
    fun turnStarted(turnId: Long, generation: Long)

    fun userTranscriptUpdated(turnId: Long, text: String)

    fun assistantTranscriptUpdated(turnId: Long, text: String)

    fun turnCompleted(
        turnId: Long,
        userText: String,
        assistantText: String,
    )

    fun turnInterrupted(turnId: Long)
}

internal object NoOpPhoneControlTurnRecorder : PhoneControlTurnRecorder {
    override fun turnStarted(turnId: Long, generation: Long) = Unit

    override fun userTranscriptUpdated(turnId: Long, text: String) = Unit

    override fun assistantTranscriptUpdated(turnId: Long, text: String) = Unit

    override fun turnCompleted(turnId: Long, userText: String, assistantText: String) = Unit

    override fun turnInterrupted(turnId: Long) = Unit
}

internal class CompositePhoneControlTurnRecorder(
    private val delegates: List<PhoneControlTurnRecorder>,
) : PhoneControlTurnRecorder {
    override fun turnStarted(turnId: Long, generation: Long) {
        delegates.forEach { it.turnStarted(turnId, generation) }
    }

    override fun userTranscriptUpdated(turnId: Long, text: String) {
        delegates.forEach { it.userTranscriptUpdated(turnId, text) }
    }

    override fun assistantTranscriptUpdated(turnId: Long, text: String) {
        delegates.forEach { it.assistantTranscriptUpdated(turnId, text) }
    }

    override fun turnCompleted(turnId: Long, userText: String, assistantText: String) {
        delegates.forEach { it.turnCompleted(turnId, userText, assistantText) }
    }

    override fun turnInterrupted(turnId: Long) {
        delegates.forEach { it.turnInterrupted(turnId) }
    }
}
