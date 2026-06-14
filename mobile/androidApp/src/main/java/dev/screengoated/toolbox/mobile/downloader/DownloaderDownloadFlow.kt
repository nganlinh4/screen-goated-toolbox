package dev.screengoated.toolbox.mobile.downloader

import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Environment
import android.provider.MediaStore
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

// URL analysis + download execution helpers extracted from DownloaderRepository.
internal data class DownloadExecutionResult(
    val filePath: String?,
    val contentUri: String? = null,
)

internal suspend fun DownloaderRepository.analyzeUrl(sessionIdx: Int, url: String) {
    val current = _state.value.sessions.getOrNull(sessionIdx) ?: return
    if (url == current.lastUrlAnalyzed) {
        android.util.Log.d("SGT-DL", "analyzeUrl: SKIP same url already analyzed")
        return
    }
    if (current.phase == DownloadPhase.DOWNLOADING) {
        android.util.Log.d("SGT-DL", "analyzeUrl: SKIP phase=DOWNLOADING")
        return
    }

    android.util.Log.d("SGT-DL", "analyzeUrl: START phase=${current.phase} formats=${current.availableFormats.size}")
    updateSession(sessionIdx) {
        val keepPhase = it.phase == DownloadPhase.DOWNLOADING
        android.util.Log.d("SGT-DL", "analyzeUrl: updateSession START keepPhase=$keepPhase phase=${it.phase} formats=${it.availableFormats.size}")
        it.copy(
            isAnalyzing = true,
            phase = if (keepPhase) it.phase else DownloadPhase.ANALYZING,
            analysisError = if (keepPhase) it.analysisError else null,
        )
    }

    withContext(Dispatchers.IO) {
        try {
            android.util.Log.d("SGT-DL", "analyzeUrl: executing yt-dlp --dump-json")
            val request = YoutubeDLRequest(url)
            request.addOption("--dump-json")
            request.addOption("--no-download")
            request.addOption("--no-playlist")
            val response = YoutubeDL.getInstance().execute(request)
            val json = JSONObject(response.out)

            val heights = mutableSetOf<Int>()
            val formats = json.optJSONArray("formats")
            if (formats != null) {
                for (i in 0 until formats.length()) {
                    val fmt = formats.getJSONObject(i)
                    val vcodec = fmt.optString("vcodec", "none")
                    if (vcodec == "none" || vcodec == "images") continue
                    val h = fmt.optInt("height", 0)
                    if (h > 0) heights.add(h)
                }
            }
            val resolutions = heights.sortedDescending().map { "${it}p" }

            val subtitles = mutableListOf<String>()
            val subs = json.optJSONObject("subtitles")
            if (subs != null) {
                val keys = subs.keys()
                while (keys.hasNext()) subtitles.add(keys.next())
                subtitles.sort()
            }

            android.util.Log.d("SGT-DL", "analyzeUrl: DONE resolutions=$resolutions")
            updateSession(sessionIdx) {
                val keepPhase = it.phase == DownloadPhase.DOWNLOADING
                val newFormats = resolutions.ifEmpty { it.availableFormats }
                android.util.Log.d("SGT-DL", "analyzeUrl: updateSession DONE keepPhase=$keepPhase phase=${it.phase} oldFormats=${it.availableFormats.size} newFormats=${newFormats.size}")
                it.copy(
                    isAnalyzing = false,
                    phase = if (keepPhase) it.phase else DownloadPhase.IDLE,
                    availableFormats = newFormats,
                    availableSubtitles = subtitles.ifEmpty { it.availableSubtitles },
                    lastUrlAnalyzed = url,
                )
            }
        } catch (e: Exception) {
            android.util.Log.d("SGT-DL", "analyzeUrl: ERROR ${e.message}")
            updateSession(sessionIdx) {
                val keepPhase = it.phase == DownloadPhase.DOWNLOADING
                android.util.Log.d("SGT-DL", "analyzeUrl: updateSession ERROR keepPhase=$keepPhase phase=${it.phase} formats=${it.availableFormats.size}")
                it.copy(
                    isAnalyzing = false,
                    phase = if (keepPhase) it.phase else DownloadPhase.IDLE,
                    analysisError = if (keepPhase) it.analysisError else (e.message ?: "Analysis failed"),
                )
            }
        }
    }
}

// ── Download config ──


