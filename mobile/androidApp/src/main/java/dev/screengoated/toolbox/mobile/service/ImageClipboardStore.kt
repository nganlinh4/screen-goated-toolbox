package dev.screengoated.toolbox.mobile.service

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.core.content.FileProvider
import java.io.File

internal object ImageClipboardStore {
    private const val MAX_FILES = 64
    private const val MAX_BYTES = 128L * 1024 * 1024
    // Android exposes only the newest primary clip; these newest files are never evicted so a
    // paste (or clipboard-history app) still reading a recent URI keeps its bytes.
    private const val PROTECTED_NEWEST = 8

    @Synchronized
    fun publish(directory: File, bytes: ByteArray, prepare: (File) -> () -> Unit): Boolean {
        if (bytes.size.toLong() > MAX_BYTES) return false
        var file: File? = null
        var publishing = false
        return try {
            if (!directory.isDirectory && !directory.mkdirs()) return false
            val existing = (directory.listFiles() ?: return false)
                .filter(File::isFile)
                .sortedWith(compareBy(File::lastModified, File::getName))
                .toMutableList()
            if (!evictOldestUntilFits(existing, bytes.size.toLong())) return false
            // The zero-padded monotonic prefix orders copies made within one timestamp tick.
            file = File.createTempFile("image-%020d-".format(System.nanoTime()), ".png", directory)
            file.writeBytes(bytes)
            val publish = prepare(file)
            // Once publication starts, a thrown exception cannot prove that the
            // system did not accept the URI. Retain that file conservatively.
            publishing = true
            publish()
            true
        } catch (_: Exception) {
            if (!publishing) file?.delete()
            false
        }
    }

    /** Evicts the oldest files, never the newest few, until one more file fits both limits. */
    private fun evictOldestUntilFits(oldestFirst: MutableList<File>, incoming: Long): Boolean {
        var used = oldestFirst.sumOf(File::length)
        while (oldestFirst.size >= MAX_FILES || used + incoming > MAX_BYTES) {
            if (oldestFirst.size <= PROTECTED_NEWEST) return false
            val oldest = oldestFirst.removeAt(0)
            val size = oldest.length()
            if (!oldest.delete()) return false
            used -= size
        }
        return true
    }
}

internal fun publishImageClipboard(context: Context, manager: ClipboardManager, bytes: ByteArray): Boolean =
    ImageClipboardStore.publish(File(context.cacheDir, "clipboard-images"), bytes) { file ->
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
        val clip = ClipData.newUri(context.contentResolver, "SGT Image", uri)
        val publishClip: () -> Unit = { manager.setPrimaryClip(clip) }
        publishClip
    }
