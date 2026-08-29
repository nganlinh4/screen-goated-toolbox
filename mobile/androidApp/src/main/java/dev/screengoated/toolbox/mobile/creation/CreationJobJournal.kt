package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

@Serializable
internal data class CreationJournalContinuation(
    val engineId: String,
    val token: String,
    val sourcePath: String,
    val outputPath: String,
    val outputName: String,
    val createdAtMs: Long,
    val projectId: String = "",
    val revisionId: String = "",
    val supportedActions: List<String>? = null,
    val availableActions: List<String> = emptyList(),
)

@Serializable
internal data class CreationJournalRecord(
    val ownerId: String,
    val request: CreationWorkerRequest,
    val status: CreationJobStatus,
    val startedAtMs: Long,
    val destination: String? = null,
    val engineId: String? = null,
    val continuation: CreationJournalContinuation? = null,
)

internal class CreationJobJournal(context: Context) {
    private val directory = File(context.filesDir, "creation/state")
    private val target = File(directory, "accepted-jobs.json")
    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    fun load(): List<CreationJournalRecord> {
        if (!target.isFile) return emptyList()
        return runCatching {
            val text = requireNotNull(
                readCreationIndexTextBounded(target, CREATION_JOURNAL_INDEX_MAX_BYTES),
            )
            val decoded = json.decodeFromString<List<CreationJournalRecord>>(text)
            require(
                decoded.size <= CREATION_JOURNAL_MAXIMUM_RECORDS &&
                    decoded.distinctBy { it.request.jobId }.size == decoded.size &&
                    decoded.distinctBy { it.request.dispatchId }.size == decoded.size,
            )
            decoded.map(::migrateRecord)
        }.getOrElse {
            throw IllegalStateException("Creation state is unavailable", it)
        }
    }

    private fun migrateRecord(record: CreationJournalRecord): CreationJournalRecord {
        val tool = CreationTool.fromWireName(record.request.tool) ?: return record
        val operation = CreationContract.normalizedOperation(tool, record.request.operation)
            ?: record.request.operation
        return record.copy(
            request = record.request.copy(operation = operation),
            status = record.status.copy(
                operation = CreationContract.normalizedOperation(tool, record.status.operation),
            ),
        )
    }

    fun save(records: Collection<CreationJournalRecord>) {
        directory.mkdirs()
        require(records.size <= CREATION_JOURNAL_MAXIMUM_RECORDS) {
            "Creation state exceeds capacity"
        }
        val encoded = json.encodeToString(records)
        require(encoded.encodeToByteArray().size <= CREATION_JOURNAL_INDEX_MAX_BYTES) {
            "Creation state is too large"
        }
        writeCreationIndexTextAtomically(target, encoded, CREATION_JOURNAL_INDEX_MAX_BYTES)
    }
}

internal const val CREATION_JOURNAL_MAXIMUM_RECORDS = 384

internal fun selectCreationJournalRecords(
    records: Collection<CreationJournalRecord>,
    maximumRecords: Int,
): List<CreationJournalRecord> {
    require(maximumRecords > 0)
    val active = records.filter { creationStageIsBusy(it.status.stage) }
    require(active.size <= maximumRecords) { "Active creation state exceeds capacity" }
    val terminal = records.asSequence()
        .filterNot { creationStageIsBusy(it.status.stage) }
        .sortedByDescending(CreationJournalRecord::startedAtMs)
        .take(maximumRecords - active.size)
    return (active.asSequence() + terminal)
        .sortedBy(CreationJournalRecord::startedAtMs)
        .toList()
}
