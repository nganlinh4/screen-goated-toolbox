package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.content.Intent
import android.database.Cursor
import android.net.Uri
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.provider.OpenableColumns
import androidx.core.content.FileProvider
import java.io.File
import java.io.FileOutputStream
import java.io.InputStream
import java.util.UUID

internal class CreationFileStore(private val context: Context) {
    private val resolver = context.contentResolver
    private val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    fun importImages(uris: List<Uri>): List<String> {
        val sourceDir = File(context.filesDir, "creation/sources").apply { mkdirs() }
        return uris.mapNotNull { uri ->
            runCatching {
                val original = displayName(uri) ?: "image"
                val safe = safeName(original)
                val target = uniqueFile(sourceDir, safe)
                resolver.openInputStream(uri).use { input ->
                    requireNotNull(input) { "Could not open $original" }
                    FileOutputStream(target).use(input::copyTo)
                }
                target.absolutePath
            }.getOrNull()
        }
    }

    fun rememberOutputDirectory(uri: Uri): String {
        runCatching {
            resolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
            )
        }
        preferences.edit().putString(KEY_OUTPUT_TREE, uri.toString()).apply()
        return outputDirectoryLabel(uri)
    }

    fun defaultOutputDirectoryLabel(): String {
        val tree = outputTree()
        return if (tree == null) DEFAULT_OUTPUT_LABEL else outputDirectoryLabel(tree)
    }

    fun stagingFile(tool: CreationTool, sourcePath: String, extension: String): File {
        val directory = File(context.filesDir, "creation/staging/${tool.wireName}").apply { mkdirs() }
        val stem = File(sourcePath).nameWithoutExtension.ifBlank { tool.wireName }
        return uniqueFile(directory, "${safeStem(stem)}.$extension").also { file ->
            check(file.createNewFile()) { "Could not reserve output file" }
        }
    }

    fun publish(staging: File, preferredName: String, mimeType: String): String {
        val tree = outputTree()
        val uri = if (tree != null) {
            publishToTree(tree, staging, preferredName, mimeType)
        } else {
            publishToDownloads(staging, preferredName, mimeType)
        }
        staging.delete()
        return uri.toString()
    }

    fun exists(path: String): Boolean = when (val uri = path.toContentUri()) {
        null -> File(path).isFile
        else -> runCatching {
            resolver.openAssetFileDescriptor(uri, "r")?.use { true } ?: false
        }.getOrDefault(false)
    }

    fun size(path: String): Long = when (val uri = path.toContentUri()) {
        null -> File(path).length()
        else -> query(uri, OpenableColumns.SIZE)?.toLongOrNull() ?: -1L
    }

    fun readBytes(path: String, maximum: Long): ByteArray {
        val knownSize = size(path)
        require(knownSize < 0 || knownSize <= maximum) { "Preview asset is too large" }
        val input = path.toContentUri()?.let(resolver::openInputStream)
            ?: File(path).inputStream()
        return input.use { stream ->
            val bytes = stream.readBytes()
            require(bytes.size.toLong() <= maximum) { "Preview asset is too large" }
            bytes
        }
    }

    fun writeText(path: String, value: String) {
        val uri = path.toContentUri()
        if (uri == null) {
            File(path).writeText(value)
        } else {
            requireNotNull(resolver.openOutputStream(uri, "wt")) { "Result is not writable" }
                .bufferedWriter()
                .use { it.write(value) }
        }
    }

    fun rename(path: String, requestedName: String): Pair<String, String> {
        val clean = safeName(requestedName)
        require(clean.isNotBlank()) { "Enter a file name" }
        val uri = path.toContentUri()
        if (uri == null) {
            val current = File(path)
            val target = uniqueFile(current.parentFile ?: error("Missing parent folder"), clean)
            check(current.renameTo(target)) { "Could not rename result" }
            return target.absolutePath to target.name
        }
        val renamed = runCatching { DocumentsContract.renameDocument(resolver, uri, clean) }
            .getOrNull()
            ?: run {
                val values = android.content.ContentValues().apply {
                    put(MediaStore.MediaColumns.DISPLAY_NAME, clean)
                }
                check(resolver.update(uri, values, null, null) > 0) { "Could not rename result" }
                uri
            }
        return renamed.toString() to (displayName(renamed) ?: clean)
    }

    fun delete(path: String): Boolean = path.toContentUri()?.let { uri ->
        runCatching { DocumentsContract.deleteDocument(resolver, uri) }.getOrDefault(false) ||
            resolver.delete(uri, null, null) > 0
    } ?: File(path).delete()

    fun openExternally(path: String) {
        val uri = path.toContentUri() ?: FileProvider.getUriForFile(
            context,
            "${context.packageName}.fileprovider",
            File(path),
        )
        context.startActivity(
            Intent(Intent.ACTION_VIEW).setData(uri)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }

    fun openInput(path: String): InputStream = path.toContentUri()?.let(resolver::openInputStream)
        ?: File(path).inputStream()

    fun uploadUri(path: String): Uri = path.toContentUri() ?: FileProvider.getUriForFile(
        context,
        "${context.packageName}.fileprovider",
        File(path),
    )

    fun materializePreview(path: String, extension: String): File {
        val uri = path.toContentUri() ?: return File(path)
        val directory = File(context.cacheDir, "creation/previews").apply { mkdirs() }
        val safeExtension = extension.lowercase().filter(Char::isLetterOrDigit).ifBlank { "bin" }
        val target = File(directory, "${path.hashCode().toUInt().toString(16)}.$safeExtension")
        val expectedSize = size(path)
        if (target.isFile && target.length() > 0L &&
            (expectedSize < 0L || target.length() == expectedSize)
        ) {
            return target
        }
        val temporary = File(directory, "${target.name}.tmp")
        requireNotNull(resolver.openInputStream(uri)) { "Preview is unavailable" }.use { input ->
            FileOutputStream(temporary).use(input::copyTo)
        }
        if (!temporary.renameTo(target)) {
            temporary.copyTo(target, overwrite = true)
            temporary.delete()
        }
        return target
    }

    private fun outputTree(): Uri? = preferences.getString(KEY_OUTPUT_TREE, null)
        ?.let(Uri::parse)
        ?.takeIf { uri -> resolver.persistedUriPermissions.any { it.uri == uri && it.isWritePermission } }

    private fun publishToTree(tree: Uri, source: File, name: String, mime: String): Uri {
        val parentId = DocumentsContract.getTreeDocumentId(tree)
        val parent = DocumentsContract.buildDocumentUriUsingTree(tree, parentId)
        val target = requireNotNull(DocumentsContract.createDocument(resolver, parent, mime, name)) {
            "Could not create output file"
        }
        resolver.openOutputStream(target, "w").use { output ->
            requireNotNull(output) { "Could not write output file" }
            source.inputStream().use { it.copyTo(output) }
        }
        return target
    }

    private fun publishToDownloads(source: File, name: String, mime: String): Uri {
        val values = android.content.ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, name)
            put(MediaStore.MediaColumns.MIME_TYPE, mime)
            put(MediaStore.MediaColumns.RELATIVE_PATH, "${Environment.DIRECTORY_DOWNLOADS}/SGT")
            put(MediaStore.MediaColumns.IS_PENDING, 1)
        }
        val collection = MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
        val target = requireNotNull(resolver.insert(collection, values)) { "Could not create output file" }
        try {
            resolver.openOutputStream(target, "w").use { output ->
                requireNotNull(output) { "Could not write output file" }
                source.inputStream().use { it.copyTo(output) }
            }
            values.clear()
            values.put(MediaStore.MediaColumns.IS_PENDING, 0)
            resolver.update(target, values, null, null)
            return target
        } catch (error: Throwable) {
            resolver.delete(target, null, null)
            throw error
        }
    }

    private fun outputDirectoryLabel(uri: Uri): String {
        val name = query(uri, DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            ?: query(DocumentsContract.buildDocumentUriUsingTree(uri, DocumentsContract.getTreeDocumentId(uri)), DocumentsContract.Document.COLUMN_DISPLAY_NAME)
        return name?.let { "Storage/$it" } ?: "Storage"
    }

    private fun displayName(uri: Uri): String? = query(uri, OpenableColumns.DISPLAY_NAME)

    private fun query(uri: Uri, column: String): String? {
        var cursor: Cursor? = null
        return try {
            cursor = resolver.query(uri, arrayOf(column), null, null, null)
            if (cursor?.moveToFirst() == true) cursor.getString(0) else null
        } catch (_: Throwable) {
            null
        } finally {
            cursor?.close()
        }
    }

    private fun uniqueFile(directory: File, requested: String): File {
        val safe = safeName(requested)
        val first = File(directory, safe)
        if (!first.exists()) return first
        val dot = safe.lastIndexOf('.')
        val stem = if (dot > 0) safe.substring(0, dot) else safe
        val extension = if (dot > 0) safe.substring(dot) else ""
        repeat(9_998) { offset ->
            val candidate = File(directory, "${stem}_${offset + 2}$extension")
            if (!candidate.exists()) return candidate
        }
        return File(directory, "${stem}_${UUID.randomUUID()}$extension")
    }

    private fun safeName(value: String): String = value
        .substringAfterLast('/')
        .substringAfterLast('\\')
        .map { if (it.isLetterOrDigit() || it in "._-") it else '_' }
        .joinToString("")
        .trim('.', ' ')
        .ifBlank { "result" }

    private fun safeStem(value: String): String = safeName(value).substringBeforeLast('.').ifBlank { "result" }

    private fun String.toContentUri(): Uri? = takeIf { it.startsWith("content://") }?.let(Uri::parse)

    private companion object {
        const val PREFERENCES = "creation_output"
        const val KEY_OUTPUT_TREE = "tree_uri"
        const val DEFAULT_OUTPUT_LABEL = "/storage/emulated/0/Download/SGT"
    }
}
