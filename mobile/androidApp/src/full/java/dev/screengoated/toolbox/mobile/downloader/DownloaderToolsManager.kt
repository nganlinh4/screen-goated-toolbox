package dev.screengoated.toolbox.mobile.downloader

import android.content.Context
import android.os.Environment
import androidx.core.content.edit
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.util.concurrent.ConcurrentHashMap

// Tool/deps (yt-dlp + ffmpeg native binaries) management for standalone distributions.
private const val GH_RELEASE =
    "https://github.com/nganlinh4/youtubedl-android/releases/download/v0.18.1-sgt"
private val NATIVE_ZIP_FILES = listOf(
    "libpython.zip.so" to "$GH_RELEASE/libpython.zip.so",
    "libffmpeg.zip.so" to "$GH_RELEASE/libffmpeg.zip.so",
)

internal fun DownloaderRepository.isNativeZipsDownloaded(): Boolean {
    return File(nativeZipDir, "libpython.zip.so").exists() &&
        File(nativeZipDir, "libffmpeg.zip.so").exists()
}

internal fun DownloaderRepository.ensureInit() {
    if (!initialized) {
        val hasZips = isNativeZipsDownloaded()
        val zipDir = if (hasZips) nativeZipDir else null
        android.util.Log.d("SGT-DL", "ensureInit hasZips=$hasZips zipDir=$zipDir")
        YoutubeDL.getInstance().init(context, zipDir)
        FFmpeg.getInstance().init(context, zipDir)
        initialized = true
        android.util.Log.d("SGT-DL", "ensureInit done")
    }
}

internal fun DownloaderRepository.isAlreadyExtracted(): Boolean {
    val ytdlDir = File(context.noBackupFilesDir, "youtubedl-android")
    val ytdlpExists = File(ytdlDir, "yt-dlp").exists()
    val pythonExists = File(ytdlDir, "packages/python").exists()
    return ytdlpExists && pythonExists
}


internal fun DownloaderRepository.downloadNativeZips() {
    nativeZipDir.mkdirs()
    val client = okhttp3.OkHttpClient.Builder()
        .connectTimeout(30, java.util.concurrent.TimeUnit.SECONDS)
        .readTimeout(300, java.util.concurrent.TimeUnit.SECONDS)
        .followRedirects(true)
        .build()
    val totalFiles = NATIVE_ZIP_FILES.size
    for ((fileIdx, entry) in NATIVE_ZIP_FILES.withIndex()) {
        val (filename, url) = entry
        val target = File(nativeZipDir, filename)
        if (target.exists()) continue

        _state.update {
            it.copy(ytdlp = ToolState(
                ToolInstallStatus.DOWNLOADING,
                version = "Downloading ${fileIdx + 1}/$totalFiles: $filename",
            ))
        }

        val request = okhttp3.Request.Builder().url(url)
            .header("User-Agent", "Mozilla/5.0 SGT-Mobile").build()
        val response = client.newCall(request).execute()
        if (!response.isSuccessful) throw Exception("HTTP ${response.code} downloading $filename")
        val body = response.body
        val totalBytes = body.contentLength()
        val tmpFile = File(nativeZipDir, "$filename.tmp")

        body.byteStream().use { input ->
            java.io.FileOutputStream(tmpFile).use { output ->
                val buffer = ByteArray(8192)
                var downloaded = 0L
                while (true) {
                    val read = input.read(buffer)
                    if (read == -1) break
                    output.write(buffer, 0, read)
                    downloaded += read
                    if (totalBytes > 0 && downloaded % (64 * 1024) < 8192) {
                        val pct = (downloaded * 100 / totalBytes).toInt()
                        _state.update {
                            it.copy(ytdlp = ToolState(
                                ToolInstallStatus.DOWNLOADING,
                                version = "$filename: $pct%",
                            ))
                        }
                    }
                }
            }
        }
        if (!tmpFile.exists() || tmpFile.length() == 0L) {
            tmpFile.delete()
            throw Exception("Failed to download $filename")
        }
        tmpFile.renameTo(target)
    }

    _state.update {
        it.copy(ytdlp = ToolState(ToolInstallStatus.DOWNLOADING, version = "Extracting..."))
    }
}


internal fun DownloaderRepository.prepareYoutubeDlUpdate() {
    if (!isNativeZipsDownloaded()) {
        android.util.Log.d("SGT-DL", "prepareYoutubeDlUpdate: re-downloading native zips")
        downloadNativeZips()
    }
    initialized = false
    try {
        val field = YoutubeDL::class.java.getDeclaredField("initialized")
        field.isAccessible = true
        field.setBoolean(YoutubeDL.getInstance(), false)
    } catch (_: Exception) {}
    try {
        val field = FFmpeg::class.java.getDeclaredField("initialized")
        field.isAccessible = true
        field.setBoolean(FFmpeg.getInstance(), false)
    } catch (_: Exception) {}
    ensureInit()
}

internal fun DownloaderRepository.updateYoutubeDlNightly(): Boolean {
    prepareYoutubeDlUpdate()
    val status = YoutubeDL.getInstance().updateYoutubeDL(
        context,
        com.yausername.youtubedl_android.YoutubeDL.UpdateChannel.NIGHTLY,
    )
    return status == com.yausername.youtubedl_android.YoutubeDL.UpdateStatus.DONE
}

/** Delete downloaded zip files after extraction — they're just wasting disk space. */
internal fun DownloaderRepository.cleanupNativeZips() {
    for (name in listOf("libpython.zip.so", "libffmpeg.zip.so")) {
        val f = File(nativeZipDir, name)
        if (f.exists()) {
            android.util.Log.d("SGT-DL", "cleanupNativeZips: deleting $name (${f.length() / 1024 / 1024} MB)")
            f.delete()
        }
    }
}

internal fun DownloaderRepository.removeNativePayload() = Unit

internal fun DownloaderRepository.nativePayloadSizeMb(): Double = dirSizeMb(nativeZipDir)

internal fun DownloaderRepository.calculateYtdlpSize(): String {
    // yt-dlp + Python extracted content (excludes FFmpeg subdir to avoid double-counting)
    val ytdlDir = File(context.noBackupFilesDir, "youtubedl-android")
    val ffmpegPkgPath = File(ytdlDir, "packages/ffmpeg").absolutePath
    var total = 0L
    if (ytdlDir.exists()) {
        ytdlDir.walkTopDown().forEach { f ->
            if (f.isFile && !f.absolutePath.startsWith(ffmpegPkgPath)) {
                total += f.length()
            }
        }
    }
    val sizeMb = total / (1024.0 * 1024.0)
    val version = try { YoutubeDL.getInstance().version(context) } catch (_: Exception) { null }
    return if (version != null) "$version (%.1f MB)".format(sizeMb) else "%.1f MB".format(sizeMb)
}

internal fun DownloaderRepository.calculateFfmpegSize(): String {
    val sizeMb = dirSizeMb(File(context.noBackupFilesDir, "youtubedl-android/packages/ffmpeg"))
    return "%.1f MB".format(sizeMb)
}

internal fun DownloaderRepository.dirSizeMb(dir: File): Double {
    if (!dir.exists()) return 0.0
    var total = 0L
    dir.walkTopDown().forEach { if (it.isFile) total += it.length() }
    return total / (1024.0 * 1024.0)
}

/** Total size of all deletable downloaded dependencies. */
