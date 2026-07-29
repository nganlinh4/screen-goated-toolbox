package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import java.util.UUID
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

@Serializable
internal data class CreationHistoryRenameReceipt(
    val transactionId: String,
    val entryId: String,
    val oldPath: String,
    val oldName: String,
    val targetName: String,
    val expectedSize: Long,
    val expectedSha256: String,
    val oldIdentity: String = "",
    val newPath: String? = null,
    val newIdentity: String? = null,
    val committed: Boolean = false,
)

internal class CreationHistoryRenameStore(
    context: Context,
    private val files: CreationFileStore,
) {
    private val target = File(context.filesDir, "creation/state/history-renames.json")
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }
    private val lock = Any()

    fun rename(
        entry: CreationHistoryEntry,
        requestedName: String,
        commit: (CreationHistoryEntry) -> Unit,
    ): CreationHistoryEntry = synchronized(lock) {
        val size = files.size(entry.outputPath)
        require(size >= 0L) { "Saved result is unavailable" }
        val digest = files.sha256(entry.outputPath)
        val oldIdentity = requireNotNull(files.artifactIdentity(entry.outputPath)) {
            "Saved result identity is unavailable"
        }
        val transactionId = UUID.randomUUID().toString()
        val targetName = files.outputs.planHistoryRenameName(
            entry.outputPath,
            requestedName,
            transactionId,
        )
        var receipt = CreationHistoryRenameReceipt(
            transactionId,
            entry.id,
            entry.outputPath,
            entry.outputName,
            targetName,
            size,
            digest,
            oldIdentity,
        )
        val records = read().toMutableList()
        require(records.none { it.entryId == entry.id }) { "A rename is already pending" }
        records += receipt
        write(records)
        val renamed = files.outputs.renameForHistory(
            receipt.oldPath,
            receipt.targetName,
            receipt.oldIdentity,
            receipt.expectedSize,
            receipt.expectedSha256,
        )
        receipt = receipt.copy(
            newPath = renamed.first,
            newIdentity = requireNotNull(files.artifactIdentity(renamed.first)),
        )
        replace(records, receipt)
        write(records)
        val updated = entry.copy(
            outputPath = renamed.first,
            outputName = renamed.second,
            committedIdentity = receipt.newIdentity,
        )
        commit(updated)
        receipt = receipt.copy(committed = true)
        replace(records, receipt)
        write(records)
        finish(records, receipt)
        updated
    }

    fun recover(
        entries: List<CreationHistoryEntry>,
        commitAll: (List<CreationHistoryEntry>) -> Unit,
    ): List<CreationHistoryEntry> = synchronized(lock) {
        val records = read().toMutableList()
        if (records.isEmpty()) return entries
        var updated = entries
        records.toList().forEach { saved ->
            runCatching {
                var receipt = saved
                var entry = updated.firstOrNull { it.id == receipt.entryId }
                    ?: return@runCatching
                if (receipt.newPath == null) {
                    val renamed = files.outputs.renameForHistory(
                        receipt.oldPath,
                        receipt.targetName,
                        receipt.oldIdentity,
                        receipt.expectedSize,
                        receipt.expectedSha256,
                    )
                    receipt = receipt.copy(
                        newPath = renamed.first,
                        newIdentity = requireNotNull(files.artifactIdentity(renamed.first)),
                    )
                    replace(records, receipt)
                    write(records)
                }
                val newPath = requireNotNull(receipt.newPath)
                require(
                    creationRenameArtifactIsVerified(
                        receipt,
                        files.artifactIdentity(newPath),
                        files.size(newPath),
                        files.sha256(newPath),
                    ),
                ) { "Renamed result verification failed" }
                if (creationRenameRecoveryMustCommitHistory(receipt, entry)) {
                    entry = entry.copy(
                        outputPath = newPath,
                        outputName = receipt.targetName,
                        committedIdentity = receipt.newIdentity,
                    )
                    updated = updated.map { if (it.id == entry.id) entry else it }
                    commitAll(updated)
                }
                if (!receipt.committed) {
                    receipt = receipt.copy(committed = true)
                    replace(records, receipt)
                    write(records)
                }
                finish(records, receipt)
            }
        }
        updated
    }

    private fun finish(
        records: MutableList<CreationHistoryRenameReceipt>,
        receipt: CreationHistoryRenameReceipt,
    ) {
        if (!receipt.committed) return
        if (receipt.newPath == receipt.oldPath ||
            receipt.newIdentity == receipt.oldIdentity
        ) {
            records.removeAll { it.transactionId == receipt.transactionId }
            write(records)
            return
        }
        val oldExists = files.exists(receipt.oldPath)
        val stillExact = oldExists &&
            files.artifactIdentity(receipt.oldPath) == receipt.oldIdentity &&
            files.size(receipt.oldPath) == receipt.expectedSize &&
            runCatching {
                files.sha256(receipt.oldPath).equals(
                    receipt.expectedSha256,
                    ignoreCase = true,
                )
            }.getOrDefault(false)
        val removed = !oldExists || !stillExact || files.delete(receipt.oldPath)
        if (!removed) return
        records.removeAll { it.transactionId == receipt.transactionId }
        write(records)
    }

    private fun read(): List<CreationHistoryRenameReceipt> {
        if (!target.exists()) return emptyList()
        val text = requireNotNull(
            readCreationIndexTextBounded(target, CREATION_RENAME_INDEX_MAX_BYTES),
        ) { "Creation rename state is unavailable" }
        val decoded = json.decodeFromString<List<CreationHistoryRenameReceipt>>(text)
        require(
            decoded.size <= CREATION_RENAME_MAXIMUM_RECORDS &&
                decoded.map(CreationHistoryRenameReceipt::entryId).distinct().size == decoded.size,
        )
        return decoded
    }

    private fun write(records: List<CreationHistoryRenameReceipt>) {
        require(records.size <= CREATION_RENAME_MAXIMUM_RECORDS)
        writeCreationIndexTextAtomically(
            target,
            json.encodeToString(records),
            CREATION_RENAME_INDEX_MAX_BYTES,
        )
    }

    private fun replace(
        records: MutableList<CreationHistoryRenameReceipt>,
        receipt: CreationHistoryRenameReceipt,
    ) {
        records.removeAll { it.transactionId == receipt.transactionId }
        records += receipt
    }
}

internal const val CREATION_RENAME_INDEX_MAX_BYTES = 4L * 1024 * 1024
private const val CREATION_RENAME_MAXIMUM_RECORDS = 384

internal fun creationRenameRecoveryMustCommitHistory(
    receipt: CreationHistoryRenameReceipt,
    entry: CreationHistoryEntry,
): Boolean = receipt.newPath != null &&
    (entry.outputPath != receipt.newPath || entry.outputName != receipt.targetName)

internal fun creationRenameArtifactIsVerified(
    receipt: CreationHistoryRenameReceipt,
    actualIdentity: String?,
    actualSize: Long,
    actualSha256: String?,
): Boolean = receipt.newIdentity != null &&
    actualIdentity == receipt.newIdentity &&
    actualSize == receipt.expectedSize &&
    actualSha256?.equals(receipt.expectedSha256, ignoreCase = true) == true
