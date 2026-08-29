package dev.screengoated.toolbox.mobile.creation

import android.content.ContentResolver
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import java.util.UUID

internal class CreationSafTree(
    private val resolver: ContentResolver,
) {
    fun names(tree: Uri): Set<String> {
        val names = mutableSetOf<String>()
        scan(tree) { _, name ->
            name?.let(names::add)
        }
        return names
    }

    fun find(tree: Uri, name: String): Uri? {
        var match: Uri? = null
        scan(tree) { uri, observedName ->
            if (observedName == name) {
                require(match == null) { "The selected folder contains duplicate file names" }
                match = uri
            }
        }
        return match
    }

    fun validateAtomicDestination(tree: Uri) {
        val token = UUID.randomUUID().toString().replace("-", "")
        val pendingName = ".sgt-$token.probe"
        val finalName = ".sgt-$token.ready"
        require(find(tree, pendingName) == null)
        require(find(tree, finalName) == null)
        val parentId = DocumentsContract.getTreeDocumentId(tree)
        val parent = DocumentsContract.buildDocumentUriUsingTree(tree, parentId)
        val pending = requireNotNull(
            DocumentsContract.createDocument(
                resolver,
                parent,
                "application/octet-stream",
                pendingName,
            ),
        )
        val documentId = DocumentsContract.getDocumentId(pending)
        var renamed: Uri? = null
        var renamedDocumentId: String? = null
        try {
            renamed = requireNotNull(
                DocumentsContract.renameDocument(resolver, pending, finalName),
            )
            renamedDocumentId = DocumentsContract.getDocumentId(requireNotNull(renamed))
            require(query(requireNotNull(renamed), OpenableColumns.DISPLAY_NAME) == finalName)
            require(renamedDocumentId == documentId)
        } finally {
            renamed?.let { runCatching { DocumentsContract.deleteDocument(resolver, it) } }
            runCatching { DocumentsContract.deleteDocument(resolver, pending) }
            renamedDocumentId?.let { exactId ->
                runCatching { find(tree, finalName) }.getOrNull()?.let { final ->
                    if (creationSafProbeOwns(exactId, DocumentsContract.getDocumentId(final))) {
                        runCatching { DocumentsContract.deleteDocument(resolver, final) }
                    }
                }
            }
        }
    }

    private fun scan(tree: Uri, visit: (Uri, String?) -> Unit) {
        val documentId = DocumentsContract.getTreeDocumentId(tree)
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, documentId)
        requireNotNull(
            resolver.query(
                children,
                arrayOf(
                    DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                    DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                ),
                null,
                null,
                null,
            ),
        ) { "The selected folder cannot be enumerated" }.use { cursor ->
            var count = 0
            while (cursor.moveToNext()) {
                count += 1
                require(count <= CREATION_SAF_MAXIMUM_CHILDREN) {
                    "The selected folder contains too many files"
                }
                visit(
                    DocumentsContract.buildDocumentUriUsingTree(tree, cursor.getString(0)),
                    cursor.getString(1),
                )
            }
        }
    }

    private fun query(uri: Uri, column: String): String? = runCatching {
        resolver.query(uri, arrayOf(column), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) cursor.getString(0) else null
        }
    }.getOrNull()
}

internal const val CREATION_SAF_MAXIMUM_CHILDREN = 4_096

internal fun creationSafProbeOwns(expectedDocumentId: String?, actualDocumentId: String?): Boolean =
    expectedDocumentId != null && expectedDocumentId == actualDocumentId
