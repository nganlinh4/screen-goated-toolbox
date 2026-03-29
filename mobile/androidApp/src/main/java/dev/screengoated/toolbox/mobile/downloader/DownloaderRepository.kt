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

    private val nativeZipDir: File = context.getDir("ytdl_native", Context.MODE_PRIVATE)

    private fun isNativeZipsDownloaded(): Boolean {
        return File(nativeZipDir, "libpython.zip.so").exists() &&
            File(nativeZipDir, "libffmpeg.zip.so").exists()
    }

    private fun ensureInit() {
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

    private fun isAlreadyExtracted(): Boolean {
        val ytdlDir = File(context.noBackupFilesDir, "youtubedl-android")
        val ytdlpExists = File(ytdlDir, "yt-dlp").exists()
        val pythonExists = File(ytdlDir, "packages/python").exists()
        return ytdlpExists && pythonExists
    }

    fun checkTools() {
        scope.launch {
            withContext(Dispatchers.IO) {
                val hasNativeZips = isNativeZipsDownloaded()
                val alreadyExtracted = isAlreadyExtracted()
                android.util.Log.d("SGT-DL", "checkTools: hasNativeZips=$hasNativeZips alreadyExtracted=$alreadyExtracted")
                // Tools are ready if extracted (zips may have been cleaned up after extraction)
                val ffmpegPkgDir = File(context.noBackupFilesDir, "youtubedl-android/packages/ffmpeg")
                val ffmpegReady = alreadyExtracted && ffmpegPkgDir.exists()
                val ffmpegState = if (ffmpegReady) {
                    ToolState(ToolInstallStatus.INSTALLED, version = calculateFfmpegSize())
                } else {
                    ToolState(ToolInstallStatus.MISSING)
                }

                if (alreadyExtracted) {
                    try {
                        ensureInit()
                    } catch (_: Exception) {}
                    // Clean up stale zips from previous installs
                    cleanupNativeZips()
                    _state.update {
                        it.copy(
                            ytdlp = ToolState(ToolInstallStatus.INSTALLED, version = calculateYtdlpSize()),
                            ffmpeg = ffmpegState,
                        )
                    }
                } else {
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
        scope.launch {
            _state.update {
                it.copy(
                    ytdlp = ToolState(ToolInstallStatus.DOWNLOADING),
                    ffmpeg = ToolState(ToolInstallStatus.DOWNLOADING),
                )
            }
            withContext(Dispatchers.IO) {
                try {
                    android.util.Log.d("SGT-DL", "installTools: starting, hasZips=${isNativeZipsDownloaded()}")
                    if (!isNativeZipsDownloaded()) downloadNativeZips()
                    android.util.Log.d("SGT-DL", "installTools: zips ready, resetting init")
                    initialized = false
                    // Force the library to re-extract by resetting its internal state
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
                    // Clear version prefs to force fresh extraction from new zip source
                    try {
                        context.getSharedPreferences("youtubedl-android", Context.MODE_PRIVATE)
                            .edit().clear().apply()
                    } catch (_: Exception) {}
                    // Delete old extracted packages so init re-extracts
                    val packagesDir = File(context.noBackupFilesDir, "youtubedl-android/packages")
                    packagesDir.deleteRecursively()

                    ensureInit()
                    // Clean up downloaded zips after successful extraction — they're no longer needed
                    cleanupNativeZips()
                    val extracted = isAlreadyExtracted()
                    if (!extracted) {
                        _state.update {
                            it.copy(ytdlp = ToolState(ToolInstallStatus.ERROR, error = "Extraction failed — restart app"))
                        }
                        return@withContext
                    }
                    _state.update {
                        it.copy(
                            ytdlp = ToolState(ToolInstallStatus.INSTALLED, version = calculateYtdlpSize()),
                            ffmpeg = ToolState(ToolInstallStatus.INSTALLED, version = calculateFfmpegSize()),
                        )
                    }
                } catch (e: Exception) {
                    _state.update {
                        it.copy(ytdlp = ToolState(ToolInstallStatus.ERROR, error = e.message))
                    }
                }
            }
        }
    }

    private fun downloadNativeZips() {
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
            val body = response.body ?: throw Exception("Empty body for $filename")
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

    fun deleteTools() {
        scope.launch {
            withContext(Dispatchers.IO) {
                initialized = false
                // Reset library internal state
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
                // Delete extracted files and downloaded native zips
                val dirs = listOf(
                    File(context.noBackupFilesDir, "youtubedl-android"),
                    File(context.filesDir, "youtubedl-android"),
                    nativeZipDir,
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
                    ffmpeg = ToolState(ToolInstallStatus.MISSING),
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
                        _state.update {
                            it.copy(ytdlp = ToolState(ToolInstallStatus.INSTALLED, version = calculateYtdlpSize()))
                        }
                    }
                } catch (_: Exception) {
                    _state.update { it.copy(ytdlpUpdate = UpdateStatus.ERROR) }
                }
            }
        }
    }

    /** Delete downloaded zip files after extraction — they're just wasting disk space. */
    private fun cleanupNativeZips() {
        for (name in listOf("libpython.zip.so", "libffmpeg.zip.so")) {
            val f = File(nativeZipDir, name)
            if (f.exists()) {
                android.util.Log.d("SGT-DL", "cleanupNativeZips: deleting $name (${f.length() / 1024 / 1024} MB)")
                f.delete()
            }
        }
    }

    private fun calculateYtdlpSize(): String {
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

    private fun calculateFfmpegSize(): String {
        val sizeMb = dirSizeMb(File(context.noBackupFilesDir, "youtubedl-android/packages/ffmpeg"))
        return "%.1f MB".format(sizeMb)
    }

    private fun dirSizeMb(dir: File): Double {
        if (!dir.exists()) return 0.0
        var total = 0L
        dir.walkTopDown().forEach { if (it.isFile) total += it.length() }
        return total / (1024.0 * 1024.0)
    }

    /** Total size of all deletable downloaded dependencies. */
    fun calculateTotalDepsSize(): String {
        val dir1 = File(context.noBackupFilesDir, "youtubedl-android")
        val dir2 = File(context.filesDir, "youtubedl-android")
        val dir3 = nativeZipDir
        val s1 = dirSizeMb(dir1)
        val s2 = dirSizeMb(dir2)
        val s3 = dirSizeMb(dir3)
        android.util.Log.d("SGT-DL", "totalDepsSize: noBackup/ytdl=%.1f MB, files/ytdl=%.1f MB, zips=%.1f MB".format(s1, s2, s3))
        return "%.0f MB".format(s1 + s2 + s3)
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
        val oldFormats = _state.value.activeSession.availableFormats.size
        updateSession(idx) {
            it.copy(
                inputUrl = url,
                analysisError = null,
                lastInputChangeMs = System.currentTimeMillis(),
                availableFormats = emptyList(),
                availableSubtitles = emptyList(),
                lastUrlAnalyzed = "",
            )
        }
        android.util.Log.d("SGT-DL", "updateUrl: cancelling old analysis, formats=$oldFormats")
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

        android.util.Log.d("SGT-DL", "startDownload: formats=${session.availableFormats.size} phase=${session.phase}")
        analysisJob?.cancel()
        downloadJob?.cancel()
        updateSession(idx) {
            android.util.Log.d("SGT-DL", "startDownload: updateSession DOWNLOADING, keeping formats=${it.availableFormats.size}")
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

        // Execute with progress callback and process ID for cancellation
        var finalPath: String? = null
        val response = YoutubeDL.getInstance().execute(request, "download_$sessionIdx") { progress, eta, line ->
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
        val idx = _state.value.activeTabIndex
        YoutubeDL.getInstance().destroyProcessById("download_$idx")
        downloadJob?.cancel()
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

    companion object {
        private const val GH_RELEASE =
            "https://github.com/nganlinh4/youtubedl-android/releases/download/v0.18.1-sgt"
        private val NATIVE_ZIP_FILES = listOf(
            "libpython.zip.so" to "$GH_RELEASE/libpython.zip.so",
            "libffmpeg.zip.so" to "$GH_RELEASE/libffmpeg.zip.so",
        )
    }
}
