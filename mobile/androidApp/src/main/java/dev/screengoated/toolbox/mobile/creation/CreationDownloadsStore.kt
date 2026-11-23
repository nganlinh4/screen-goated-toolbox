package dev.screengoated.toolbox.mobile.creation

import android.content.ContentResolver
import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.StatFs
import android.provider.MediaStore
import android.provider.OpenableColumns
import java.io.File
import java.io.FileOutputStream

internal class CreationDownloadsStore(context: Context) {
    private val resolver = context.contentResolver
    private val volumePath = context.getExternalFilesDir(null)
    val destination: String = CREATION_DOWNLOADS_DESTINATION

    fun owns(uri: Uri): Boolean = uri.authority == MediaStore.AUTHORITY &&
        uri.pathSegments.getOrNull(1) == DOWNLOADS_SEGMENT

    fun availableBytes(): Long? = volumePath?.let { path ->
        runCatching { StatFs(path.absolutePath).availableBytes }.getOrNull()
    }

    /**
     * Finds a Downloads row by name, including this app's IS_PENDING rows. A plain
     * MediaStore query hides pending rows, so a crash-resumed reservation would
     * insert a second row that MediaStore renames to "<name> (1)".
     */
    fun find(name: String): Uri? {
        val projection = arrayOf(MediaStore.Downloads._ID)
        val selection = "${MediaStore.Downloads.RELATIVE_PATH} = ? AND " +
            "${MediaStore.Downloads.DISPLAY_NAME} = ?"
        val selectionArgs = arrayOf(DOWNLOADS_PATH, name)
        val sortOrder = "${MediaStore.Downloads._ID} DESC"
        val cursor = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val queryArgs = Bundle().apply {
                putString(ContentResolver.QUERY_ARG_SQL_SELECTION, selection)
                putStringArray(ContentResolver.QUERY_ARG_SQL_SELECTION_ARGS, selectionArgs)
                putString(ContentResolver.QUERY_ARG_SQL_SORT_ORDER, sortOrder)
                putInt(MediaStore.QUERY_ARG_MATCH_PENDING, MediaStore.MATCH_INCLUDE)
            }
            resolver.query(MediaStore.Downloads.EXTERNAL_CONTENT_URI, projection, queryArgs, null)
        } else {
            @Suppress("DEPRECATION")
            val includePending = MediaStore.setIncludePending(MediaStore.Downloads.EXTERNAL_CONTENT_URI)
            resolver.query(includePending, projection, selection, selectionArgs, sortOrder)
        }
        return cursor?.use {
            if (!it.moveToFirst()) return@use null
            ContentUris.withAppendedId(MediaStore.Downloads.EXTERNAL_CONTENT_URI, it.getLong(0))
        }
    }

    fun displayName(uri: Uri): String? = resolver.query(
        uri,
        arrayOf(OpenableColumns.DISPLAY_NAME),
        null,
        null,
        null,
    )?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }

    fun reserve(intent: CreationPublishIntent): CreationPendingReservation {
        require(find(intent.finalName) == null) { "Creation destination is already occupied" }
        val pendingName = requireNotNull(intent.pendingName)
        require(
            pendingName == creationDownloadsPendingName(
                requireNotNull(intent.reservationToken),
                intent.finalName,
            ),
        )
        find(pendingName)?.let { pending ->
            return CreationPendingReservation(pending.toString(), pending.toString())
        }
        val values = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, pendingName)
            put(MediaStore.Downloads.MIME_TYPE, intent.mimeType)
            put(MediaStore.Downloads.RELATIVE_PATH, DOWNLOADS_PATH)
            put(MediaStore.Downloads.IS_PENDING, 1)
        }
        val pending = requireNotNull(
            resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values),
        ) { "Could not reserve output file" }
        return CreationPendingReservation(pending.toString(), pending.toString())
    }

    fun populate(intent: CreationPublishIntent, pendingHandle: String, source: File) {
        val pending = Uri.parse(pendingHandle)
        require(owns(pending) && displayName(pending) == intent.pendingName)
        requireNotNull(resolver.openFileDescriptor(pending, "w")) {
            "Could not write output file"
        }.use { descriptor ->
            FileOutputStream(descriptor.fileDescriptor).use { output ->
                source.inputStream().use { it.copyTo(output) }
                output.fd.sync()
            }
        }
    }

    fun publish(intent: CreationPublishIntent, pendingHandle: String): String {
        val pending = Uri.parse(pendingHandle)
        require(owns(pending) && displayName(pending) == intent.pendingName)
        require(find(intent.finalName) == null) { "Creation destination is already occupied" }
        val values = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, intent.finalName)
            put(MediaStore.Downloads.IS_PENDING, 0)
        }
        require(resolver.update(pending, values, null, null) == 1) {
            "Could not publish output file"
        }
        require(displayName(pending) == intent.finalName) {
            "Downloads changed the output name"
        }
        return pending.toString()
    }

    fun rename(uri: Uri, targetName: String): Uri {
        require(owns(uri))
        val values = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, targetName)
        }
        require(resolver.update(uri, values, null, null) == 1) {
            "Could not rename output file"
        }
        require(displayName(uri) == targetName) { "Downloads changed the output name" }
        return uri
    }

    fun delete(uri: Uri): Boolean = resolver.delete(uri, null, null) > 0

    private companion object {
        val DOWNLOADS_PATH = "${Environment.DIRECTORY_DOWNLOADS}/"
        const val DOWNLOADS_SEGMENT = "downloads"
    }
}

internal const val CREATION_DOWNLOADS_DESTINATION = "content://media/external/downloads"
