package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import dev.screengoated.toolbox.mobile.history.HistoryPersistence
import dev.screengoated.toolbox.mobile.history.MAX_HISTORY_LIMIT
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
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
    private val sharedHistorySettings = HistoryPersistence(context, Json { ignoreUnknownKeys = true }, readOnlySnapshots = true)
    private val cleanup = files.pendingCleanupStore()
    private val renames = CreationHistoryRenameStore(context, files)
    private val svgEdits = CreationSvgEditStore(context, files)
    private val svgOriginals = creationSvgOriginalsIndex(context.filesDir)

    fun hasUnresolvedSvgEdits(): Boolean = synchronized(lock) {
        svgEdits.pendingIds()?.isNotEmpty() != false ||
            runCatching { readRetainedSvgOriginals(svgOriginals) }.isFailure
    }

    fun ensureSvgEditSettled(id: String) = synchronized(lock) {
        val pending = svgEdits.pendingIds()
        require(pending != null && id !in pending) { "A saved vector edit is awaiting recovery" }
    }

    fun saveSvgEdit(path: String, svg: String): CreationHistoryEntry = synchronized(lock) {
        val pendingOwner = svgEdits.pendingEntryFor(path)
        val entries = read().toMutableList()
        val entry = resolveCreationSvgEditEntry(entries, path, pendingOwner)
        if (entry.outputPath != path) {
            val bytes = svg.toByteArray(Charsets.UTF_8)
            if (entry.committedSize == bytes.size.toLong() &&
                entry.committedSha256 == creationStreamSha256(bytes.inputStream()) &&
                creationSvgOriginalMatches(entry, files)
            ) return@synchronized entry
        }
        val savedAtMs = creationSvgEditTimestamp(entries)
        svgEdits.save(entry, svg, savedAtMs) { updated, originalCopy ->
            val index = entries.indexOfFirst { it.id == updated.id }
            require(index >= 0 && entries[index] == entry) { "Saved result changed while editing" }
            val revised = creationHistoryWithSvgCopy(entries, updated, originalCopy)
            entries.clear()
            entries.addAll(revised)
            write(entries, requireAtomic = true)
        }
    }

    fun list(tool: CreationTool): List<CreationHistoryEntry> = synchronized(lock) {
        recoverCleanupOwnership()
        val all = read()
        val pendingIds = svgEdits.pendingIds()
        val existing = all.filter {
            pendingIds == null || it.id in pendingIds || isRetainedSvgOriginal(it) ||
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
            .filter { it.tool == tool.wireName && (pendingIds == null || it.id in pendingIds ||
                isRetainedSvgOriginal(it) || files.exists(it.outputPath)) }
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
        val ownedOutput = creationCommittedProofPath(
            outputPath,
            files.managedPathIdentity(outputPath),
        )
        val companionPath = (metadata["download"] as? JsonObject)
            ?.get("path")?.let { it as? JsonPrimitive }?.content
        val ownedCompanion = companionPath?.let { path ->
            creationCommittedProofPath(path, files.managedPathIdentity(path))
        }
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
            companionCommittedSize = ownedCompanion?.let(files::size)?.takeIf { it >= 0L },
            companionCommittedSha256 = ownedCompanion?.let {
                runCatching { files.sha256(it) }.getOrNull()
            },
            companionCommittedIdentity = companionPath?.let(files::artifactIdentity),
        )
        val currentEntries = read()
        val pendingIds = requireNotNull(svgEdits.pendingIds()) { "Saved vector edits are unavailable" }
        val retained = currentEntries.filter {
            it.id in pendingIds || isRetainedSvgOriginal(it) || isUserOwnedCreationOutputPath(it.outputPath) || files.exists(it.outputPath)
        }.toMutableList()
        require(retained.none { it.id in pendingIds && (it.dispatchId == dispatchId || it.outputPath == outputPath) }) {
            "A saved vector edit is awaiting recovery"
        }
        retained.removeAll { it.dispatchId == dispatchId || it.outputPath == outputPath }
        retained += entry
        val livePaths = files.journalProtectedManagedPaths(protectedPaths)
        val retentionItems = retained.filterNot(::isRetainedSvgOriginal).map { candidate ->
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
        val kept = retained.filter { it.id in keptIds || it.id in pendingIds || isRetainedSvgOriginal(it) }
        val removed = retained.filter { it.id !in keptIds && it.id !in pendingIds && !isRetainedSvgOriginal(it) }
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
        ensureSvgEditSettled(id)
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
            if (isRetainedSvgOriginal(current)) {
                reconcileRetainedSvgRenames(svgOriginals, entries) { creationSvgOriginalMatches(it, files) }
            }
            write(entries)
        }
    }

    fun delete(id: String): CreationHistoryEntry = synchronized(lock) {
        val entries = read().toMutableList()
        val index = entries.indexOfFirst { it.id == id }
        require(index >= 0) { "Saved result is unavailable" }
        ensureSvgEditSettled(id)
        val removed = entries.removeAt(index)
        val managed = files.managedPathIdentity(removed.outputPath)
        val retainedOriginal = if (managed == null) null else {
            val records = try { readRetainedSvgOriginals(svgOriginals) } catch (error: Exception) {
                throw CreationSvgOriginalsUnavailableException(error)
            }
            val matches = records.filter { it.outputPath == managed }
            val verified = matches.singleOrNull { it.identity == removed.committedIdentity &&
                it.size == removed.committedSize && it.sha256 == removed.committedSha256 }
            require(matches.isEmpty() || verified != null) { "Saved original changed before deletion" }
            verified
        }
        val deleted = listOfNotNull(removed.outputPath, removed.companionOutputPath()).all { path ->
            if (path == removed.outputPath && retainedOriginal != null) {
                deleteRetainedSvgOriginal(retainedOriginal)
            } else if (isUserOwnedCreationOutputPath(path)) files.delete(path)
            else !files.exists(path) || files.delete(path)
        }
        check(deleted) {
            "Could not delete result"
        }
        write(entries)
        if (retainedOriginal != null) {
            forgetRetainedSvgOriginal(svgOriginals, retainedOriginal)
        }
        removed
    }

    private fun deleteRetainedSvgOriginal(record: CreationRetainedSvgOriginal): Boolean {
        val path = java.nio.file.Paths.get(record.outputPath)
        if (java.nio.file.Files.notExists(path, java.nio.file.LinkOption.NOFOLLOW_LINKS)) {
            cleanup.drain()
            return cleanup.hasPendingPath(record.outputPath) == false &&
                java.nio.file.Files.notExists(path, java.nio.file.LinkOption.NOFOLLOW_LINKS)
        }
        val captured = cleanup.isolateAndEnqueue(listOf(CreationCleanupCandidate(
            path = record.outputPath,
            expectedIdentity = record.identity,
            expectedSize = record.size,
            expectedSha256 = record.sha256,
        )))
        cleanup.drain()
        return record.outputPath in captured &&
            cleanup.hasPendingPath(record.outputPath) == false &&
            java.nio.file.Files.notExists(path, java.nio.file.LinkOption.NOFOLLOW_LINKS)
    }

    fun maintain(
        budgetBytes: Long = CREATION_MANAGED_STORAGE_CAP_BYTES,
        pruneEphemeral: Boolean = true,
    ) = synchronized(lock) {
        require(budgetBytes >= 0L)
        recoverCleanupOwnership()
        val currentEntries = read()
        val pendingIds = svgEdits.pendingIds() ?: return@synchronized
        val retained = currentEntries.filter {
            it.id in pendingIds || isRetainedSvgOriginal(it) || isUserOwnedCreationOutputPath(it.outputPath) || files.exists(it.outputPath)
        }
        val livePaths = files.journalProtectedManagedPaths(emptySet())
        val items = retained.filterNot(::isRetainedSvgOriginal).map { entry ->
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
        val kept = retained.filter { it.id in keptIds || it.id in pendingIds || isRetainedSvgOriginal(it) }
        val removed = retained.filter { it.id !in keptIds && it.id !in pendingIds && !isRetainedSvgOriginal(it) }
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
        if (!target.isFile) {
            val retained = mergeRetainedSvgOriginals(
                emptyList(), runCatching { readRetainedSvgOriginals(svgOriginals) }.getOrDefault(emptyList()),
            )
            val renamed = renames.recover(retained) {
                if (it.any(::isRetainedSvgOriginal)) {
                    reconcileRetainedSvgRenames(svgOriginals, it) { entry -> creationSvgOriginalMatches(entry, files) }
                }
                write(it)
            }
            return svgEdits.recover(renamed) { write(it, requireAtomic = true) }
        }
        val decoded = runCatching {
            val text = requireNotNull(
                readCreationIndexTextBounded(target, CREATION_HISTORY_INDEX_MAX_BYTES),
            )
            json.decodeFromString(ListSerializer(CreationHistoryEntry.serializer()), text)
        }.getOrElse {
            throw IllegalStateException("Creation history is unavailable", it)
        }
        val pendingIds = svgEdits.pendingIds()
        val boundedIdsBeforePending = boundedCreationHistory(decoded).mapTo(mutableSetOf(), CreationHistoryEntry::id)
        val bounded = decoded.filter { pendingIds == null || it.id in boundedIdsBeforePending || it.id in pendingIds }
        val boundedIds = bounded.mapTo(mutableSetOf(), CreationHistoryEntry::id)
        val dropped = decoded.filter { it.id !in boundedIds }
        val changed = bounded.size != decoded.size
        if (changed) {
            cleanup.isolateAndEnqueue(
                cleanupRecords(
                    dropped,
                    files.journalProtectedManagedPaths(emptySet()),
                ),
            )
            write(bounded)
        }
        val merged = mergeRetainedSvgOriginals(
            bounded, runCatching { readRetainedSvgOriginals(svgOriginals) }.getOrDefault(emptyList()),
            pendingIds,
            renames.pendingReceipts(),
        )
        val renamed = renames.recover(merged) {
            if (it.any(::isRetainedSvgOriginal)) {
                reconcileRetainedSvgRenames(svgOriginals, it) { entry -> creationSvgOriginalMatches(entry, files) }
            }
            write(it)
        }
        return svgEdits.recover(renamed) { write(it, requireAtomic = true) }
    }

    private fun cleanupRecords(
        removed: List<CreationHistoryEntry>,
        livePaths: Set<String>,
    ): List<CreationCleanupCandidate> {
        if (runCatching { readRetainedSvgOriginals(svgOriginals) }.isFailure) return emptyList()
        val outputs = removed.flatMap { entry ->
            buildList {
                if (entry.committedSize != null && entry.committedSha256 != null) {
                    files.managedPathIdentity(entry.outputPath)?.let { path ->
                        entry.committedIdentity?.let { identity ->
                            add(
                                CreationCleanupCandidate(
                                    path = path,
                                    expectedSize = entry.committedSize,
                                    expectedSha256 = entry.committedSha256,
                                    expectedIdentity = identity,
                                    retainedHistoryEntry = entry,
                                ),
                            )
                        }
                    }
                }
                val companionPath = entry.companionOutputPath()
                if (companionPath != null &&
                    entry.companionCommittedSize != null &&
                    entry.companionCommittedSha256 != null
                ) {
                    files.managedPathIdentity(companionPath)?.let { path ->
                        entry.companionCommittedIdentity?.let { identity ->
                            add(
                                CreationCleanupCandidate(
                                    path = path,
                                    expectedSize = entry.companionCommittedSize,
                                    expectedSha256 = entry.companionCommittedSha256,
                                    expectedIdentity = identity,
                                    retainedHistoryEntry = entry,
                                ),
                            )
                        }
                    }
                }
            }
        }
        return outputs
            .filter { it.path !in livePaths }
            .distinctBy(CreationCleanupCandidate::path)
    }

    private fun write(entries: List<CreationHistoryEntry>, requireAtomic: Boolean = false) {
        val retainedIds = runCatching { readRetainedSvgOriginals(svgOriginals) }
            .getOrDefault(emptyList()).mapTo(mutableSetOf()) { retainedSvgOriginalEntry(it).id }
        val ordinary = entries.filterNot { it.id in retainedIds }
        writeCreationIndexTextAtomically(
            target,
            json.encodeToString(ListSerializer(CreationHistoryEntry.serializer()), ordinary),
            CREATION_HISTORY_INDEX_MAX_BYTES,
            requireAtomic = requireAtomic,
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

}

internal fun boundedCreationHistory(decoded: List<CreationHistoryEntry>): List<CreationHistoryEntry> =
    decoded.filter { entry -> CreationTool.entries.any { it.wireName == entry.tool } }
        .groupBy(CreationHistoryEntry::tool).values.flatMap { entries ->
            entries.sortedByDescending(CreationHistoryEntry::createdAtMs).take(MAX_HISTORY_LIMIT)
        }

internal fun creationSvgEditTimestamp(entries: List<CreationHistoryEntry>): Long {
    val latest = entries.maxOfOrNull(CreationHistoryEntry::createdAtMs) ?: 0L
    require(latest < Long.MAX_VALUE) { "Creation history timestamps are unavailable" }
    return maxOf(System.currentTimeMillis(), latest + 1)
}

internal fun resolveCreationSvgEditEntry(
    entries: List<CreationHistoryEntry>,
    requestedPath: String,
    pendingOwner: String?,
): CreationHistoryEntry {
    if (pendingOwner != null) {
        entries.singleOrNull { it.id == pendingOwner && it.tool == CreationTool.IMAGE_TO_SVG.wireName }
            ?.let { return it }
    }
    val matching = entries.filter {
        it.tool == CreationTool.IMAGE_TO_SVG.wireName &&
            it.outputPath == requestedPath
    }
    require(matching.size == 1) { "Saved vector result is unavailable or ambiguous" }
    return matching.single()
}

internal fun creationHistoryWithSvgCopy(
    entries: List<CreationHistoryEntry>,
    updated: CreationHistoryEntry,
    originalCopy: CreationHistoryEntry,
): List<CreationHistoryEntry> {
    require(updated.id != originalCopy.id)
    val existing = entries.firstOrNull { it.id == originalCopy.id }
    require(existing == null || existing == originalCopy || retainedSvgOriginalMatchesCopy(existing, originalCopy)) {
        "Saved result identity changed"
    }
    return entries.map { if (it.id == updated.id) updated else it } +
        if (existing == null) listOf(originalCopy) else emptyList()
}

/**
 * Re-applies history limits after the user changes them. It runs through the one
 * app-scoped manager, so pruning sees the same in-memory source leases and recent
 * managed paths as live drafts and jobs, and it runs off the caller's thread.
 */
internal object CreationHistoryMaintenance {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    fun run(context: Context) {
        val manager = CreationJobManager.get(context.applicationContext)
        scope.launch {
            maintainCreationHistoryAfterStartup(
                awaitStartup = { manager.awaitStartup() },
                maintain = { manager.history.maintain() },
            )
        }
    }
}

internal suspend fun maintainCreationHistoryAfterStartup(
    awaitStartup: suspend () -> Unit,
    maintain: () -> Unit,
) {
    runCatching {
        awaitStartup()
        maintain()
    }
}

private val creationHistoryLock = Any()
