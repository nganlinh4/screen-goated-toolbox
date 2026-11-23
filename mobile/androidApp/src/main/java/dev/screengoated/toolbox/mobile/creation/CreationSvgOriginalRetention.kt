package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

private const val MAX_RETAINED_SVG_ORIGINALS = 4096
internal const val CREATION_SVG_ORIGINALS_MAX_BYTES = 4L * 1024 * 1024
internal const val CREATION_SVG_ORIGINALS_UNAVAILABLE = "creation_svg_originals_unavailable"

internal class CreationSvgOriginalsUnavailableException(cause: Throwable? = null) :
    IllegalStateException(CREATION_SVG_ORIGINALS_UNAVAILABLE, cause)

@Serializable
internal data class CreationRetainedSvgOriginal(
    val outputPath: String,
    val identity: String,
    val size: Long,
    val sha256: String,
    val entry: CreationHistoryEntry? = null,
)

internal fun creationSvgOriginalsIndex(filesDir: File): File =
    File(filesDir, "creation/state/svg-originals.json")

internal fun readRetainedSvgOriginals(index: File): List<CreationRetainedSvgOriginal> {
    if (Files.notExists(index.toPath(), LinkOption.NOFOLLOW_LINKS)) return emptyList()
    val text = requireNotNull(readCreationIndexTextBounded(index, CREATION_SVG_ORIGINALS_MAX_BYTES)) {
        "Retained vector originals are unavailable"
    }
    val records = Json.decodeFromString(ListSerializer(CreationRetainedSvgOriginal.serializer()), text)
    require(records.size <= MAX_RETAINED_SVG_ORIGINALS &&
        records.map(CreationRetainedSvgOriginal::outputPath).toSet().size == records.size &&
        records.map { retainedSvgOriginalEntry(it).id }.toSet().size == records.size &&
        records.all {
        it.outputPath.isNotBlank() && it.identity.isNotBlank() && it.size >= 0 && it.sha256.isNotBlank()
            && (it.entry == null || (it.entry.outputPath == it.outputPath &&
                it.entry.committedIdentity == it.identity && it.entry.committedSize == it.size &&
                it.entry.committedSha256 == it.sha256))
    }) { "Retained vector originals are unavailable" }
    return records
}

/** Reserve protection before publishing an edit. Reservations persist even if its history ages out. */
internal fun retainCreationSvgOriginal(index: File, record: CreationRetainedSvgOriginal) {
    val records = try { readRetainedSvgOriginals(index) } catch (error: Exception) {
        throw CreationSvgOriginalsUnavailableException(error)
    }
    if (record in records) return
    val previous = records.singleOrNull { it.outputPath == record.outputPath }
    if (previous != null && (previous.identity != record.identity || previous.size != record.size ||
            previous.sha256 != record.sha256)
    ) throw CreationSvgOriginalsUnavailableException()
    if (previous == null && records.size >= MAX_RETAINED_SVG_ORIGINALS) throw CreationSvgOriginalsUnavailableException()
    try {
        writeCreationIndexTextAtomically(
            index, Json.encodeToString(records.filterNot { it.outputPath == record.outputPath } + record),
            CREATION_SVG_ORIGINALS_MAX_BYTES, requireAtomic = true,
        )
    } catch (error: Exception) { throw CreationSvgOriginalsUnavailableException(error) }
}

private const val RETAINED_MARKER = "retainedSvgOriginal"

internal fun retainedSvgOriginalEntry(record: CreationRetainedSvgOriginal): CreationHistoryEntry {
    val entry = record.entry ?: CreationHistoryEntry(
        id = "retained-" + creationStreamSha256(
            (record.outputPath + "\u0000" + record.identity).byteInputStream(),
        ),
        tool = CreationTool.IMAGE_TO_SVG.wireName,
        sourcePath = "",
        outputPath = record.outputPath,
        outputName = File(record.outputPath).name,
        createdAtMs = 0L,
        committedSize = record.size,
        committedSha256 = record.sha256,
        committedIdentity = record.identity,
    )
    return entry.copy(metadata = JsonObject(entry.metadata + (RETAINED_MARKER to JsonPrimitive(true))))
}

internal fun isRetainedSvgOriginal(entry: CreationHistoryEntry): Boolean =
    entry.metadata[RETAINED_MARKER] == JsonPrimitive(true)

internal fun withoutRetainedSvgOriginalMarker(entry: CreationHistoryEntry): CreationHistoryEntry =
    entry.copy(metadata = JsonObject(entry.metadata - RETAINED_MARKER))

internal fun retainedSvgOriginalMatchesCopy(entry: CreationHistoryEntry, copy: CreationHistoryEntry): Boolean =
    isRetainedSvgOriginal(entry) && runCatching {
        withoutRetainedSvgOriginalMarker(entry).copy(outputPath = File(entry.outputPath).canonicalPath) ==
            withoutRetainedSvgOriginalMarker(copy).copy(outputPath = File(copy.outputPath).canonicalPath)
    }.getOrDefault(false)