internal fun DownloaderRepository.executeDownload(
    sessionIdx: Int,
    session: DownloadSessionState,
    processId: String,
): DownloadExecutionResult {
    val settings = _state.value.settings
    val usesPublicDefault = settings.customDownloadPath == null
    val outputDir = if (usesPublicDefault) getStagingDownloadDir() else getDownloadDir()
    outputDir.mkdirs()

    val request = YoutubeDLRequest(session.inputUrl)

    // Common args (matching Windows run.rs)
    request.addOption("--encoding", "utf-8")
    request.addOption("--newline")
    request.addOption("--force-overwrites")

    // Playlist
    if (settings.usePlaylist) request.addOption("--yes-playlist")
    else request.addOption("--no-playlist")

    // Metadata
    if (settings.useMetadata) {
        request.addOption("--embed-metadata")
        request.addOption("--embed-chapters")
        request.addOption("--embed-thumbnail")
    }

    // SponsorBlock
    if (settings.useSponsorBlock) {
        request.addOption("--sponsorblock-remove", "all")
    }

    // Subtitles
    if (settings.useSubtitles) {
        request.addOption("--write-subs")
        val lang = session.selectedSubtitle ?: settings.selectedSubtitle ?: "en.*,vi.*,ko.*"
        request.addOption("--sub-langs", lang)
        request.addOption("--embed-subs")
    }

    // Format
    when (session.downloadType) {
        DownloadType.VIDEO -> {
            val fmt = session.selectedFormat ?: settings.lastVideoFormat
            if (fmt != null) {
                val height = fmt.removeSuffix("p")
                request.addOption("-f", "bestvideo[height<=$height]+bestaudio/best[height<=$height]")
            } else {
                request.addOption("-f", "bestvideo+bestaudio/best")
            }
            request.addOption("--merge-output-format", "mp4")
        }
        DownloadType.AUDIO -> {
            request.addOption("-x")
            request.addOption("--audio-format", "mp3")
            request.addOption("--audio-quality", 0)
        }
    }

    // Output path
    request.addOption("-o", File(outputDir, "%(title)s.%(ext)s").absolutePath)

    // Execute with progress callback and process ID for cancellation
    var finalPath: String? = null
    val response = YoutubeDL.getInstance().execute(request, processId) { progress, eta, line ->
        if (isDownloadCancelled(session.id)) throw CancellationException("Download cancelled")
        val fraction = (progress / 100f).coerceIn(0f, 1f)
        val msg = buildProgressMessage(line, fraction)
        updateSession(sessionIdx) {
            it.copy(progress = DownloadProgress(fraction, msg))
        }
        // Parse final filename from progress lines
        if (line.isNotBlank()) {
            updateSession(sessionIdx) { it.copy(logs = it.logs + line) }
            val parsed = parseFilePath(line)
            if (parsed != null) {
                finalPath = parsed
                activeDownloadPaths[session.id] = parsed
            }
        }
    }

    // Also check response output for final path
    response.out?.lines()?.forEach { line ->
        val parsed = parseFilePath(line)
        if (parsed != null) {
            finalPath = parsed
            activeDownloadPaths[session.id] = parsed
        }
    }

    if (isDownloadCancelled(session.id)) throw CancellationException("Download cancelled")

    if (response.exitCode != 0) {
        throw Exception("Exit code: ${response.exitCode}")
    }

    if (finalPath == null) return DownloadExecutionResult(null)
    if (!usesPublicDefault) return DownloadExecutionResult(finalPath)
    return publishToPublicDownloads(File(finalPath))
}

internal fun DownloaderRepository.getStagingDownloadDir(): File {
    return File(context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS), "SGT")
}

internal fun DownloaderRepository.publishToPublicDownloads(source: File): DownloadExecutionResult {
    if (!source.isFile) return DownloadExecutionResult(source.absolutePath)

    val relativePath = "${Environment.DIRECTORY_DOWNLOADS}/SGT/"
    val resolver = context.contentResolver
    val collection = MediaStore.Downloads.EXTERNAL_CONTENT_URI
    deleteExistingPublicDownload(source.name, relativePath)

    val values = ContentValues().apply {
        put(MediaStore.MediaColumns.DISPLAY_NAME, source.name)
        put(MediaStore.MediaColumns.MIME_TYPE, source.guessMimeType())
        put(MediaStore.MediaColumns.RELATIVE_PATH, relativePath)
        put(MediaStore.MediaColumns.IS_PENDING, 1)
    }

    var uri: Uri? = null
    try {
        uri = resolver.insert(collection, values) ?: error("Could not create Downloads/SGT entry")
        resolver.openOutputStream(uri)?.use { output ->
            source.inputStream().use { input -> input.copyTo(output) }
        } ?: error("Could not open Downloads/SGT output")
        resolver.update(
            uri,
            ContentValues().apply { put(MediaStore.MediaColumns.IS_PENDING, 0) },
            null,
            null,
        )
        source.delete()
        val publicPath = File(
            Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS),
            "SGT/${source.name}",
        ).absolutePath
        return DownloadExecutionResult(publicPath, uri.toString())
    } catch (e: Exception) {
        if (uri != null) {
            runCatching { resolver.delete(uri, null, null) }
        }
        throw e
    }
}

