package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import java.io.File
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.attribute.BasicFileAttributes
import java.util.UUID
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

@Serializable
internal data class CreationSvgEditReceipt(
    val entry: CreationHistoryEntry,
    val originalCopyId: String,
    val stagedPath: String,
    val stagedIdentity: String,
    val size: Long,
    val sha256: String,
    val intent: CreationPublishIntent,
    val savedAtMs: Long = entry.createdAtMs,
    val pendingHandle: String? = null,
    val pendingIdentity: String? = null,
    val prepared: Boolean = false,
    val publishedPath: String? = null,
)

/** Publishes a new result before changing history, leaving the old result intact. */
internal class CreationSvgEditStore(
    private val context: Context,
    private val files: CreationFileStore,
) {
    private val index = File(context.filesDir, "creation/state/svg-edits.json")
    private val staging = File(context.filesDir, "creation/svg-edit-staging")
    private val json = Json { ignoreUnknownKeys = false; encodeDefaults = true }

    fun save(
        entry: CreationHistoryEntry,
        svg: String,
        savedAtMs: Long,
        commit: (CreationHistoryEntry, CreationHistoryEntry) -> Unit,
    ): CreationHistoryEntry {
        CreationArtifactValidator.validateSvgText(svg)
        require(creationSvgOriginalMatches(entry, files)) { "Saved result changed before editing" }
        val originalCopyId = UUID.randomUUID().toString()
        files.managedPathIdentity(entry.outputPath)?.let { managedPath ->
            retainCreationSvgOriginal(
                creationSvgOriginalsIndex(context.filesDir),
                CreationRetainedSvgOriginal(
                    managedPath, requireNotNull(entry.committedIdentity), requireNotNull(entry.committedSize),
                    requireNotNull(entry.committedSha256),
                    entry.copy(outputPath = managedPath),
                ),
            )
        }
        val records = read().toMutableList()
        require(records.none { it.entry.id == entry.id }) { "An edit is already being recovered" }
        require(records.size < 64) { "Too many edits are awaiting recovery" }
        val destination = destinationFor(entry.outputPath)
        val operationId = UUID.randomUUID().toString()
        val intent = files.planPublishIntent(
            operationId,
            editedName(entry.outputName),
            "image/svg+xml",
            destination,
            records.map(CreationSvgEditReceipt::intent),
        )
        staging.mkdirs()
        require(Files.isDirectory(staging.toPath(), LinkOption.NOFOLLOW_LINKS) && !isCreationLink(staging.toPath())) {
            "Edit staging is unavailable"
        }
        val staged = File.createTempFile("edit-", ".svg", staging)
        val independentIdentity = creationStageFileKey(staged)
        var stagingIdentity: String? = null
        var mayBeRecorded = false
        try {
            stagingIdentity = requireNotNull(files.artifactIdentity(staged.absolutePath))
            FileOutputStream(staged).use { stream ->
                stream.write(svg.toByteArray(Charsets.UTF_8))
                stream.fd.sync()
            }
            val stagedPath = staged.absolutePath
            val receipt = CreationSvgEditReceipt(
                entry = entry,
                originalCopyId = originalCopyId,
                stagedPath = stagedPath,
                stagedIdentity = stagingIdentity,
                size = staged.length(),
                sha256 = files.sha256(stagedPath),
                intent = intent,
                savedAtMs = savedAtMs,
            )
            records += receipt
            mayBeRecorded = true
            write(records)
            return finish(records, receipt, entry, commit)
        } finally {
            if (canRemoveUnrecordedSvgStage(
                    mayBeRecorded, staged.absolutePath, stagingIdentity ?: independentIdentity,
                    if (stagingIdentity == null) creationStageFileKey(staged)
                    else files.artifactIdentity(staged.absolutePath), ::read,
                )
            ) {
                staged.delete()
            }
        }
    }

    fun recover(
        entries: List<CreationHistoryEntry>,
        commit: (List<CreationHistoryEntry>) -> Unit,
    ): List<CreationHistoryEntry> {
        val records = runCatching { read().toMutableList() }.getOrElse { return entries }
        val retained = runCatching {
            readRetainedSvgOriginals(creationSvgOriginalsIndex(context.filesDir))
        }.getOrDefault(emptyList())
        val recoveryEntries = entries.toMutableList()
        records.forEach { receipt ->
            val old = receipt.entry
            val existing = recoveryEntries.indexOfFirst { it.id == old.id }
            if ((existing < 0 || isRetainedSvgOriginal(recoveryEntries[existing])) &&
                retained.any { it.outputPath == runCatching { File(old.outputPath).canonicalPath }.getOrNull() &&
                    it.identity == old.committedIdentity &&
                    it.size == old.committedSize && it.sha256 == old.committedSha256 } &&
                creationSvgOriginalMatches(old, files) &&
                (existing < 0 || (recoveryEntries[existing].committedIdentity == old.committedIdentity &&
                    recoveryEntries[existing].committedSha256 == old.committedSha256))
            ) {
                if (existing < 0) recoveryEntries += old else recoveryEntries[existing] = old
            }
        }
        return recoverCreationSvgEditEntries(recoveryEntries, records.toList(), commit) { receipt, entry, apply ->
            finish(records, receipt, entry, apply)
        }
    }

    fun pendingEntryFor(path: String): String? =
        read().singleOrNull { it.entry.outputPath == path }?.entry?.id

    fun pendingIds(): Set<String>? = runCatching { read().mapTo(mutableSetOf()) { it.entry.id } }.getOrNull()

    private fun finish(
        records: MutableList<CreationSvgEditReceipt>,
        initial: CreationSvgEditReceipt,
        entry: CreationHistoryEntry,
        commit: (CreationHistoryEntry, CreationHistoryEntry) -> Unit,
    ): CreationHistoryEntry = finishCreationSvgEdit(
        initial,
        entry,
        object : CreationSvgEditOperations {
            override fun originalMatches(entry: CreationHistoryEntry) = creationSvgOriginalMatches(entry, files)

            override fun stagedMatches(receipt: CreationSvgEditReceipt): Boolean {
                val file = File(receipt.stagedPath)
                return file.parentFile?.canonicalFile == staging.canonicalFile &&
                    files.artifactIdentity(receipt.stagedPath) == receipt.stagedIdentity &&
                    creationFileMatchesProof(file, receipt.size, receipt.sha256)
            }

            override fun reserve(receipt: CreationSvgEditReceipt) = files.reservePublishIntent(receipt.intent)

            override fun populate(receipt: CreationSvgEditReceipt) {
                files.populatePublishIntent(
                    receipt.intent, requireNotNull(receipt.pendingHandle),
                    requireNotNull(receipt.pendingIdentity), File(receipt.stagedPath),
                    receipt.size, receipt.sha256,
                )
            }

            override fun publish(receipt: CreationSvgEditReceipt) = files.commitPublishIntent(
                receipt.intent, requireNotNull(receipt.pendingHandle),
                requireNotNull(receipt.pendingIdentity), receipt.size, receipt.sha256,
            )

            override fun publishedMatches(receipt: CreationSvgEditReceipt) = files.publishedArtifactMatches(
                requireNotNull(receipt.publishedPath), requireNotNull(receipt.pendingIdentity),
                receipt.size, receipt.sha256,
            )

            override fun persist(receipt: CreationSvgEditReceipt) = replace(records, receipt)

            override fun complete(receipt: CreationSvgEditReceipt) = complete(records, receipt)
        },
        { updated, originalCopy ->
            transferRetainedSvgOriginal(
                creationSvgOriginalsIndex(context.filesDir), initial.entry, originalCopy,
            )
            commit(updated, originalCopy)
        },
    )

    private fun complete(records: MutableList<CreationSvgEditReceipt>, receipt: CreationSvgEditReceipt) {
        val file = File(receipt.stagedPath)
        if (file.exists()) {
            require(file.parentFile?.canonicalFile == staging.canonicalFile &&
                files.artifactIdentity(receipt.stagedPath) == receipt.stagedIdentity &&
                creationFileMatchesProof(file, receipt.size, receipt.sha256))
            check(file.delete()) { "Could not finish saved edit" }
        }
        records.removeAll { it.entry.id == receipt.entry.id }
        write(records)
    }

    private fun destinationFor(path: String): String? {
        if (!path.startsWith("content://")) {
            require(files.managedPathIdentity(path) != null) { "Saved result is not managed" }
            return null
        }
        val uri = Uri.parse(path)
        if (CreationDownloadsStore(context).owns(uri)) return CREATION_DOWNLOADS_DESTINATION
        val tree = DocumentsContract.buildTreeDocumentUri(
            requireNotNull(uri.authority),
            DocumentsContract.getTreeDocumentId(uri),
        )
        require(context.contentResolver.persistedUriPermissions.any {
            it.uri == tree && it.isReadPermission && it.isWritePermission
        }) { "Select the saved folder again before editing" }
        CreationSafTree(context.contentResolver).validateAtomicDestination(tree)
        return tree.toString()
    }

    private fun read(): List<CreationSvgEditReceipt> {
        return readCreationSvgEditReceipts(index)
    }

    private fun write(records: List<CreationSvgEditReceipt>) {
        writeCreationIndexTextAtomically(index, json.encodeToString(records), 4L * 1024 * 1024, requireAtomic = true)
    }

    private fun replace(records: MutableList<CreationSvgEditReceipt>, receipt: CreationSvgEditReceipt) {
        val index = records.indexOfFirst { it.entry.id == receipt.entry.id }
        require(index >= 0)
        records[index] = receipt
        write(records)
    }
}

