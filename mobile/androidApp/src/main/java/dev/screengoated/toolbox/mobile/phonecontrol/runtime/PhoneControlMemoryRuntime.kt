package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRecordInput
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRepository
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRole
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryTurnInput
import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean

/** Owns the one process-start recovery pass performed by AppContainer. */
internal class PhoneControlMemoryStartup(
    private val repository: PhoneControlMemoryRepository,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    private val recovered = AtomicBoolean(false)

    fun recoverOnce(): Boolean {
        if (!recovered.compareAndSet(false, true)) return false
        return try {
            repository.recoverStaleDrafts(clock())
            true
        } catch (error: Throwable) {
            recovered.set(false)
            throw error
        }
    }
}

/** Persists only structurally complete turns; transcript text never determines identity. */
internal class PhoneControlMemoryTurnRecorder(
    private val repository: PhoneControlMemoryRepository,
    private val clock: () -> Long = System::currentTimeMillis,
    val sessionId: String = UUID.randomUUID().toString(),
) : PhoneControlTurnRecorder {
    private data class Draft(
        val turnId: Long,
        val userCreatedAt: Long,
        var assistantCreatedAt: Long? = null,
        var userText: String = "",
        var assistantText: String = "",
        var completed: Boolean = false,
        var persisted: Boolean = false,
        var persistedUserText: String = "",
    )

    private val drafts = linkedMapOf<Long, Draft>()
    private var persistedTurns = 0
    private var finalized = false

    init {
        require(sessionId.isNotBlank()) { "sessionId must not be blank" }
    }

    @Synchronized
    override fun turnStarted(turnId: Long, generation: Long) {
        if (finalized) return
        drafts.entries.removeAll { (id, draft) -> id != turnId && draft.persisted }
        drafts.putIfAbsent(turnId, Draft(turnId = turnId, userCreatedAt = clock()))
    }

    @Synchronized
    override fun userTranscriptUpdated(turnId: Long, text: String) {
        if (finalized) return
        val draft = drafts[turnId] ?: return
        draft.userText = text
        if (draft.persisted && draft.persistedUserText != text) reviseUser(draft)
    }

    @Synchronized
    override fun assistantTranscriptUpdated(turnId: Long, text: String) {
        if (finalized) return
        val draft = drafts[turnId] ?: return
        if (draft.assistantCreatedAt == null) draft.assistantCreatedAt = clock()
        draft.assistantText = text
    }

    @Synchronized
    override fun turnCompleted(turnId: Long, userText: String, assistantText: String) {
        if (finalized) return
        val draft = drafts.getOrPut(turnId) {
            Draft(turnId = turnId, userCreatedAt = clock())
        }
        draft.userText = userText
        draft.assistantText = assistantText
        if (draft.assistantCreatedAt == null) draft.assistantCreatedAt = clock()
        draft.completed = true
        if (userText.isBlank() && assistantText.isBlank()) {
            drafts.remove(turnId)
        } else if (!draft.persisted) {
            persist(draft)
        } else if (draft.persistedUserText != userText) {
            reviseUser(draft)
        }
    }

    @Synchronized
    override fun turnInterrupted(turnId: Long) {
        if (!finalized) drafts.remove(turnId)
    }

    @Synchronized
    fun finalizeSession() {
        if (finalized) return
        drafts.values.filter { it.completed && !it.persisted }.forEach(::persist)
        drafts.values.filter { it.persisted && it.persistedUserText != it.userText }
            .forEach(::reviseUser)
        if (persistedTurns > 0) {
            runCatching { repository.finalize(sessionId, clock()) }
                .onFailure { error -> logFailure("finalize", null, error) }
        }
        finalized = true
        drafts.clear()
    }

    private fun persist(draft: Draft) {
        val turnId = draft.turnId.toString()
        val assistantCreatedAt = draft.assistantCreatedAt ?: clock()
        val input = PhoneControlMemoryTurnInput(
            turnId = turnId,
            user = PhoneControlMemoryRecordInput(
                recordId = "$sessionId/$turnId/user",
                turnId = turnId,
                role = PhoneControlMemoryRole.USER,
                text = draft.userText,
                createdAtEpochMs = draft.userCreatedAt,
            ),
            assistant = PhoneControlMemoryRecordInput(
                recordId = "$sessionId/$turnId/assistant",
                turnId = turnId,
                role = PhoneControlMemoryRole.ASSISTANT,
                text = draft.assistantText,
                createdAtEpochMs = assistantCreatedAt,
            ),
        )
        runCatching { repository.append(sessionId, input) }
            .onSuccess {
                draft.persisted = true
                draft.persistedUserText = draft.userText
                persistedTurns += 1
            }
            .onFailure { error -> logFailure("append", draft.turnId, error) }
    }

    private fun reviseUser(draft: Draft) {
        runCatching {
            repository.reviseUserText(sessionId, draft.turnId.toString(), draft.userText)
        }
            .onSuccess { draft.persistedUserText = draft.userText }
            .onFailure { error -> logFailure("revise", draft.turnId, error) }
    }

    private fun logFailure(operation: String, turnId: Long?, error: Throwable) {
        Log.e(TAG, "memory_$operation failed turn_id=${turnId ?: 0L}", error)
    }

    private companion object {
        const val TAG = "SGTPhoneControlMemory"
    }
}