private fun DownloaderRepository.deleteExistingPublicDownload(displayName: String, relativePath: String) {
    val projection = arrayOf(MediaStore.MediaColumns._ID)
    val selection = "${MediaStore.MediaColumns.DISPLAY_NAME}=? AND ${MediaStore.MediaColumns.RELATIVE_PATH}=?"
    val args = arrayOf(displayName, relativePath)
    runCatching {
        context.contentResolver.query(
            MediaStore.Downloads.EXTERNAL_CONTENT_URI,
            projection,
            selection,
            args,
            null,
        )?.use { cursor ->
            val idColumn = cursor.getColumnIndexOrThrow(MediaStore.MediaColumns._ID)
            while (cursor.moveToNext()) {
                val uri = ContentUris.withAppendedId(
                    MediaStore.Downloads.EXTERNAL_CONTENT_URI,
                    cursor.getLong(idColumn),
                )
                runCatching { context.contentResolver.delete(uri, null, null) }
            }
        }
    }
}

private fun File.guessMimeType(): String {
    return when (extension.lowercase()) {
        "mp4" -> "video/mp4"
        "mkv" -> "video/x-matroska"
        "webm" -> "video/webm"
        "mp3" -> "audio/mpeg"
        "m4a" -> "audio/mp4"
        "opus" -> "audio/opus"
        "ogg" -> "audio/ogg"
        else -> "application/octet-stream"
    }
}


internal fun DownloaderRepository.isDownloadCancelled(sessionId: Int, error: Throwable? = null): Boolean {
    return error is CancellationException || cancelledDownloadSessionIds.contains(sessionId)
}

internal fun DownloaderRepository.cleanupCancelledDownload(sessionId: Int) {
    val path = activeDownloadPaths.remove(sessionId) ?: return
    val target = File(path)
    cleanupCandidates(target).forEach { candidate ->
        if (candidate.isFile) {
            runCatching {
                if (candidate.delete()) {
                    android.util.Log.d("SGT-DL", "Removed cancelled artifact: ${candidate.absolutePath}")
                }
            }
        }
    }
    if (target.isFile && target.length() == 0L) {
        runCatching {
            if (target.delete()) {
                android.util.Log.d("SGT-DL", "Removed zero-byte cancelled output: ${target.absolutePath}")
            }
        }
    }
}

internal fun DownloaderRepository.cleanupCandidates(target: File): List<File> {
    val parent = target.parentFile ?: return emptyList()
    val name = target.name
    val extension = target.extension.takeIf { it.isNotBlank() }
    val candidates = mutableListOf(
        File(parent, "$name.part"),
        File(parent, "$name.ytdl"),
        File(parent, "$name.tmp"),
        File(parent, "$name.temp"),
    )
    if (extension != null) {
        val base = target.name.removeSuffix(".$extension")
        candidates += File(parent, "$base.$extension.part")
        candidates += File(parent, "$base.$extension.ytdl")
    }
    return candidates.distinctBy { it.absolutePath }
}


internal fun DownloaderRepository.buildProgressMessage(line: String, fraction: Float): String {
    if (line.isBlank() || !line.contains("[download]") || !line.contains("%")) {
        return "${(fraction * 100).toInt()}%"
    }
    val parts = line.split("\\s+".toRegex())
    var total: String? = null
    var speed: String? = null
    var eta: String? = null
    for ((i, part) in parts.withIndex()) {
        if (part == "of" && i + 1 < parts.size) {
            val v = parts[i + 1]; if (v != "Unknown" && v != "N/A") total = v
        } else if (part == "at" && i + 1 < parts.size) {
            val v = parts[i + 1]; if (v != "Unknown" && v != "N/A") speed = v
        } else if (part == "ETA" && i + 1 < parts.size) {
            val v = parts[i + 1]; if (v != "Unknown" && v != "N/A") eta = v
        }
    }
    return buildString {
        append("${(fraction * 100).toInt()}%")
        if (total != null) append(" of $total")
        if (speed != null) append(" at $speed")
        if (eta != null) append(" ETA $eta")
    }
}

internal fun DownloaderRepository.parseFilePath(line: String): String? {
    if (line.contains("Merging formats into \"")) {
        val start = line.indexOf("Merging formats into \"") + "Merging formats into \"".length
        return line.substring(start).trimEnd().trimEnd('"')
    }
    if (line.contains("[ExtractAudio] Destination: ")) {
        val start = line.indexOf("[ExtractAudio] Destination: ") + "[ExtractAudio] Destination: ".length
        return line.substring(start).trim()
    }
    if (line.contains("Destination: ")) {
        val start = line.indexOf("Destination: ") + "Destination: ".length
        val path = line.substring(start).trim()
        if (!path.endsWith(".vtt") && !path.endsWith(".srt") &&
            !path.endsWith(".ass") && !path.endsWith(".lrc")
        ) return path
    }
    if (line.contains(" has already been downloaded")) {
        val prefix = "[download] "
        val suffix = " has already been downloaded"
        val s = if (line.contains(prefix)) line.indexOf(prefix) + prefix.length else 0
        val e = line.indexOf(suffix)
        if (s < e) {
            val path = line.substring(s, e).trim()
            if (!path.endsWith(".vtt") && !path.endsWith(".srt") &&
                !path.endsWith(".ass") && !path.endsWith(".lrc")
            ) return path
        }
    }
    return null
}