internal fun readCreationSvgEditReceipts(index: File): List<CreationSvgEditReceipt> {
    if (!Files.exists(index.toPath(), LinkOption.NOFOLLOW_LINKS)) return emptyList()
    val text = requireNotNull(readCreationIndexTextBounded(index, 4L * 1024 * 1024)) {
        "Saved edits are unavailable"
    }
    val records = Json.decodeFromString(
        ListSerializer(CreationSvgEditReceipt.serializer()), text,
    )
    require(records.size <= 64) { "Too many saved edits" }
    return records
}

internal fun requireCreationSvgEditOwner(
    entries: List<CreationHistoryEntry>,
    receipt: CreationSvgEditReceipt,
): CreationHistoryEntry = requireNotNull(entries.singleOrNull { it.id == receipt.entry.id }) {
    "A saved vector edit is awaiting its original history entry"
}

internal fun canRemoveUnrecordedSvgStage(
    mayBeRecorded: Boolean,
    path: String,
    expectedIdentity: String?,
    observedIdentity: String?,
    readReceipts: () -> List<CreationSvgEditReceipt>,
): Boolean = expectedIdentity != null && expectedIdentity == observedIdentity &&
    (!mayBeRecorded || runCatching { readReceipts().none { it.stagedPath == path } }.getOrDefault(false))

internal fun creationStageFileKey(file: File): String? = runCatching {
    Files.readAttributes(file.toPath(), BasicFileAttributes::class.java, LinkOption.NOFOLLOW_LINKS)
        .fileKey()?.toString()
}.getOrNull()

internal fun creationSvgOriginalMatches(entry: CreationHistoryEntry, files: CreationFileStore): Boolean {
    val identity = entry.committedIdentity ?: return false
    val size = entry.committedSize ?: return false
    val digest = entry.committedSha256 ?: return false
    return files.publishedArtifactMatches(entry.outputPath, identity, size, digest)
}

internal fun editedName(name: String): String =
    "${name.substringBeforeLast('.', name)}-edited.svg"
