package dev.screengoated.toolbox.mobile.history

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.AndroidFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileFingerprint
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileObjectId
import java.io.FileNotFoundException

/** Grants access only to the verified object associated with the current History item. */
class HistoryMediaProvider : ContentProvider() {
    override fun onCreate() = true

    private fun request(uri: Uri): HistoryMediaRequest {
        if (uri.scheme != "content" || uri.authority != "${requireNotNull(context).packageName}.history-media") throw FileNotFoundException()
        return historyMediaRequest(uri.pathSegments)
    }

    private fun current(request: HistoryMediaRequest): Pair<HistoryRepository, HistoryUiState> {
        val app = requireNotNull(context).applicationContext as? SgtMobileApplication ?: throw FileNotFoundException()
        val repository = app.appContainerOrNull?.historyRepository ?: throw FileNotFoundException()
        val state = repository.state.value
        authorizeHistoryMedia(state, request)
        return repository to state
    }

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        if (mode != "r") throw FileNotFoundException()
        var duplicate: ParcelFileDescriptor? = null
        return try {
            val request = request(uri)
            val (repository, snapshot) = current(request)
            val pieces = request.proof.identity.split(':')
            if (pieces.size != 2) throw FileNotFoundException()
            val identity = FileObjectId(pieces[0].toLongOrNull() ?: throw FileNotFoundException(),
                pieces[1].toLongOrNull() ?: throw FileNotFoundException())
            val io = AndroidFileTransactionIo
            HistoryDirectoryRoute(io, repository.mediaDirectory().toPath()).use { route ->
                openVerifiedHistoryMedia(io, route, request.name, identity,
                    FileFingerprint(request.proof.size, request.proof.sha256), request.proof.size.toInt()).use { file ->
                    duplicate = repository.admitMediaOpen(snapshot) { file.duplicateReadDescriptor() }
                        ?: throw FileNotFoundException()
                }
            }
            requireNotNull(duplicate)
        } catch (error: Throwable) {
            try { duplicate?.close() } catch (close: Throwable) { error.addSuppressed(close) }
            if (error !is Exception && error !is LinkageError) throw error
            throw FileNotFoundException("History media unavailable").also { it.initCause(error) }
        }
    }

    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, selectionArgs: Array<out String>?, sortOrder: String?): Cursor {
        val request = request(uri)
        current(request)
        val columns = projection ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        return MatrixCursor(columns).apply {
            addRow(columns.map { when (it) { OpenableColumns.DISPLAY_NAME -> request.name; OpenableColumns.SIZE -> request.proof.size; else -> null } })
        }
    }

    override fun getType(uri: Uri): String {
        val extension = request(uri).name.substringAfterLast('.', "").lowercase()
        return MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension) ?: "application/octet-stream"
    }
    override fun insert(uri: Uri, values: ContentValues?): Uri? = throw UnsupportedOperationException()
    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?) = throw UnsupportedOperationException()
    override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?) = throw UnsupportedOperationException()
}
