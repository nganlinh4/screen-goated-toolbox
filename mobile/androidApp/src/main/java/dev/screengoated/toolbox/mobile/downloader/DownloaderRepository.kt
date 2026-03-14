package dev.screengoated.toolbox.mobile.downloader

import android.content.Context
import android.os.Environment
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
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

class DownloaderRepository(
    private val context: Context,
    private val persistence: DownloaderPersistence,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private val _state = MutableStateFlow(DownloaderUiState(settings = persistence.load()))
    val state: StateFlow<DownloaderUiState> = _state.asStateFlow()

    private var analysisJob: Job? = null
    private var downloadJob: Job? = null
    private var nextSessionId = 2

    // ── Tool management ──

    private var initialized = false

    private fun ensureInit() {
        if (!initialized) {
            // log:"ensureInit: calling YoutubeDL.init()")
            YoutubeDL.getInstance().init(context)
            // log:"ensureInit: calling FFmpeg.init()")
            FFmpeg.getInstance().init(context)
            initialized = true
            // log:"ensureInit: done, initialized=true")
        } else {
            // log:"ensureInit: already initialized, skipping")
        }
    }

    private fun isAlreadyExtracted(): Boolean {
        val ytdlDir = File(context.noBackupFilesDir, "youtubedl-android")
        val ytdlpExists = File(ytdlDir, "yt-dlp").exists()
        val pythonExists = File(ytdlDir, "packages/python").exists()
        // log:"isAlreadyExtracted: dir=$ytdlDir ytdlp=$ytdlpExists python=$pythonExists")
        // List what's actually in the dir
        if (ytdlDir.exists()) {
            // log:"isAlreadyExtracted: contents=${ytdlDir.list()?.joinToString()}")
        }
        return ytdlpExists && pythonExists
    }

    fun checkTools() {
        // log:"checkTools: starting")
        scope.launch {
            withContext(Dispatchers.IO) {
                val ffmpegSize = calculateFfmpegSize()
                val ffmpegState = ToolState(ToolInstallStatus.INSTALLED, version = ffmpegSize)

                val extracted = isAlreadyExtracted()
                // log:"checkTools: extracted=$extracted")

                if (extracted) {
                    try {
                        ensureInit()
                    } catch (e: Exception) {
                        // log:"checkTools: ensureInit failed", e)
                    }
                    val ytdlpSize = calculateYtdlpSize()
                    // log:"checkTools: INSTALLED size=$ytdlpSize")
                    _state.update {
                        it.copy(
                            ytdlp = ToolState(ToolInstallStatus.INSTALLED, version = ytdlpSize),
                            ffmpeg = ffmpegState,
                        )
                    }
                } else {
                    // log:"checkTools: MISSING")
                    _state.update {
                        it.copy(
                            ytdlp = ToolState(ToolInstallStatus.MISSING),
                            ffmpeg = ffmpegState,
                        )
                    }
                }
            }
        }
    }

    fun installTools() {
        // log:"installTools: starting")
        scope.launch {
            _state.update { it.copy(ytdlp = ToolState(ToolInstallStatus.DOWNLOADING)) }
            withContext(Dispatchers.IO) {
                try {
                    initialized = false
                    // Force the library to re-extract by resetting its internal state
                    // The library uses a static 'initialized' field — we need to use reflection
                    try {
                        val field = YoutubeDL::class.java.getDeclaredField("initialized")
                        field.isAccessible = true
                        field.setBoolean(YoutubeDL.getInstance(), false)
                        // log:"installTools: reset library initialized via reflection")
                    } catch (e: Exception) {
                        // log:"installTools: reflection reset failed", e)
                    }
                    try {
                        val field = FFmpeg::class.java.getDeclaredField("initialized")
                        field.isAccessible = true
                        field.setBoolean(FFmpeg.getInstance(), false)
                    } catch (_: Exception) {}

                    ensureInit()
                    val extracted = isAlreadyExtracted()
                    // log:"installTools: after init, extracted=$extracted")
                    if (!extracted) {
                        _state.update {
                            it.copy(ytdlp = ToolState(ToolInstallStatus.ERROR, error = "Extraction failed — restart app"))
                        }
                        return@withContext
                    }
                    val ytdlpSize = calculateYtdlpSize()
                    // log:"installTools: INSTALLED size=$ytdlpSize")
                    _state.update {
                        it.copy(
                            ytdlp = ToolState(ToolInstallStatus.INSTALLED, version = ytdlpSize),
                        )
                    }
                } catch (e: Exception) {
                    // log:"installTools: FAILED", e)
                    _state.update {
                        it.copy(ytdlp = ToolState(ToolInstallStatus.ERROR, error = e.message))
                    }
                }
            }
        }
    }

    fun deleteTools() {
        // log:"deleteTools: starting")
        scope.launch {
            withContext(Dispatchers.IO) {
                initialized = false
                // Delete extracted files
                val dirs = listOf(
                    File(context.noBackupFilesDir, "youtubedl-android"),
                    File(context.filesDir, "youtubedl-android"),
                )
                for (dir in dirs) {
                    if (dir.exists()) {
                        try {
                            Runtime.getRuntime().exec(arrayOf("rm", "-rf", dir.absolutePath)).waitFor()
                        } catch (_: Exception) {
                            dir.deleteRecursively()
                        }
                    }
                }
                // Clear the library's SharedPreferences so init() will re-extract next time
                val prefNames = listOf("youtubedl-android", "com.yausername.youtubedl_android")
                for (name in prefNames) {
                    try {
                        context.getSharedPreferences(name, android.content.Context.MODE_PRIVATE)
                            .edit().clear().apply()
                    } catch (_: Exception) {}
                }
                // log:"deleteTools: files + prefs cleared")
            }
            _state.update {
                it.copy(
                    ytdlp = ToolState(ToolInstallStatus.MISSING),
                    ytdlpUpdate = UpdateStatus.IDLE,
                )
            }
        }
    }

    fun checkUpdates() {
        scope.launch {
            _state.update { it.copy(ytdlpUpdate = UpdateStatus.CHECKING) }
            withContext(Dispatchers.IO) {
                try {
                    ensureInit()
                    val status = YoutubeDL.getInstance().updateYoutubeDL(context, com.yausername.youtubedl_android.YoutubeDL.UpdateChannel.NIGHTLY)
                    val updated = status == com.yausername.youtubedl_android.YoutubeDL.UpdateStatus.DONE
                    _state.update {
                        it.copy(
                            ytdlpUpdate = if (updated) UpdateStatus.UPDATE_AVAILABLE else UpdateStatus.UP_TO_DATE,
                        )
                    }
                    if (updated) {
                        val ytdlpSize = calculateYtdlpSize()
                        _state.update {
                            it.copy(ytdlp = ToolState(ToolInstallStatus.INSTALLED, version = ytdlpSize))
                        }
                    }
                } catch (_: Exception) {
                    _state.update { it.copy(ytdlpUpdate = UpdateStatus.ERROR) }
                }
            }
        }
    }

    private fun calculateYtdlpSize(): String {
        val ytdlDir = File(context.noBackupFilesDir, "youtubedl-android")
        val sizeMb = dirSizeMb(ytdlDir)
        val version = try { YoutubeDL.getInstance().version(context) } catch (_: Exception) { null }
        return if (version != null) "$version (%.1f MB)".format(sizeMb) else "%.1f MB".format(sizeMb)
    }

    private fun calculateFfmpegSize(): String {
        val nativeLibDir = File(context.applicationInfo.nativeLibraryDir)
        val size = fileSizeMb(File(nativeLibDir, "libffmpeg.so")) +
            fileSizeMb(File(nativeLibDir, "libffprobe.so")) +
            dirSizeMb(File(context.noBackupFilesDir, "youtubedl-android/packages/ffmpeg"))
        return "Bundled (%.1f MB)".format(size)
    }

    private fun dirSizeMb(dir: File): Double {
        if (!dir.exists()) return 0.0
        var total = 0L
        dir.walkTopDown().forEach { if (it.isFile) total += it.length() }
        return total / (1024.0 * 1024.0)
    }

    private fun fileSizeMb(file: File): Double {
        return if (file.exists()) file.length() / (1024.0 * 1024.0) else 0.0
    }

    // ── Multi-tab ──

    fun addTab() {
        val currentType = _state.value.activeSession.downloadType
        val id = nextSessionId++
        _state.update {
            val sessions = it.sessions + DownloadSessionState(
                id = id,
                tabName = "Tab ${it.sessions.size + 1}",
                downloadType = currentType,
            )
            it.copy(sessions = sessions, activeTabIndex = sessions.lastIndex)
        }
    }

    fun closeTab(idx: Int) {
        _state.update {
            if (it.sessions.size <= 1) {
                val dt = it.sessions[0].downloadType
                it.copy(
                    sessions = listOf(DownloadSessionState(id = 1, tabName = "Tab 1", downloadType = dt)),
                    activeTabIndex = 0,
                )
            } else {
                val sessions = it.sessions.toMutableList().apply { removeAt(idx) }
                sessions.forEachIndexed { i, s -> sessions[i] = s.copy(tabName = "Tab ${i + 1}") }
                val newIdx = if (it.activeTabIndex >= sessions.size) sessions.lastIndex else it.activeTabIndex
                it.copy(sessions = sessions, activeTabIndex = newIdx)
            }
        }
    }

    fun switchTab(idx: Int) {
        _state.update { it.copy(activeTabIndex = idx.coerceIn(it.sessions.indices)) }
    }

    // ── URL & Analysis ──

    fun updateUrl(url: String) {
        val idx = _state.value.activeTabIndex
        updateSession(idx) {
            it.copy(inputUrl = url, analysisError = null, lastInputChangeMs = System.currentTimeMillis())
        }
        analysisJob?.cancel()
        if (url.isNotBlank()) {
            analysisJob = scope.launch {
                delay(800)
                analyzeUrl(idx, url)
            }
        }
    }

    private suspend fun analyzeUrl(sessionIdx: Int, url: String) {
        val current = _state.value.sessions.getOrNull(sessionIdx) ?: return
        if (url == current.lastUrlAnalyzed) return
        // Don't analyze if already downloading
        if (current.phase == DownloadPhase.DOWNLOADING || current.phase == DownloadPhase.FINISHED) return

        updateSession(sessionIdx) {
            it.copy(
                isAnalyzing = true,
                phase = DownloadPhase.ANALYZING,
                availableFormats = emptyList(),
                availableSubtitles = emptyList(),
                analysisError = null,
            )
        }

        withContext(Dispatchers.IO) {
            try {
                val request = YoutubeDLRequest(url)
                request.addOption("--dump-json")
                request.addOption("--no-download")
                request.addOption("--no-playlist")
                val response = YoutubeDL.getInstance().execute(request)
                val json = JSONObject(response.out)

                // Extract resolutions — only from formats with real video codec
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

                // Extract subtitles
                val subtitles = mutableListOf<String>()
                val subs = json.optJSONObject("subtitles")
                if (subs != null) {
                    val keys = subs.keys()
                    while (keys.hasNext()) subtitles.add(keys.next())
                    subtitles.sort()
                }

                updateSession(sessionIdx) {
                    it.copy(
                        isAnalyzing = false,
                        phase = DownloadPhase.IDLE,
                        availableFormats = resolutions,
                        availableSubtitles = subtitles,
                        lastUrlAnalyzed = url,
                    )
                }
            } catch (e: Exception) {
                updateSession(sessionIdx) {
                    it.copy(
                        isAnalyzing = false,
                        phase = DownloadPhase.IDLE,
                        analysisError = e.message ?: "Analysis failed",
                    )
                }
            }
        }
    }

    // ── Download config ──

    fun setDownloadType(type: DownloadType) {
        val idx = _state.value.activeTabIndex
        updateSession(idx) { it.copy(downloadType = type) }
    }

    fun setFormat(format: String?) {
        val idx = _state.value.activeTabIndex
        updateSession(idx) { it.copy(selectedFormat = format) }
    }

    fun setSubtitle(subtitle: String?) {
        val idx = _state.value.activeTabIndex
        updateSession(idx) { it.copy(selectedSubtitle = subtitle) }
    }

    fun toggleErrorLog() {
        val idx = _state.value.activeTabIndex
        updateSession(idx) { it.copy(showErrorLog = !it.showErrorLog) }
    }

    // ── Download ──

    fun startDownload() {
        val idx = _state.value.activeTabIndex
        val session = _state.value.activeSession
        if (session.inputUrl.isBlank()) return

        downloadJob?.cancel()
        updateSession(idx) {
            it.copy(
                phase = DownloadPhase.DOWNLOADING,
                progress = DownloadProgress(),
                logs = emptyList(),
                errorMessage = null,
            )
        }

        downloadJob = scope.launch {
            withContext(Dispatchers.IO) {
                try {
                    val result = executeDownload(idx, session)
                    updateSession(idx) {
                        it.copy(phase = DownloadPhase.FINISHED, finishedFilePath = result)
                    }
                } catch (e: Exception) {
                    // Auto-retry: update yt-dlp and try once more
                    try {
                        YoutubeDL.getInstance().updateYoutubeDL(context)
                        val result = executeDownload(idx, session)
                        updateSession(idx) {
                            it.copy(phase = DownloadPhase.FINISHED, finishedFilePath = result)
                        }
                    } catch (retryError: Exception) {
                        updateSession(idx) {
                            it.copy(
                                phase = DownloadPhase.ERROR,
                                errorMessage = retryError.message ?: "Download failed",
                            )
                        }
                    }
                }
            }
        }
    }

    private fun executeDownload(sessionIdx: Int, session: DownloadSessionState): String? {
        val settings = _state.value.settings
        val outputDir = getDownloadDir()
        outputDir.mkdirs()

        val request = YoutubeDLRequest(session.inputUrl)

        // ffmpeg location — native libs extracted from APK

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
                val fmt = session.selectedFormat
                if (fmt != null && fmt != "Best") {
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

        // Execute with progress callback
        var finalPath: String? = null
        val response = YoutubeDL.getInstance().execute(request) { progress, eta, line ->
            val fraction = (progress / 100f).coerceIn(0f, 1f)
            val msg = buildProgressMessage(line, fraction)
            updateSession(sessionIdx) {
                it.copy(progress = DownloadProgress(fraction, msg))
            }
            // Parse final filename from progress lines
            if (line.isNotBlank()) {
                updateSession(sessionIdx) { it.copy(logs = it.logs + line) }
                val parsed = parseFilePath(line)
                if (parsed != null) finalPath = parsed
            }
        }

        // Also check response output for final path
        response.out?.lines()?.forEach { line ->
            val parsed = parseFilePath(line)
            if (parsed != null) finalPath = parsed
        }

        if (response.exitCode != 0) {
            throw Exception("Exit code: ${response.exitCode}")
        }

        return finalPath
    }

    fun cancelDownload() {
        downloadJob?.cancel()
        val idx = _state.value.activeTabIndex
        updateSession(idx) { it.copy(phase = DownloadPhase.IDLE) }
    }

    fun resetSession() {
        val idx = _state.value.activeTabIndex
        updateSession(idx) {
            DownloadSessionState(id = it.id, tabName = it.tabName, downloadType = it.downloadType)
        }
    }

    // ── Settings ──

    fun updateSettings(transform: (DownloaderSettings) -> DownloaderSettings) {
        _state.update {
            val newSettings = transform(it.settings)
            persistence.save(newSettings)
            it.copy(settings = newSettings)
        }
    }

    fun setDownloadPath(path: String?) {
        updateSettings { it.copy(customDownloadPath = path) }
    }

    fun getDownloadDir(): File {
        val custom = _state.value.settings.customDownloadPath
        return if (custom != null) {
            File(custom)
        } else {
            // Use app-specific external dir — accessible via file manager, survives app clear cache
            val dir = File(context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS), "SGT")
            dir.mkdirs()
            dir
        }
    }

    // ── Helpers ──

    private fun updateSession(idx: Int, transform: (DownloadSessionState) -> DownloadSessionState) {
        _state.update {
            val sessions = it.sessions.toMutableList()
            if (idx in sessions.indices) {
                sessions[idx] = transform(sessions[idx])
            }
            it.copy(sessions = sessions)
        }
    }

    private fun buildProgressMessage(line: String, fraction: Float): String {
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

    private fun parseFilePath(line: String): String? {
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
}
