package dev.screengoated.toolbox.mobile.history

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import java.io.File

internal object HistoryExternalActions {
    fun openItem(
        context: Context,
        file: File,
    ): Boolean {
        return runCatching {
            val uri = FileProvider.getUriForFile(
                context,
                "${context.packageName}.fileprovider",
                file,
            )
            val intent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, mimeTypeFor(file))
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            context.startActivity(intent)
        }.isSuccess
    }

    fun openFolder(
        context: Context,
        folder: File,
        supportsFolderOpen: Boolean,
    ): Boolean {
        if (!supportsFolderOpen) {
            return false
        }
        val storagePath = folder.absolutePath
            .removePrefix("/storage/emulated/0/")
            .replace("/", "%2F")
        val docUri = Uri.parse(
            "content://com.android.externalstorage.documents/document/primary%3A$storagePath",
        )
        return runCatching {
            val intent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(docUri, DocumentsContract.Document.MIME_TYPE_DIR)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            context.startActivity(intent)
        }.recoverCatching {
            val fallback = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
                putExtra(DocumentsContract.EXTRA_INITIAL_URI, docUri)
            }
            context.startActivity(fallback)
        }.isSuccess
    }

    private fun mimeTypeFor(file: File): String {
        val ext = file.extension.lowercase()
        return MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: when (ext) {
            "png", "jpg", "jpeg", "webp" -> "image/*"
            "wav", "mp3", "m4a", "ogg", "opus" -> "audio/*"
            "txt", "md" -> "text/plain"
            else -> "*/*"
        }
    }
}
