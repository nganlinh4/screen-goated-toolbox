package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import java.util.UUID
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

@Serializable
internal data class CreationHistoryCompanionRenameReceipt(
    val oldPath: String,
    val oldName: String,
    val targetName: String,
    val expectedSize: Long,
    val expectedSha256: String,
    val oldIdentity: String,
    val newPath: String? = null,
    val newIdentity: String? = null,
)

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
    val companion: CreationHistoryCompanionRenameReceipt? = null,
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
        val companionPath = entry.companionOutputPath()
        val companionName = entry.companionOutputName()
        val targetNames = files.outputs.planHistoryRenameNames(
            entry.outputPath,
            companionPath,
            companionName,
            requestedName,
            transactionId,
        )
        val companion = companionPath?.let { path ->
            val companionSize = files.size(path)
            require(companionSize >= 0L) { "Saved companion result is unavailable" }
            CreationHistoryCompanionRenameReceipt(
                oldPath = path,
                oldName = requireNotNull(companionName),
                targetName = requireNotNull(targetNames.second),
                expectedSize = companionSize,
                expectedSha256 = files.sha256(path),
                oldIdentity = requireNotNull(files.artifactIdentity(path)) {
                    "Saved companion result identity is unavailable"
                },
            )
        }
        var receipt = CreationHistoryRenameReceipt(
            transactionId,
            entry.id,
            entry.outputPath,
            entry.outputName,
            targetNames.first,
            size,
            digest,
            oldIdentity,
            companion = companion,
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
        receipt.companion?.takeIf { it.newPath == null }?.let { pending ->
            val renamedCompanion = files.outputs.renameForHistory(
                pending.oldPath,
                pending.targetName,
                pending.oldIdentity,
                pending.expectedSize,
                pending.expectedSha256,
            )
            receipt = receipt.copy(
                companion = pending.copy(
                    newPath = renamedCompanion.first,
                    newIdentity = requireNotNull(files.artifactIdentity(renamedCompanion.first)),
                ),
            )
            replace(records, receipt)
            write(records)
        }
        val updated = creationEntryWithCompletedRename(entry, receipt)
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
                receipt.companion?.takeIf { it.newPath == null }?.let { pending ->
                    val renamed = files.outputs.renameForHistory(
                        pending.oldPath,
                        pending.targetName,
                        pending.oldIdentity,
                        pending.expectedSize,
                        pending.expectedSha256,
                    )
                    receipt = receipt.copy(
                        companion = pending.copy(
                            newPath = renamed.first,
                            newIdentity = requireNotNull(files.artifactIdentity(renamed.first)),
                        ),
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
                require(creationRenameCompanionIsVerified(receipt.companion, files)) {
                    "Renamed companion result verification failed"
                }
                if (creationRenameRecoveryMustCommitHistory(receipt, entry)) {
                    entry = creationEntryWithCompletedRename(entry, receipt)
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
        val primaryRemoved = finishOldArtifact(
            receipt.oldPath,
            receipt.oldIdentity,
            receipt.expectedSize,
            receipt.expectedSha256,
        )
        val companionRemoved = receipt.companion?.let {
            finishOldArtifact(it.oldPath, it.oldIdentity, it.expectedSize, it.expectedSha256)
        } ?: true
        if (!primaryRemoved || !companionRemoved) return
        records.removeAll { it.transactionId == receipt.transactionId }
        write(records)
    }

    private fun finishOldArtifact(
        oldPath: String,
        oldIdentity: String,
        expectedSize: Long,
        expectedSha256: String,
    ): Boolean {
        val oldExists = files.exists(oldPath)
        val stillExact = oldExists &&
            files.artifactIdentity(oldPath) == oldIdentity &&
            files.size(oldPath) == expectedSize &&
            runCatching {
                files.sha256(oldPath).equals(
                    expectedSha256,
                    ignoreCase = true,
                )
            }.getOrDefault(false)
        return !oldExists || !stillExact || files.delete(oldPath)
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
): Boolean = receipt.newPath != null && (
    entry.outputPath != receipt.newPath || entry.outputName != receipt.targetName ||
        receipt.companion?.let {
            entry.companionOutputPath() != it.newPath ||
                (entry.metadata["download"] as? JsonObject)?.get("name") != JsonPrimitive(it.targetName)
        } == true
    )

internal fun creationRenameArtifactIsVerified(
    receipt: CreationHistoryRenameReceipt,
    actualIdentity: String?,
    actualSize: Long,
    actualSha256: String?,
): Boolean = receipt.newIdentity != null &&
    actualIdentity == receipt.newIdentity &&
    actualSize == receipt.expectedSize &&
    actualSha256?.equals(receipt.expectedSha256, ignoreCase = true) == true

internal fun creationEntryWithCompletedRename(
    entry: CreationHistoryEntry,
    receipt: CreationHistoryRenameReceipt,
): CreationHistoryEntry {
    val renamedCompanion = receipt.companion
    val renamedMetadata = renamedCompanion?.let { companion ->
        val download = ((entry.metadata["download"] as? JsonObject)?.toMutableMap() ?: mutableMapOf())
            .apply {
                put("path", JsonPrimitive(requireNotNull(companion.newPath)))
                put("name", JsonPrimitive(companion.targetName))
            }
        JsonObject(entry.metadata.toMutableMap().apply { put("download", JsonObject(download)) })
    } ?: entry.metadata
    return entry.copy(
        outputPath = requireNotNull(receipt.newPath),
        outputName = receipt.targetName,
        committedIdentity = receipt.newIdentity,
        metadata = renamedMetadata,
        companionCommittedIdentity = renamedCompanion?.newIdentity
            ?: entry.companionCommittedIdentity,
    )
}

private fun creationRenameCompanionIsVerified(
    receipt: CreationHistoryCompanionRenameReceipt?,
    files: CreationFileStore,
): Boolean = receipt == null || receipt.newPath?.let { path ->
    receipt.newIdentity != null &&
        files.artifactIdentity(path) == receipt.newIdentity &&
        files.size(path) == receipt.expectedSize &&
        runCatching { files.sha256(path) }
            .getOrNull()?.equals(receipt.expectedSha256, ignoreCase = true) == true
} == true
