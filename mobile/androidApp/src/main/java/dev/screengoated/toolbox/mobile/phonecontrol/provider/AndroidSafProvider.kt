package dev.screengoated.toolbox.mobile.phonecontrol.provider

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.nio.charset.CodingErrorAction
import java.io.ByteArrayOutputStream
import java.io.InputStream

internal class AndroidSafProvider(
    private val context: Context,
    private val artifacts: PhoneControlArtifactStore,
) {
    fun list(tree: String, sortBy: String, descending: Boolean, limit: Int): AndroidProviderResult {
        val treeUri = parseContentUri(tree) ?: return invalidUri()
        val documentId = runCatching {
            if ("document" in treeUri.pathSegments) {
                DocumentsContract.getDocumentId(treeUri)
            } else {
                DocumentsContract.getTreeDocumentId(treeUri)
            }
        }.getOrElse {
            return failure("not_tree_uri", "The resource is not a document-tree URI.")
        }
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, documentId)
        val items = mutableListOf<SafItem>()
        val query = runCatching {
            context.contentResolver.query(children, PROJECTION, null, null, null)
        }.getOrNull() ?: return failure("grant_unavailable", "The document-tree grant is unavailable.")
        query.use { cursor ->
            val idColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
            val nameColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            val mimeColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_MIME_TYPE)
            val sizeColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_SIZE)
            val modifiedColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
            val flagsColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_FLAGS)
            while (cursor.moveToNext()) {
                val childId = cursor.getString(idColumn)
                val mime = cursor.getString(mimeColumn).orEmpty()
                items += SafItem(
                    name = cursor.getString(nameColumn).orEmpty(),
                    uri = DocumentsContract.buildDocumentUriUsingTree(treeUri, childId).toString(),
                    mimeType = mime,
                    size = cursor.getLong(sizeColumn),
                    modifiedMs = cursor.getLong(modifiedColumn),
                    flags = cursor.getInt(flagsColumn),
                    directory = mime == DocumentsContract.Document.MIME_TYPE_DIR,
                )
            }
        }
        val comparator = when (sortBy) {
            "name" -> compareBy<SafItem> { it.name.lowercase() }
            "size" -> compareBy(SafItem::size)
            else -> compareBy(SafItem::modifiedMs)
        }
        val selected = items.sortedWith(if (descending) comparator.reversed() else comparator)
            .take(limit.coerceIn(1, MAX_LIST_ITEMS))
        return AndroidProviderResult.Success(
            buildJsonObject {
                put("tree_uri", treeUri.toString())
                put("count", selected.size)
                put(
                    "items",
                    buildJsonArray {
                        selected.forEach { item ->
                            add(
                                buildJsonObject {
                                    put("name", item.name)
                                    put("uri", item.uri)
                                    put("kind", if (item.directory) "directory" else "file")
                                    put("mime_type", item.mimeType)
                                    put("size", item.size)
                                    put("modified_ms", item.modifiedMs)
                                    put("writable", item.flags and WRITABLE_FLAGS != 0)
                                },
                            )
                        }
                    },
                )
            },
        )
    }

    fun readText(resource: String, expectedSha256: String?, maxChars: Int): AndroidProviderResult {
        val uri = parseContentUri(resource) ?: return invalidUri()
        val bytes = runCatching {
            context.contentResolver.openInputStream(uri)?.use { input ->
                input.readBounded(MAX_TEXT_BYTES + 1)
            } ?: error("The document provider returned no stream.")
        }.getOrElse { error ->
            return failure("read_failed", error.message ?: "The document could not be read.")
        }
        if (bytes.size > MAX_TEXT_BYTES) {
            return failure("file_too_large", "The document exceeds the bounded text limit.")
        }
        val sha = bytes.sha256()
        if (expectedSha256 != null && !sha.equals(expectedSha256, ignoreCase = true)) {
            return failure("hash_mismatch", "The document changed since the supplied hash.")
        }
        val text = decodeUtf8(bytes) ?: return failure("not_utf8", "The document is not UTF-8 text.")
        val bounded = text.take(maxChars.coerceIn(1, MAX_TEXT_CHARS))
        val name = displayName(uri)
        val artifact = artifacts.put(bytes, "text/plain; charset=utf-8", name)
        return AndroidProviderResult.Success(
            buildJsonObject {
                put("uri", uri.toString())
                put("name", name.orEmpty())
                put("sha256", sha)
                put("text", bounded)
                put("characters", text.length)
                put("truncated", bounded.length < text.length)
                put("artifact_id", artifact.id)
            },
        )
    }

    private fun displayName(uri: Uri): String? = runCatching {
        context.contentResolver.query(
            uri,
            arrayOf(OpenableColumns.DISPLAY_NAME),
            null,
            null,
            null,
        )?.use { cursor ->
            if (cursor.moveToFirst()) cursor.getString(0) else null
        }
    }.getOrNull()

    private fun parseContentUri(value: String): Uri? = runCatching { Uri.parse(value.trim()) }
        .getOrNull()
        ?.takeIf { it.scheme == "content" }

    private fun invalidUri() = failure("invalid_resource", "A content:// document URI is required.")

    private fun failure(code: String, message: String) = AndroidProviderResult.Failure(
        code = code,
        message = message,
        requiredUserStep = if (code == "grant_unavailable") {
            "Choose the folder again in Phone Control setup."
        } else {
            null
        },
    )

    private data class SafItem(
        val name: String,
        val uri: String,
        val mimeType: String,
        val size: Long,
        val modifiedMs: Long,
        val flags: Int,
        val directory: Boolean,
    )

    private companion object {
        val PROJECTION = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
            DocumentsContract.Document.COLUMN_FLAGS,
        )
        const val WRITABLE_FLAGS = DocumentsContract.Document.FLAG_SUPPORTS_WRITE or
            DocumentsContract.Document.FLAG_SUPPORTS_DELETE or
            DocumentsContract.Document.FLAG_DIR_SUPPORTS_CREATE
        const val MAX_LIST_ITEMS = 2_000
        const val MAX_TEXT_BYTES = 8 * 1024 * 1024
        const val MAX_TEXT_CHARS = 64_000
    }
}

private fun decodeUtf8(bytes: ByteArray): String? = runCatching {
    Charsets.UTF_8.newDecoder()
        .onMalformedInput(CodingErrorAction.REPORT)
        .onUnmappableCharacter(CodingErrorAction.REPORT)
        .decode(java.nio.ByteBuffer.wrap(bytes))
        .toString()
}.getOrNull()

private fun InputStream.readBounded(limit: Int): ByteArray {
    val output = ByteArrayOutputStream(minOf(limit, 64 * 1024))
    val buffer = ByteArray(8192)
    while (output.size() < limit) {
        val read = read(buffer, 0, minOf(buffer.size, limit - output.size()))
        if (read < 0) break
        output.write(buffer, 0, read)
    }
    return output.toByteArray()
}