internal fun mergeRetainedSvgOriginals(
    entries: List<CreationHistoryEntry>,
    records: List<CreationRetainedSvgOriginal>,
    activeOwnerIds: Set<String>? = emptySet(),
    renames: List<CreationHistoryRenameReceipt> = emptyList(),
): List<CreationHistoryEntry> {
    val merged = entries.toMutableList()
    for (record in records) {
        val retained = retainedSvgOriginalEntry(record)
        val matchingId = merged.indexOfFirst { it.id == retained.id }
        if (matchingId >= 0) {
            val current = merged[matchingId]
            val sameOriginal = File(current.outputPath).canonicalPath == File(retained.outputPath).canonicalPath &&
                merged[matchingId].committedIdentity == retained.committedIdentity &&
                merged[matchingId].committedSize == retained.committedSize &&
                merged[matchingId].committedSha256 == retained.committedSha256
            val renamePending = renames.any { creationRetainedRenameIsRecoverable(current, retained, it) }
            require(sameOriginal || renamePending) {
                "Retained original history identity conflicts"
            }
            // Recovery compares the owner to the exact receipt, including metadata and dispatch ID.
            if (activeOwnerIds != null && retained.id !in activeOwnerIds) {
                merged[matchingId] = if (renamePending) current.copy(
                    metadata = JsonObject(current.metadata + (RETAINED_MARKER to JsonPrimitive(true))),
                ) else retained
            }
        } else if (merged.none { it.outputPath == retained.outputPath &&
                it.committedIdentity == retained.committedIdentity && it.committedSha256 == retained.committedSha256 }
        ) merged += retained
    }
    return merged
}

internal fun creationRetainedRenameIsRecoverable(
    old: CreationHistoryEntry,
    retained: CreationHistoryEntry,
    receipt: CreationHistoryRenameReceipt,
): Boolean = receipt.entryId == old.id && old.id == retained.id &&
    receipt.oldPath == old.outputPath && receipt.oldName == old.outputName &&
    receipt.oldIdentity == old.committedIdentity &&
    receipt.newPath == retained.outputPath && receipt.targetName == retained.outputName &&
    receipt.newIdentity == retained.committedIdentity &&
    receipt.newIdentity != null &&
    old.committedSize == receipt.expectedSize && retained.committedSize == receipt.expectedSize &&
    old.committedSha256 == receipt.expectedSha256 && retained.committedSha256 == receipt.expectedSha256

/** Move the original's retained history identity only as its edited successor commits. */
internal fun transferRetainedSvgOriginal(
    index: File,
    previous: CreationHistoryEntry,
    originalCopy: CreationHistoryEntry,
) {
    val path = runCatching { File(previous.outputPath).canonicalPath }.getOrNull() ?: return
    val records = readRetainedSvgOriginals(index)
    val record = records.singleOrNull { it.outputPath == path } ?: return
    require(record.identity == previous.committedIdentity && record.size == previous.committedSize &&
        record.sha256 == previous.committedSha256) { "Retained original changed while editing" }
    val previousId = retainedSvgOriginalEntry(record).id
    require(previousId == previous.id || previousId == originalCopy.id) {
        "Retained original history identity changed"
    }
    if (previousId == originalCopy.id) return
    val revised = record.copy(entry = originalCopy.copy(outputPath = record.outputPath))
    writeCreationIndexTextAtomically(
        index, Json.encodeToString(records.map { if (it.outputPath == record.outputPath) revised else it }),
        CREATION_SVG_ORIGINALS_MAX_BYTES, requireAtomic = true,
    )
}

/** Rename replay updates protection before publishing the corresponding history update. */
internal fun reconcileRetainedSvgRenames(
    index: File,
    entries: List<CreationHistoryEntry>,
    verify: (CreationHistoryEntry) -> Boolean,
) {
    val records = readRetainedSvgOriginals(index)
    var changed = false
    val revised = records.map { record ->
        val existing = retainedSvgOriginalEntry(record)
        val candidate = entries.firstOrNull { it.id == existing.id } ?: return@map record
        if (candidate.outputPath == record.outputPath && candidate.committedIdentity == record.identity) {
            return@map record
        }
        require(candidate.committedSize == record.size && candidate.committedSha256 == record.sha256 &&
            candidate.committedIdentity != null && verify(candidate)) { "Retained original changed during rename" }
        changed = true
        CreationRetainedSvgOriginal(candidate.outputPath, candidate.committedIdentity, record.size, record.sha256, candidate)
    }
    if (changed) writeCreationIndexTextAtomically(
        index, Json.encodeToString(revised), CREATION_SVG_ORIGINALS_MAX_BYTES, requireAtomic = true,
    )
}

internal fun forgetRetainedSvgOriginal(index: File, record: CreationRetainedSvgOriginal) {
    val records = readRetainedSvgOriginals(index)
    if (record !in records) return
    writeCreationIndexTextAtomically(
        index, Json.encodeToString(records.filterNot { it == record }),
        CREATION_SVG_ORIGINALS_MAX_BYTES, requireAtomic = true,
    )
}
