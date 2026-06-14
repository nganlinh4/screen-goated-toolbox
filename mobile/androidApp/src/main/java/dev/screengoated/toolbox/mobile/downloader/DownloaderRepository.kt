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

class DownloaderRepository(
    internal val context: Context,
    internal val persistence: DownloaderPersistence,
) {
    internal val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    internal val _state = MutableStateFlow(DownloaderUiState(settings = persistence.load()))
    val state: StateFlow<DownloaderUiState> = _state.asStateFlow()

    internal var analysisJob: Job? = null
    internal var downloadJob: Job? = null
    internal val cancelledDownloadSessionIds = ConcurrentHashMap.newKeySet<Int>()
    internal val activeDownloadPaths = ConcurrentHashMap<Int, String>()
    internal var nextSessionId = 2

    // ── Tool management ──

    internal var initialized = false

    internal val nativeZipDir: File = context.getDir("ytdl_native", Context.MODE_PRIVATE)

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
                            .edit { clear() }
                    } catch (_: Exception) {}
                    // Delete old extracted packages so init re-extracts
                    val packagesDir = File(context.noBackupFilesDir, "youtubedl-android/packages")
                    packagesDir.deleteRecursively()

                    ensureInit()
                    _state.update {
                        it.copy(
                            ytdlp = ToolState(ToolInstallStatus.DOWNLOADING, version = "Updating yt-dlp..."),
                            ytdlpUpdate = UpdateStatus.CHECKING,
                        )
                    }
                    val updated = updateYoutubeDlNightly()
                    _state.update {
                        it.copy(
                            ytdlpUpdate = if (updated) UpdateStatus.UPDATE_AVAILABLE else UpdateStatus.UP_TO_DATE,
                        )
                    }
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
                            .edit { clear() }
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
                    val updated = updateYoutubeDlNightly()
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
                } catch (e: Exception) {
                    android.util.Log.e("SGT-DL", "checkUpdates failed", e)
                    _state.update { it.copy(ytdlpUpdate = UpdateStatus.ERROR) }
                }
            }
        }
    }

    /**
     * yt-dlp updates require the Python zip payload to exist so YoutubeDL.init()
     * can succeed before the updater runs. We delete those zips after install to
     * save space, so the update path must re-download them on demand.
     */
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

    fun setDownloadType(type: DownloadType) {
        val idx = _state.value.activeTabIndex
        updateSession(idx) { it.copy(downloadType = type) }
    }

    fun setFormat(format: String?) {
        val idx = _state.value.activeTabIndex
        updateSession(idx) { it.copy(selectedFormat = format) }
        updateSettings { it.copy(lastVideoFormat = format) }
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
        val processId = "download_${session.id}"

        android.util.Log.d("SGT-DL", "startDownload: formats=${session.availableFormats.size} phase=${session.phase}")
        analysisJob?.cancel()
        downloadJob?.cancel()
        cancelledDownloadSessionIds.remove(session.id)
        activeDownloadPaths.remove(session.id)
        updateSession(idx) {
            android.util.Log.d("SGT-DL", "startDownload: updateSession DOWNLOADING, keeping formats=${it.availableFormats.size}")
            it.copy(
                phase = DownloadPhase.DOWNLOADING,
                progress = DownloadProgress(),
                logs = emptyList(),
                errorMessage = null,
                processId = processId,
                finishedFilePath = null,
                finishedFileUri = null,
            )
        }

        downloadJob = scope.launch {
            withContext(Dispatchers.IO) {
                try {
                    val result = executeDownload(idx, session, processId)
                    activeDownloadPaths.remove(session.id)
                    updateSession(idx) {
                        it.copy(
                            phase = DownloadPhase.FINISHED,
                            finishedFilePath = result.filePath,
                            finishedFileUri = result.contentUri,
                            processId = null,
                        )
                    }
                } catch (e: Exception) {
                    if (isDownloadCancelled(session.id, e)) {
                        cleanupCancelledDownload(session.id)
                        updateSession(idx) {
                            it.copy(phase = DownloadPhase.IDLE, progress = DownloadProgress(), processId = null)
                        }
                        cancelledDownloadSessionIds.remove(session.id)
                        return@withContext
                    }

                    // Auto-retry: update yt-dlp and try once more
                    try {
                        _state.update { it.copy(ytdlpUpdate = UpdateStatus.CHECKING) }
                        val updated = updateYoutubeDlNightly()
                        _state.update {
                            it.copy(ytdlpUpdate = if (updated) UpdateStatus.UPDATE_AVAILABLE else UpdateStatus.UP_TO_DATE)
                        }
                        if (isDownloadCancelled(session.id)) throw CancellationException("Download cancelled")
                        val result = executeDownload(idx, session, processId)
                        activeDownloadPaths.remove(session.id)
                        updateSession(idx) {
                            it.copy(
                                phase = DownloadPhase.FINISHED,
                                finishedFilePath = result.filePath,
                                finishedFileUri = result.contentUri,
                                processId = null,
                            )
                        }
                    } catch (retryError: Exception) {
                        if (isDownloadCancelled(session.id, retryError)) {
                            cleanupCancelledDownload(session.id)
                            updateSession(idx) {
                                it.copy(phase = DownloadPhase.IDLE, progress = DownloadProgress(), processId = null)
                            }
                            cancelledDownloadSessionIds.remove(session.id)
                            return@withContext
                        }

                        activeDownloadPaths.remove(session.id)
                        updateSession(idx) {
                            it.copy(
                                phase = DownloadPhase.ERROR,
                                errorMessage = retryError.message ?: "Download failed",
                                processId = null,
                            )
                        }
                    }
                }
            }
        }
    }

    fun cancelDownload() {
        val idx = _state.value.activeTabIndex
        val session = _state.value.activeSession
        cancelledDownloadSessionIds.add(session.id)
        YoutubeDL.getInstance().destroyProcessById(session.processId ?: "download_${session.id}")
        YoutubeDL.getInstance().destroyProcessById("download_$idx")
        cleanupCancelledDownload(session.id)
        downloadJob?.cancel()
        updateSession(idx) { it.copy(phase = DownloadPhase.IDLE, progress = DownloadProgress(), processId = null) }
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
            val dir = File(
                Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS),
                "SGT",
            )
            dir.mkdirs()
            dir
        }
    }

    // ── Helpers ──

    internal fun updateSession(idx: Int, transform: (DownloadSessionState) -> DownloadSessionState) {
        _state.update {
            val sessions = it.sessions.toMutableList()
            if (idx in sessions.indices) {
                sessions[idx] = transform(sessions[idx])
            }
            it.copy(sessions = sessions)
        }
    }

}
