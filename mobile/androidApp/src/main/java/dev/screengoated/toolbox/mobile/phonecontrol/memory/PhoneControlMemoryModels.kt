package dev.screengoated.toolbox.mobile.phonecontrol.memory

import kotlinx.serialization.Serializable

internal const val PHONE_CONTROL_MEMORY_SCHEMA_VERSION: Int = 1
internal const val PHONE_CONTROL_MEMORY_MAX_FINALIZED_SESSIONS: Int = 20

@Serializable
internal enum class PhoneControlMemoryRole {
    USER,
    ASSISTANT,
}

/** Input supplied by the structural turn assembler. No role is inferred from text. */
internal data class PhoneControlMemoryRecordInput(
    val recordId: String,
    val turnId: String,
    val role: PhoneControlMemoryRole,
    val text: String,
    val createdAtEpochMs: Long,
)

/** One committed turn. Both records are written by one atomic sidecar replace. */
internal data class PhoneControlMemoryTurnInput(
    val turnId: String,
    val user: PhoneControlMemoryRecordInput,
    val assistant: PhoneControlMemoryRecordInput,
)

@Serializable
internal data class PhoneControlMemoryRecord(
    val schemaVersion: Int = PHONE_CONTROL_MEMORY_SCHEMA_VERSION,
    val ordinal: Long,
    val recordId: String,
    val turnId: String,
    val role: PhoneControlMemoryRole,
    val text: String,
    val createdAtEpochMs: Long,
)

@Serializable
internal data class PhoneControlMemorySession(
    val schemaVersion: Int = PHONE_CONTROL_MEMORY_SCHEMA_VERSION,
    val revision: Long,
    val sessionId: String,
    val startedAtEpochMs: Long,
    val finalizedAtEpochMs: Long? = null,
    val records: List<PhoneControlMemoryRecord> = emptyList(),
)

@Serializable
internal data class PhoneControlMemorySummary(
    val schemaVersion: Int = PHONE_CONTROL_MEMORY_SCHEMA_VERSION,
    val sessionId: String,
    val startedAtEpochMs: Long,
    val finalizedAtEpochMs: Long,
    val recordCount: Int,
    val title: String,
    val snippet: String,
)

/** Finalized text and structural metadata ready for a later search/embedding layer. */
internal data class PhoneControlMemorySearchRecord(
    val summary: PhoneControlMemorySummary,
    val records: List<PhoneControlMemoryRecord>,
    val searchText: String,
)

@Serializable
internal data class PhoneControlMemoryIndex(
    val schemaVersion: Int = PHONE_CONTROL_MEMORY_SCHEMA_VERSION,
    val revision: Long = 0L,
    val sessions: List<PhoneControlMemorySummary> = emptyList(),
)

internal fun PhoneControlMemoryRecordInput.toStored(ordinal: Long): PhoneControlMemoryRecord {
    return PhoneControlMemoryRecord(
        ordinal = ordinal,
        recordId = recordId,
        turnId = turnId,
        role = role,
        text = text,
        createdAtEpochMs = createdAtEpochMs,
    )
}

internal fun PhoneControlMemoryRecord.matches(input: PhoneControlMemoryRecordInput): Boolean {
    return recordId == input.recordId &&
        turnId == input.turnId &&
        role == input.role &&
        text == input.text &&
        createdAtEpochMs == input.createdAtEpochMs
}

internal fun List<PhoneControlMemoryRecord>.containsOnlyCompleteTurns(): Boolean {
    if (isEmpty() || size % 2 != 0) return false
    val pairs = chunked(2)
    if (pairs.map { it[0].turnId }.toSet().size != pairs.size) return false
    return pairs.all { pair ->
        val user = pair[0]
        val assistant = pair[1]
        user.turnId == assistant.turnId &&
            user.role == PhoneControlMemoryRole.USER &&
            assistant.role == PhoneControlMemoryRole.ASSISTANT
    }
}

internal fun List<PhoneControlMemoryRecord>.completeTurnPrefix(): List<PhoneControlMemoryRecord> {
    val committed = mutableListOf<PhoneControlMemoryRecord>()
    val committedTurnIds = mutableSetOf<String>()
    for (pair in chunked(2)) {
        if (pair.size != 2) break
        val user = pair[0]
        val assistant = pair[1]
        if (
            user.turnId != assistant.turnId ||
            user.role != PhoneControlMemoryRole.USER ||
            assistant.role != PhoneControlMemoryRole.ASSISTANT
        ) {
            break
        }
        if (!committedTurnIds.add(user.turnId)) break
        committed += pair
    }
    return committed
}
