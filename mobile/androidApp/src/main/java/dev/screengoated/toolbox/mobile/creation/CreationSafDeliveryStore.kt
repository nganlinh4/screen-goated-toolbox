package dev.screengoated.toolbox.mobile.creation

import android.content.ContentResolver
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import java.io.File
import java.io.FileOutputStream

internal class CreationSafDeliveryStore(
    private val resolver: ContentResolver,
    private val trees: CreationSafTree,
) {
    fun reserve(intent: CreationPublishIntent): CreationPendingReservation {
        val tree = Uri.parse(requireNotNull(intent.destination))
        require(trees.find(tree, intent.finalName) == null) {
            "Creation destination is already occupied"
        }
        val pendingName = requireNotNull(intent.pendingName)
        require(pendingName == ".sgt-${requireNotNull(intent.reservationToken)}.pending")
        trees.find(tree, pendingName)?.let { existing ->
            return CreationPendingReservation(
                existing.toString(),
                requireNotNull(identity(existing)),
            )
        }
        val parentId = DocumentsContract.getTreeDocumentId(tree)
        val parent = DocumentsContract.buildDocumentUriUsingTree(tree, parentId)
        val pending = requireNotNull(
            DocumentsContract.createDocument(resolver, parent, intent.mimeType, pendingName),
        ) { "Could not reserve output file" }
        return CreationPendingReservation(pending.toString(), requireNotNull(identity(pending)))
    }

    fun populate(intent: CreationPublishIntent, pendingHandle: String, source: File) {
        val pending = Uri.parse(pendingHandle)
        require(query(pending, OpenableColumns.DISPLAY_NAME) == intent.pendingName)
        requireNotNull(resolver.openFileDescriptor(pending, "w")) {
            "Could not write output file"
        }.use { descriptor ->
            FileOutputStream(descriptor.fileDescriptor).use { output ->
                source.inputStream().use { it.copyTo(output) }
                output.fd.sync()
            }
        }
    }

    fun commit(
        intent: CreationPublishIntent,
        pendingHandle: String,
        pendingIdentity: String,
        expectedSize: Long,
        expectedSha256: String,
    ): String {
        val tree = Uri.parse(requireNotNull(intent.destination))
        val pending = Uri.parse(pendingHandle)
        trees.find(tree, intent.finalName)?.let { published ->
            require(
                identity(published) == pendingIdentity &&
                    verified(published, expectedSize, expectedSha256),
            ) { "Creation destination is already occupied" }
            return published.toString()
        }
        require(verified(pending, expectedSize, expectedSha256)) {
            "Creation output verification failed"
        }
        require(identity(pending) == pendingIdentity)
        require(trees.find(tree, intent.finalName) == null) {
            "Creation destination is already occupied"
        }
        val published = requireNotNull(
            DocumentsContract.renameDocument(resolver, pending, intent.finalName),
        ) { "The selected folder cannot publish files atomically" }
        require(verified(published, expectedSize, expectedSha256)) {
            "Creation output verification failed"
        }
        require(query(published, OpenableColumns.DISPLAY_NAME) == intent.finalName) {
            "The selected folder changed the output name"
        }
        require(identity(published) == pendingIdentity)
        return published.toString()
    }

    private fun verified(uri: Uri, size: Long, digest: String): Boolean = runCatching {
        val observedSize = query(uri, OpenableColumns.SIZE)?.toLongOrNull()
        observedSize == size && resolver.openInputStream(uri)?.use {
            creationStreamSha256(it)
        }?.equals(digest, ignoreCase = true) == true
    }.getOrDefault(false)

    private fun identity(uri: Uri): String? = runCatching {
        "${requireNotNull(uri.authority)}:${DocumentsContract.getDocumentId(uri)}"
    }.getOrNull()

    private fun query(uri: Uri, column: String): String? = runCatching {
        resolver.query(uri, arrayOf(column), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) cursor.getString(0) else null
        }
    }.getOrNull()
}
