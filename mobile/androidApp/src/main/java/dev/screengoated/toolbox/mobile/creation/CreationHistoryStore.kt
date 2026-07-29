package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import dev.screengoated.toolbox.mobile.history.HistoryPersistence
import dev.screengoated.toolbox.mobile.history.MAX_HISTORY_LIMIT
import java.io.File
import java.util.UUID
import java.util.PriorityQueue
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

internal class CreationHistoryStore(
    context: Context,
    private val files: CreationFileStore,
) {
    private val target = File(context.filesDir, "creation/history.json")
    private val lock = creationHistoryLock
    private val json = Json { ignoreUnknownKeys = true; prettyPrint = true }
    private val sharedHistorySettings = HistoryPersistence(context, Json { ignoreUnknownKeys = true })
    private val cleanup = files.pendingCleanupStore()
    private val renames = CreationHistoryRenameStore(context, files)

    fun list(tool: CreationTool): List<CreationHistoryEntry> = synchronized(lock) {
        recoverCleanupOwnership()
        val all = read()
        val existing = all.filter {
            isUserOwnedCreationOutputPath(it.outputPath) || files.exists(it.outputPath)
        }
        if (existing.size != all.size) {
            val removed = all.filter { it.id !in existing.map(CreationHistoryEntry::id).toSet() }
            val pending = cleanupRecords(removed, emptySet())
            cleanup.isolateAndEnqueue(pending)
            write(existing)
            recoverCleanupOwnership()
        }
        existing.asSequence()
            .filter { it.tool == tool.wireName && files.exists(it.outputPath) }
            .sortedByDescending(CreationHistoryEntry::createdAtMs)
            .toList()
    }

    fun record(
        dispatchId: String,
        tool: CreationTool,
        sourcePath: String,
        outputPath: String,
        outputName: String,
        metadata: JsonObject,
        protectedPaths: Set<String> = emptySet(),
    ): CreationHistoryEntry = synchronized(lock) {
        recoverCleanupOwnership()
        val ownedOutput = files.managedPathIdentity(outputPath)
        val entry = CreationHistoryEntry(
            id = UUID.randomUUID().toString(),
            dispatchId = dispatchId,
            tool = tool.wireName,
            sourcePath = sourcePath,
            outputPath = outputPath,
            outputName = outputName,
            createdAtMs = System.currentTimeMillis(),
            metadata = metadata,
            committedSize = ownedOutput?.let(files::size)?.takeIf { it >= 0L },
            committedSha256 = ownedOutput?.let {
                runCatching { files.sha256(it) }.getOrNull()
            },
            committedIdentity = files.artifactIdentity(outputPath),
        )
        val retained = read().filter {
            isUserOwnedCreationOutputPath(it.outputPath) || files.exists(it.outputPath)
        }.toMutableList()
        retained.removeAll { it.dispatchId == dispatchId || it.outputPath == outputPath }
        retained += entry
        val livePaths = files.journalProtectedManagedPaths(protectedPaths)
        val retentionItems = retained.map { candidate ->
            CreationHistoryRetentionItem(
                id = candidate.id,
                tool = candidate.tool,
                createdAtMs = candidate.createdAtMs,
                managedPaths = candidate.allPaths().mapNotNull(files::managedPathIdentity).toSet(),
            )
        }
        val keptIds = planCreationHistoryRetention(
            entries = retentionItems,
            maximumPerTool = sharedHistorySettings.loadSettings().maxItems,
            budgetBytes = CREATION_MANAGED_STORAGE_CAP_BYTES,
            protectedManagedPaths = livePaths,
            sizeOf = files::size,
        )
        val kept = retained.filter { it.id in keptIds }
        val removed = retained.filter { it.id !in keptIds }
        val pending = cleanupRecords(removed, livePaths)
        cleanup.isolateAndEnqueue(pending)
        write(kept.sortedByDescending(CreationHistoryEntry::createdAtMs))
        recoverCleanupOwnership()
        val protected = protectedPaths + kept.flatMap(CreationHistoryEntry::allPaths)
        files.pruneManagedArtifacts(protected.toSet(), CREATION_MANAGED_STORAGE_CAP_BYTES)
        entry
    }

    fun rename(id: String, requestedName: String): CreationHistoryEntry = synchronized(lock) {
        val entries = read().toMutableList()
        val index = entries.indexOfFirst { it.id == id }
        require(index >= 0) { "Saved result is unavailable" }
        val current = entries[index]
        val extension = current.outputName.substringAfterLast('.', "")
        val requestedExtension = requestedName.substringAfterLast('.', "")
        val finalName = if (extension.isNotBlank() && !requestedExtension.equals(extension, true)) {
            "${requestedName.substringBeforeLast('.', requestedName)}.$extension"
        } else {
            requestedName
        }
        if (finalName == current.outputName) return current
        return renames.rename(current, finalName) { updated ->
            entries[index] = updated
            write(entries)
        }
    }

    fun delete(id: String): CreationHistoryEntry = synchronized(lock) {
        val entries = read().toMutableList()
        val index = entries.indexOfFirst { it.id == id }
        require(index >= 0) { "Saved result is unavailable" }
        val removed = entries.removeAt(index)
        val deleted = if (isUserOwnedCreationOutputPath(removed.outputPath)) {
            files.delete(removed.outputPath)
        } else {
            !files.exists(removed.outputPath) || files.delete(removed.outputPath)
        }
        check(deleted) {
            "Could not delete result"
        }
        write(entries)
        removed
    }

    fun maintain(
        budgetBytes: Long = CREATION_MANAGED_STORAGE_CAP_BYTES,
        pruneEphemeral: Boolean = true,
    ) = synchronized(lock) {
        require(budgetBytes >= 0L)
        recoverCleanupOwnership()
        val retained = read().filter {
            isUserOwnedCreationOutputPath(it.outputPath) || files.exists(it.outputPath)
        }
        val livePaths = files.journalProtectedManagedPaths(emptySet())
        val items = retained.map { entry ->
            CreationHistoryRetentionItem(
                entry.id,
                entry.tool,
                entry.createdAtMs,
                entry.allPaths().mapNotNull(files::managedPathIdentity).toSet(),
            )
        }
        val historyPaths = items.flatMap(CreationHistoryRetentionItem::managedPaths).toSet()
        val historyBytes = historyPaths.fold(0L) { total, path ->
            creationSaturatingBytes(total, files.size(path).coerceAtLeast(0L))
        }
        val totalManagedBytes = files.managedStorageBytes()
        val historyBudget = creationHistoryAdmissionBudget(
            totalManagedBytes,
            historyBytes.coerceAtMost(totalManagedBytes),
            budgetBytes,
        )
        val keptIds = planCreationHistoryRetention(
            items,
            sharedHistorySettings.loadSettings().maxItems,
            historyBudget,
            livePaths.intersect(historyPaths),
            files::size,
        )
        val kept = retained.filter { it.id in keptIds }
        val removed = retained.filter { it.id !in keptIds }
        cleanup.isolateAndEnqueue(cleanupRecords(removed, livePaths))
        write(kept.sortedByDescending(CreationHistoryEntry::createdAtMs))
        recoverCleanupOwnership()
        if (pruneEphemeral) {
            files.pruneManagedArtifacts(
                kept.flatMap(CreationHistoryEntry::allPaths).toSet() + livePaths,
                budgetBytes,
            )
        }
    }

    private fun read(): List<CreationHistoryEntry> {
        if (!target.isFile) return emptyList()
        val decoded = runCatching {
            val text = requireNotNull(
                readCreationIndexTextBounded(target, CREATION_HISTORY_INDEX_MAX_BYTES),
            )
            json.decodeFromString(ListSerializer(CreationHistoryEntry.serializer()), text)
        }.getOrElse {
            throw IllegalStateException("Creation history is unavailable", it)
        }
        val bounded = boundedCreationHistory(decoded)
        val boundedIds = bounded.mapTo(mutableSetOf(), CreationHistoryEntry::id)
        val dropped = decoded.filter { it.id !in boundedIds }
        var changed = bounded.size != decoded.size
        val migrated = bounded.map { entry ->
            val operation = (entry.metadata["operation"] as? JsonPrimitive)?.content
            val normalizedOperation = CreationTool.fromWireName(entry.tool)?.let { tool ->
                CreationContract.normalizedOperation(tool, operation)
            }
            val normalized = if (normalizedOperation != null && normalizedOperation != operation) {
                changed = true
                entry.copy(
                    metadata = JsonObject(
                        entry.metadata.toMutableMap().apply {
                            put("operation", JsonPrimitive(normalizedOperation))
                        },
                    ),
                )
            } else {
                entry
            }
            normalized
        }
        if (changed) {
            cleanup.isolateAndEnqueue(
                cleanupRecords(
                    dropped,
                    files.journalProtectedManagedPaths(emptySet()),
                ),
            )
            write(migrated)
        }
        return renames.recover(migrated, ::write)
    }

    private fun cleanupRecords(
        removed: List<CreationHistoryEntry>,
        livePaths: Set<String>,
    ): List<CreationCleanupCandidate> {
        val outputs = removed.mapNotNull { entry ->
            if (entry.committedSize == null || entry.committedSha256 == null) {
                return@mapNotNull null
            }
            val path = files.managedPathIdentity(entry.outputPath) ?: return@mapNotNull null
            CreationCleanupCandidate(
                path = path,
                expectedSize = entry.committedSize,
                expectedSha256 = entry.committedSha256,
                expectedIdentity = entry.committedIdentity ?: return@mapNotNull null,
                retainedHistoryEntry = entry,
            )
        }
        return outputs
            .filter { it.path !in livePaths }
            .distinctBy(CreationCleanupCandidate::path)
    }

    private fun write(entries: List<CreationHistoryEntry>) {
        writeCreationIndexTextAtomically(
            target,
            json.encodeToString(ListSerializer(CreationHistoryEntry.serializer()), entries),
            CREATION_HISTORY_INDEX_MAX_BYTES,
        )
        files.reconcilePersistedUriGrants()
    }

    private fun recoverCleanupOwnership() {
        val recovered = cleanup.drain()
        if (recovered.isEmpty()) return
        val current = read().associateBy(CreationHistoryEntry::id).toMutableMap()
        recovered.forEach { current[it.id] = it }
        write(current.values.sortedByDescending(CreationHistoryEntry::createdAtMs))
        cleanup.acknowledgeReattached(recovered.map(CreationHistoryEntry::id).toSet())
    }

    private fun boundedCreationHistory(
        decoded: List<CreationHistoryEntry>,
    ): List<CreationHistoryEntry> {
        val knownTools = CreationTool.entries.map(CreationTool::wireName).toSet()
        val byTool = knownTools.associateWith {
            PriorityQueue<CreationHistoryEntry>(
                compareBy(CreationHistoryEntry::createdAtMs),
            )
        }
        decoded.forEach { entry ->
            val queue = byTool[entry.tool] ?: return@forEach
            queue += entry
            if (queue.size > MAX_HISTORY_LIMIT) queue.poll()
        }
        return byTool.values.flatMap { it.toList() }
    }

}

internal object CreationHistoryMaintenance {
    fun run(context: Context) {
        val files = CreationFileStore(context.applicationContext)
        CreationHistoryStore(context.applicationContext, files).maintain()
    }
}

private val creationHistoryLock = Any()
