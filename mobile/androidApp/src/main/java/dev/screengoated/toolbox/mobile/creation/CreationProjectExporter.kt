package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.net.Uri
import java.io.File
import java.util.UUID
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

internal data class CreationProjectExport(
    val outputName: String,
    val companionName: String?,
)

internal class CreationProjectExporter(
    context: Context,
    private val files: CreationFileStore,
) {
    private val downloads = CreationDownloadsStore(context)

    fun export(entry: CreationHistoryEntry): CreationProjectExport {
        require(entry.tool == CreationTool.IMAGE_TO_3D.wireName) {
            "Only 3D project revisions can be exported here"
        }
        val primary = provenManagedFile(
            entry.outputPath,
            entry.committedSize,
            entry.committedSha256,
            entry.committedIdentity,
        )
        val companionPath = entry.metadata["download"]
            ?.jsonObject?.get("path")?.jsonPrimitive?.contentOrNull
        val companion = companionPath?.let {
            provenManagedFile(
                it,
                entry.companionCommittedSize,
                entry.companionCommittedSha256,
                entry.companionCommittedIdentity,
            )
        }
        val names = availableNames(entry.outputName, companion?.name)
        val published = mutableListOf<Uri>()
        try {
            publish(primary, names.first).also(published::add)
            companion?.let { publish(it, requireNotNull(names.second)).also(published::add) }
            verify(published[0], requireNotNull(entry.committedSize), requireNotNull(entry.committedSha256))
            if (companion != null) {
                verify(
                    published[1],
                    requireNotNull(entry.companionCommittedSize),
                    requireNotNull(entry.companionCommittedSha256),
                )
            }
        } catch (failure: Throwable) {
            published.forEach(downloads::delete)
            throw failure
        }
        return CreationProjectExport(names.first, names.second)
    }

    private fun provenManagedFile(
        path: String,
        size: Long?,
        sha256: String?,
        identity: String?,
    ): File {
        val managed = requireNotNull(files.managedPathIdentity(path)) {
            "This revision is not an app-managed project artifact"
        }
        require(managed == identity && size != null && sha256 != null) {
            "This revision has no committed export proof"
        }
        return File(managed).also {
            require(creationFileMatchesProof(it, size, sha256)) {
                "This revision changed after it was saved"
            }
        }
    }

    private fun availableNames(primary: String, companion: String?): Pair<String, String?> {
        val safePrimary = safeCreationOutputName(primary)
        val transactionId = UUID.randomUUID().toString().replace("-", "")
        if (companion == null) {
            return uniqueCreationDownloadsName(
                safePrimary,
                { downloads.find(it) != null },
                transactionId,
            ) to null
        }
        val safeCompanion = safeCreationOutputName(companion)
        return creationHistoryRenameNames(
            safePrimary,
            safeCompanion,
            { it == safePrimary || downloads.find(it) != null },
            { it == safeCompanion || downloads.find(it) != null },
            transactionId,
        )
    }

    private fun publish(source: File, finalName: String): Uri {
        val token = UUID.randomUUID().toString().replace("-", "")
        val intent = CreationPublishIntent(
            kind = "downloads",
            destination = downloads.destination,
            finalName = finalName,
            mimeType = if (finalName.endsWith(".fbx", true)) {
                "application/octet-stream"
            } else {
                "model/gltf-binary"
            },
            pendingName = creationDownloadsPendingName(token, finalName),
            reservationToken = token,
        )
        val reservation = downloads.reserve(intent)
        return try {
            downloads.populate(intent, reservation.handle, source)
            Uri.parse(downloads.publish(intent, reservation.handle))
        } catch (failure: Throwable) {
            downloads.delete(Uri.parse(reservation.handle))
            throw failure
        }
    }

    private fun verify(uri: Uri, size: Long, sha256: String) {
        val path = uri.toString()
        require(files.size(path) == size && files.sha256(path).equals(sha256, true)) {
            "Downloads did not preserve the revision bytes"
        }
    }
}
