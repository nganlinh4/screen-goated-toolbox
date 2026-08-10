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
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import okhttp3.OkHttpClient

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
    internal var activeAnalysisProcessId: String? = null

    // ── Tool management ──

    @Volatile internal var runtimeDelivery = loadDownloaderRuntimeDelivery(context)
    @Volatile internal var runtimeInstaller = runtimeDelivery?.let {
        DownloaderRuntimeInstaller(context, it, OkHttpClient())
    }
    internal val runtimeUpdateMutex = Mutex()
    internal val runtimeRemovalPreferences: SharedPreferences = context.getSharedPreferences(
        "downloader_runtime_state",
        Context.MODE_PRIVATE,
    )
    internal val runtimeLeases = RuntimeLeaseRegistry<DownloaderRuntimeKey> {
        finishDownloaderRemoval()
    }
    @Volatile internal var processHost = runtimeInstaller?.let { installer ->
        DownloaderProcessHost(context, installer) { acquireDownloaderRuntimeLease() }
    }
    internal var installJob: Job? = null

    init {
        if (runtimeRemovalPreferences.getBoolean(REMOVAL_PENDING_KEY, false)) {
            runtimeLeases.requestRemoval(DownloaderRuntimeKey.RUNTIME)
        }
    }

    fun checkTools() = refreshDownloaderTools()

    fun installTools() = startDownloaderInstall()

    fun deleteTools() = requestDownloaderRemoval()

    fun calculateTotalDepsSize(): String = calculateDownloaderTotalSize()

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
        activeAnalysisProcessId?.let(::destroyYtDlpProcess)
        activeAnalysisProcessId = null
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
        activeAnalysisProcessId?.let(::destroyYtDlpProcess)
        activeAnalysisProcessId = null
        analysisJob?.cancel()
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
