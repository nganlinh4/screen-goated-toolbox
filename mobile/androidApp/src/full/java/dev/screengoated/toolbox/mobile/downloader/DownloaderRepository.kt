package dev.screengoated.toolbox.mobile.downloader

import android.content.Context
import android.content.SharedPreferences
import android.os.Environment
import dev.screengoated.toolbox.mobile.service.nativelibs.RuntimeLeaseRegistry
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
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.Semaphore
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import okhttp3.OkHttpClient

class DownloaderRepository internal constructor(
    internal val context: Context,
    internal val persistence: DownloaderPersistence,
    runtimeDelivery: DownloaderRuntimeDelivery? = loadDownloaderRuntimeDelivery(context),
    internal val upstreamYtDlp: UpstreamYtDlp = UpstreamYtDlp(OkHttpClient()) {
        context.assets.open("downloader-runtime/yt-dlp-release-key.asc")
    },
) {
    internal val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private val initialSettings = persistence.load()
    internal val _state = MutableStateFlow(DownloaderUiState(
        settings = initialSettings,
        sessions = listOf(newDownloadSession(1, "Tab 1", initialSettings.downloadType, initialSettings.lastVideoFormat)),
    ))
    val state: StateFlow<DownloaderUiState> = _state.asStateFlow()

    internal val analysisJobs = mutableMapOf<Int, Job>()
    internal val analysisControls = mutableMapOf<Int, AnalysisControl>()
    internal val analysisPermits = Semaphore(2)
    internal var downloadJob: Job? = null
    internal val cancelledDownloadSessionIds = ConcurrentHashMap.newKeySet<Int>()
    internal val activeDownloadPaths = ConcurrentHashMap<Int, String>()
    internal var nextSessionId = 2

    // ── Tool management ──

    internal val runtimeInstaller = runtimeDelivery?.let {
        DownloaderRuntimeInstaller(context, it, OkHttpClient())
    }
    internal val ytDlpStore = runtimeDelivery?.let { delivery ->
        delivery.ytDlpStore(
            context.noBackupFilesDir.resolve("downloader-yt-dlp"),
            requireNotNull(runtimeInstaller).ytdlpFile,
        )
    }
    internal val runtimeUpdateMutex = Mutex()
    internal val runtimeRemovalPreferences: SharedPreferences = context.getSharedPreferences(
        "downloader_runtime_state",
        Context.MODE_PRIVATE,
    )
    internal val runtimeLeases = RuntimeLeaseRegistry<DownloaderRuntimeKey> {
        finishDownloaderRemoval()
    }
    internal val processHost = runtimeInstaller?.let { installer ->
        DownloaderProcessHost(context, installer, ytDlpStore) { acquireDownloaderRuntimeLease() }
    }
    internal var installJob: Job? = null
    internal var ytDlpUpdateJob: Job? = null

    init {
        if (runtimeRemovalPreferences.getBoolean(REMOVAL_PENDING_KEY, false)) {
            runtimeLeases.requestRemoval(DownloaderRuntimeKey.RUNTIME)
        }
    }

    fun checkTools() = refreshDownloaderTools()
    fun checkYtDlpForUpdates() = startUpstreamYtDlpUpdate(manual = true)

    fun installTools() = startDownloaderInstall()

    fun deleteTools() = requestDownloaderRemoval()

    fun calculateTotalDepsSize(): String = calculateDownloaderTotalSize()

    // ── Multi-tab ──

    fun addTab() {
        val currentType = _state.value.activeSession.downloadType
        val id = allocateSessionId()
        _state.update {
            val sessions = it.sessions + newDownloadSession(id, "Tab ${it.sessions.size + 1}", currentType, it.settings.lastVideoFormat)
            it.copy(sessions = sessions, activeTabIndex = sessions.lastIndex)
        }
    }

    fun closeTab(idx: Int) {
        val closing = _state.value.sessions.getOrNull(idx) ?: return
        retireAnalysis(closing.id)
        val replacementId = if (_state.value.sessions.size <= 1) allocateSessionId() else 0
        _state.update {
            if (it.sessions.size <= 1) {
                val dt = it.sessions[0].downloadType
                it.copy(
                    sessions = listOf(newDownloadSession(replacementId, "Tab 1", dt, it.settings.lastVideoFormat)),
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

    private fun allocateSessionId(): Int {
        val id = nextSessionId
        nextSessionId = Math.addExact(nextSessionId, 1)
        return id
    }

    // ── URL & Analysis ──

    fun updateUrl(url: String) {
        val idx = _state.value.activeTabIndex
        val id = _state.value.activeSession.id
        retireAnalysis(id)
        updateSession(idx) {
            it.copy(
                inputUrl = url,
                analysisError = null,
                availableFormats = emptyList(),
                availableSubtitles = emptyList(),
                selectedSubtitle = null,
                lastUrlAnalyzed = "",
            )
        }
        if (url.isNotBlank()) queueFormatScan(_state.value.activeSession, delayMs = 800)
    }

    fun scanFormats() {
        val tab = _state.value.activeSession
        if (tab.inputUrl.isBlank() || tab.phase == DownloadPhase.DOWNLOADING ||
            analysisJobs[tab.id]?.isActive == true
        ) return
        queueFormatScan(tab)
    }

    private fun queueFormatScan(tab: DownloadSessionState, delayMs: Long = 0) {
        val control = AnalysisControl()
        analysisControls[tab.id] = control
        analysisJobs[tab.id] = scope.launch {
            if (delayMs > 0) delay(delayMs)
            analyzeUrl(tab.id, tab.analysisGeneration, tab.inputUrl, control)
        }
    }

    private fun retireAnalysis(id: Int) {
        analysisControls.remove(id)?.let { control ->
            control.retire()
            scope.launch(Dispatchers.IO) { control.stopOwned() }
        }
        analysisJobs.remove(id)?.cancel()
        updateSessionById(id) { it.copy(analysisGeneration = Math.addExact(it.analysisGeneration, 1), isAnalyzing = false, phase = if (it.phase == DownloadPhase.ANALYZING) DownloadPhase.IDLE else it.phase) }
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
        retireAnalysis(session.id)
        _state.value.sessions.mapNotNull { it.processId }.forEach(::destroyYtDlpProcess)
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
                finishedQuality = null,
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
                            finishedQuality = result.quality,
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

                    activeDownloadPaths.remove(session.id)
                    updateSession(idx) {
                        it.copy(
                            phase = DownloadPhase.ERROR,
                            errorMessage = e.message ?: "Download failed",
                            processId = null,
                        )
                    }
                }
            }
        }
    }

    fun cancelDownload() {
        val idx = _state.value.activeTabIndex
        val session = _state.value.activeSession
        cancelledDownloadSessionIds.add(session.id)
        destroyYtDlpProcess(session.processId ?: "download_${session.id}")
        cleanupCancelledDownload(session.id)
        downloadJob?.cancel()
        updateSession(idx) { it.copy(phase = DownloadPhase.IDLE, progress = DownloadProgress(), processId = null) }
    }

    fun resetSession() {
        val idx = _state.value.activeTabIndex
        retireAnalysis(_state.value.activeSession.id)
        updateSession(idx) {
            newDownloadSession(it.id, it.tabName, it.downloadType, _state.value.settings.lastVideoFormat).copy(analysisGeneration = it.analysisGeneration)
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

    internal fun updateSessionById(id: Int, transform: (DownloadSessionState) -> DownloadSessionState) {
        _state.update { state ->
            val idx = state.sessions.indexOfFirst { it.id == id }
            if (idx < 0) state else state.copy(sessions = state.sessions.toMutableList().apply { this[idx] = transform(this[idx]) })
        }
    }

}
