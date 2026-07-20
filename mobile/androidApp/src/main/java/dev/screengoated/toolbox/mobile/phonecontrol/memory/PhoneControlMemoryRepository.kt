package dev.screengoated.toolbox.mobile.phonecontrol.memory

import android.content.Context
import kotlinx.serialization.json.Json
import java.io.File

internal class PhoneControlMemoryRepository internal constructor(
    root: File,
    private val maxFinalizedSessions: Int = PHONE_CONTROL_MEMORY_MAX_FINALIZED_SESSIONS,
    json: Json = DEFAULT_JSON,
) {
    private val lock = Any()
    private val store = PhoneControlMemoryAtomicStore(root, json)

    constructor(context: Context) : this(
        root = File(context.applicationContext.noBackupFilesDir, "phone-control-memory"),
    )

    init {
        require(maxFinalizedSessions > 0) { "maxFinalizedSessions must be positive" }
        synchronized(lock) {
            store.recoverInterruptedWrites()
            reconcileIndexLocked()
        }
    }

    /**
     * Atomically appends one complete structurally identified user/assistant turn.
     * Reusing the same turn and record IDs with identical data is an idempotent retry.
     */
    fun append(
        sessionId: String,
        turn: PhoneControlMemoryTurnInput,
    ): PhoneControlMemorySession = synchronized(lock) {
        validateTurnInput(sessionId, turn)
        val current = store.readSession(sessionId)
        require(current?.finalizedAtEpochMs == null) { "Cannot append to a finalized session" }
        if (current != null) {
            require(current.records.containsOnlyCompleteTurns()) {
                "Session requires stale-draft recovery before append"
            }
        }
        val existingPair = current?.records.orEmpty().filter { it.turnId == turn.turnId }
        if (existingPair.isNotEmpty()) {
            require(existingPair.size == 2) { "Stored turn is structurally incomplete" }
            require(existingPair[0].matches(turn.user) && existingPair[1].matches(turn.assistant)) {
                "Turn ID collides with different structural data"
            }
            return@synchronized requireNotNull(current)
        }
        val newRecordIds = setOf(turn.user.recordId, turn.assistant.recordId)
        require(
            current?.records.orEmpty().none { it.recordId in newRecordIds },
        ) { "Record ID collides with an existing turn" }
        val nextOrdinal = current?.records?.size?.toLong() ?: 0L
        val records = current?.records.orEmpty() + listOf(
            turn.user.toStored(nextOrdinal),
            turn.assistant.toStored(nextOrdinal + 1L),
        )
        val updated = PhoneControlMemorySession(
            revision = (current?.revision ?: -1L) + 1L,
            sessionId = sessionId,
            startedAtEpochMs = current?.startedAtEpochMs ?: turn.user.createdAtEpochMs,
            records = records,
        )
        store.writeSession(updated)
        updated
    }

    /** Atomically replaces only the USER text of one complete turn in a live draft. */
    fun reviseUserText(
        sessionId: String,
        turnId: String,
        revisedText: String,
    ): PhoneControlMemorySession = synchronized(lock) {
        require(sessionId.isNotBlank()) { "sessionId must not be blank" }
        require(turnId.isNotBlank()) { "turnId must not be blank" }
        val current = requireNotNull(store.readSession(sessionId)) { "Unknown session" }
        require(current.finalizedAtEpochMs == null) { "Cannot revise a finalized session" }
        require(current.records.containsOnlyCompleteTurns()) { "Session has an incomplete turn" }
        val userIndex = current.records.indexOfFirst {
            it.turnId == turnId && it.role == PhoneControlMemoryRole.USER
        }
        require(userIndex >= 0) { "Unknown turn" }
        val assistant = current.records.getOrNull(userIndex + 1)
        require(
            assistant?.turnId == turnId && assistant.role == PhoneControlMemoryRole.ASSISTANT,
        ) { "Stored turn is structurally incomplete" }
        val user = current.records[userIndex]
        if (user.text == revisedText) return@synchronized current
        val revisedRecords = current.records.toMutableList().apply {
            this[userIndex] = user.copy(text = revisedText)
        }
        val revised = current.copy(
            revision = current.revision + 1L,
            records = revisedRecords,
        )
        store.writeSession(revised)
        revised
    }

    /** Finalizes once. Only finalized sessions become list/get/search visible. */
    fun finalize(
        sessionId: String,
        finalizedAtEpochMs: Long,
    ): PhoneControlMemorySession = synchronized(lock) {
        require(sessionId.isNotBlank()) { "sessionId must not be blank" }
        require(finalizedAtEpochMs >= 0L) { "finalizedAtEpochMs must be non-negative" }
        val current = requireNotNull(store.readSession(sessionId)) { "Unknown session" }
        require(current.records.containsOnlyCompleteTurns()) { "Session has an incomplete turn" }
        if (current.finalizedAtEpochMs != null) return@synchronized current
        val finalized = current.copy(
            revision = current.revision + 1L,
            finalizedAtEpochMs = finalizedAtEpochMs,
        )
        store.writeSession(finalized)
        reconcileIndexLocked()
        finalized
    }

    /**
     * Process-start recovery for sessions whose complete turns reached disk before a crash.
     * An incomplete tail is discarded; empty drafts are removed; recovered sessions finalize once.
     */
    fun recoverStaleDrafts(recoveredAtEpochMs: Long): List<PhoneControlMemorySession> = synchronized(lock) {
        require(recoveredAtEpochMs >= 0L) { "recoveredAtEpochMs must be non-negative" }
        val recovered = mutableListOf<PhoneControlMemorySession>()
        store.readAllSessions()
            .filter { it.finalizedAtEpochMs == null }
            .forEach { draft ->
                val completeRecords = draft.records.completeTurnPrefix()
                if (completeRecords.isEmpty()) {
                    store.deleteSession(draft.sessionId)
                } else {
                    val finalized = draft.copy(
                        revision = draft.revision + 1L,
                        finalizedAtEpochMs = recoveredAtEpochMs,
                        records = completeRecords,
                    )
                    store.writeSession(finalized)
                    recovered += finalized
                }
            }
        reconcileIndexLocked()
        recovered
    }

    fun list(limit: Int = maxFinalizedSessions): List<PhoneControlMemorySummary> = synchronized(lock) {
        require(limit >= 0) { "limit must be non-negative" }
        reconcileIndexLocked().take(limit)
    }

    fun get(sessionId: String): PhoneControlMemorySession? = synchronized(lock) {
        if (sessionId.isBlank()) return@synchronized null
        val session = store.readSession(sessionId) ?: return@synchronized null
        session.takeIf { it.finalizedAtEpochMs != null && it.records.containsOnlyCompleteTurns() }
    }

    fun searchReadyRecords(
        limit: Int = maxFinalizedSessions,
    ): List<PhoneControlMemorySearchRecord> = synchronized(lock) {
        require(limit >= 0) { "limit must be non-negative" }
        val summaries = reconcileIndexLocked().take(limit)
        summaries.mapNotNull { summary ->
            val session = store.readSession(summary.sessionId)
                ?.takeIf { it.finalizedAtEpochMs != null && it.records.containsOnlyCompleteTurns() }
                ?: return@mapNotNull null
            PhoneControlMemorySearchRecord(
                summary = summary,
                records = session.records,
                searchText = session.records.joinToString("\n") { it.text },
            )
        }
    }

    internal fun paths(): PhoneControlMemoryPaths = store.paths

    private fun reconcileIndexLocked(): List<PhoneControlMemorySummary> {
        val allSessions = store.readAllSessions()
        allSessions.filter {
            it.finalizedAtEpochMs != null && !it.records.containsOnlyCompleteTurns()
        }.forEach { store.quarantineSession(it.sessionId) }
        val finalized = allSessions
            .filter { it.finalizedAtEpochMs != null && it.records.containsOnlyCompleteTurns() }
            .sortedWith(
                compareByDescending<PhoneControlMemorySession> { it.finalizedAtEpochMs }
                    .thenByDescending { it.startedAtEpochMs }
                    .thenBy { it.sessionId },
            )
        val kept = finalized.take(maxFinalizedSessions)
        finalized.drop(maxFinalizedSessions).forEach { store.deleteSession(it.sessionId) }
        val summaries = kept.map(::summaryOf)
        val previous = store.readIndex()
        if (previous?.sessions != summaries) {
            store.writeIndex(
                PhoneControlMemoryIndex(
                    revision = (previous?.revision ?: 0L) + 1L,
                    sessions = summaries,
                ),
            )
        }
        return summaries
    }

    private fun summaryOf(session: PhoneControlMemorySession): PhoneControlMemorySummary {
        val finalizedAt = requireNotNull(session.finalizedAtEpochMs)
        val firstUserText = session.records.firstOrNull { it.role == PhoneControlMemoryRole.USER }?.text
        val title = firstUserText?.trim()?.takeCodePoints(MAX_TITLE_CHARS).orEmpty()
            .ifEmpty { "Conversation" }
        val snippet = session.records.joinToString(" • ") { it.text }
            .takeCodePoints(MAX_SNIPPET_CHARS)
        return PhoneControlMemorySummary(
            sessionId = session.sessionId,
            startedAtEpochMs = session.startedAtEpochMs,
            finalizedAtEpochMs = finalizedAt,
            recordCount = session.records.size,
            title = title,
            snippet = snippet,
        )
    }

    private fun validateTurnInput(sessionId: String, turn: PhoneControlMemoryTurnInput) {
        require(sessionId.isNotBlank()) { "sessionId must not be blank" }
        require(turn.turnId.isNotBlank()) { "turnId must not be blank" }
        require(turn.user.turnId == turn.turnId && turn.assistant.turnId == turn.turnId) {
            "Record turn IDs must match the committed turn"
        }
        require(turn.user.role == PhoneControlMemoryRole.USER) { "user record has the wrong role" }
        require(turn.assistant.role == PhoneControlMemoryRole.ASSISTANT) {
            "assistant record has the wrong role"
        }
        require(turn.user.recordId.isNotBlank() && turn.assistant.recordId.isNotBlank()) {
            "recordId must not be blank"
        }
        require(turn.user.recordId != turn.assistant.recordId) { "record IDs must be unique" }
        require(turn.user.createdAtEpochMs >= 0L && turn.assistant.createdAtEpochMs >= 0L) {
            "createdAtEpochMs must be non-negative"
        }
    }

    private companion object {
        private const val MAX_TITLE_CHARS = 80
        private const val MAX_SNIPPET_CHARS = 240
        private val DEFAULT_JSON = Json {
            encodeDefaults = true
            ignoreUnknownKeys = true
        }
    }
}

private fun String.takeCodePoints(maxCodePoints: Int): String {
    if (isEmpty() || maxCodePoints <= 0) return ""
    val end = offsetByCodePoints(0, codePointCount(0, length).coerceAtMost(maxCodePoints))
    return substring(0, end)
}
