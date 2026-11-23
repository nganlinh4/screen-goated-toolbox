package dev.screengoated.toolbox.mobile.downloader

import androidx.core.content.edit
import java.util.Locale
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.sync.withLock

internal enum class DownloaderRuntimeKey {
    RUNTIME,
}

internal fun DownloaderRepository.refreshDownloaderTools() {
    scope.launch {
        val state = withContext(Dispatchers.IO) { runtimeToolStates() }
        _state.update { it.copy(ytdlp = state.first, ffmpeg = state.second) }
        startUpstreamYtDlpUpdate()
    }
}

internal fun DownloaderRepository.startDownloaderInstall() {
    val installer = runtimeInstaller
    if (installer == null) {
        setRuntimeError("Downloader runtime delivery is unavailable in this build.")
        return
    }
    if (installJob?.isActive == true || _state.value.ytdlp.status == ToolInstallStatus.INSTALLED ||
        runtimeLeases.isRemovalPending(DownloaderRuntimeKey.RUNTIME)
    ) return
    val lease = runtimeLeases.acquire(listOf(DownloaderRuntimeKey.RUNTIME)) ?: return
    installJob = scope.launch {
        _state.update {
            it.copy(
                ytdlp = ToolState(ToolInstallStatus.DOWNLOADING),
                ffmpeg = ToolState(ToolInstallStatus.DOWNLOADING),
            )
        }
        try {
            withContext(Dispatchers.IO) {
                if (!hasUsableDownloaderRuntime(installer)) installer.install(::updateInstallProgress)
            }
            val states = withContext(Dispatchers.IO) { runtimeToolStates() }
            _state.update { it.copy(ytdlp = states.first, ffmpeg = states.second) }
            startUpstreamYtDlpUpdate()
        } catch (cancelled: CancellationException) {
            if (!runtimeLeases.isRemovalPending(DownloaderRuntimeKey.RUNTIME)) {
                val states = withContext(Dispatchers.IO) { runtimeToolStates() }
                _state.update { it.copy(ytdlp = states.first, ffmpeg = states.second) }
            }
        } catch (error: Throwable) {
            if (!runtimeLeases.isRemovalPending(DownloaderRuntimeKey.RUNTIME)) {
                setRuntimeError(error.message ?: "Downloader runtime installation failed.")
            }
        } finally {
            installJob = null
            lease.close()
        }
    }
}

internal fun DownloaderRepository.requestDownloaderRemoval() {
    installJob?.cancel()
    ytDlpUpdateJob?.cancel()
    runtimeRemovalPreferences.edit { putBoolean(REMOVAL_PENDING_KEY, true) }
    setRemovalPendingState()
    runtimeLeases.requestRemoval(DownloaderRuntimeKey.RUNTIME)
}

internal fun DownloaderRepository.finishDownloaderRemoval() {
    try {
        val runtimeRemoved = runtimeInstaller?.remove() ?: true
        val ytDlpRemoved = ytDlpStore?.remove() ?: true
        check(runtimeRemoved && ytDlpRemoved) { "Downloader files could not be removed. Try again." }
        runtimeRemovalPreferences.edit {
            remove(REMOVAL_PENDING_KEY)
            remove(LAST_UPDATE_CHECK)
            remove(LAST_UPDATE_ATTEMPT)
        }
        runtimeLeases.completeRemoval(DownloaderRuntimeKey.RUNTIME)
        _state.update {
            it.copy(
                ytdlp = ToolState(ToolInstallStatus.MISSING),
                ffmpeg = ToolState(ToolInstallStatus.MISSING),
                ytDlpUpdate = YtDlpUpdateStatus.IDLE,
            )
        }
    } catch (error: Throwable) {
        val message = error.message ?: "Downloader removal failed. Try again."
        _state.update {
            it.copy(
                ytdlp = ToolState(
                    ToolInstallStatus.REMOVAL_PENDING,
                    error = message,
                    retryable = true,
                ),
                ffmpeg = ToolState(
                    ToolInstallStatus.REMOVAL_PENDING,
                    error = message,
                    retryable = true,
                ),
            )
        }
    }
}

internal fun DownloaderRepository.acquireDownloaderRuntimeLease(): AutoCloseable? {
    val installer = runtimeInstaller ?: return null
    val lease = runtimeLeases.acquire(listOf(DownloaderRuntimeKey.RUNTIME)) ?: return null
    var ready = false
    try {
        ready = hasUsableDownloaderRuntime(installer)
        return lease.takeIf { ready }
    } finally {
        if (!ready) lease.close()
    }
}

private fun DownloaderRepository.hasUsableDownloaderRuntime(installer: DownloaderRuntimeInstaller): Boolean =
    installer.canRun(ytDlpStore?.current()?.executable ?: installer.ytdlpFile)

internal suspend fun DownloaderRepository.executeYtDlp(
    request: YtDlpCommand,
    processId: String? = null,
    analysis: AnalysisControl? = null,
    callback: ((Float, Long, String) -> Unit)? = null,
): YtDlpProcessResult {
    val host = requireNotNull(processHost) {
        "Downloader runtime delivery is unavailable in this build"
    }
    scope.launch { startUpstreamYtDlpUpdate() }
    return host.execute(request, processId, callback, analysis)
}

internal fun DownloaderRepository.startUpstreamYtDlpUpdate(manual: Boolean = false) {
    if (_state.value.ytdlp.status != ToolInstallStatus.INSTALLED || ytDlpUpdateJob?.isActive == true) return
    val now = System.currentTimeMillis()
    val lastSuccess = runtimeRemovalPreferences.getLong(LAST_UPDATE_CHECK, 0L)
    val lastAttempt = runtimeRemovalPreferences.getLong(LAST_UPDATE_ATTEMPT, 0L)
    if (!manual && (now - lastSuccess in 0 until UPDATE_CHECK_MS || now - lastAttempt in 0 until UPDATE_RETRY_MS)) {
        return
    }

    ytDlpUpdateJob = scope.launch {
        val lease = runtimeLeases.acquire(listOf(DownloaderRuntimeKey.RUNTIME)) ?: return@launch
        _state.update { it.copy(ytDlpUpdate = YtDlpUpdateStatus.CHECKING) }
        try {
            runtimeRemovalPreferences.edit { putLong(LAST_UPDATE_ATTEMPT, now) }
            val updated = withContext(Dispatchers.IO) { updateYtDlpFromUpstream() }
            val activeVersion = withContext(Dispatchers.IO) { calculateYtdlpSize() }
            runtimeRemovalPreferences.edit { putLong(LAST_UPDATE_CHECK, now) }
            _state.update { state ->
                state.copy(
                    ytdlp = state.ytdlp.copy(version = activeVersion),
                    ytDlpUpdate = when {
                        updated -> YtDlpUpdateStatus.UPDATED
                        manual -> YtDlpUpdateStatus.UP_TO_DATE
                        else -> YtDlpUpdateStatus.IDLE
                    },
                )
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            android.util.Log.i("SGT-Downloader", "Upstream yt-dlp update unavailable", error)
            _state.update { it.copy(ytDlpUpdate = YtDlpUpdateStatus.FAILED) }
        } finally {
            lease.close()
        }
    }
}

/** Caller owns the downloader runtime lease for the duration of this update. */
internal suspend fun DownloaderRepository.updateYtDlpFromUpstream(): Boolean =
    runtimeUpdateMutex.withLock {
        val store = requireNotNull(ytDlpStore)
        val host = requireNotNull(processHost)
        check(runtimeInstaller?.isNativeInstalled() == true) { "Downloader runtime is not installed" }
        upstreamYtDlp.installLatest(store, host::verifyCandidate)
    }

internal fun DownloaderRepository.destroyYtDlpProcess(processId: String): Boolean =
    processHost?.destroy(processId) ?: false

internal fun DownloaderRepository.calculateYtdlpSize(): String {
    val installer = runtimeInstaller ?: return "0 MB"
    val active = ytDlpStore?.current()
    val bytes = (active?.executable?.length() ?: installer.componentBytes(DownloaderArtifactRole.YT_DLP)) +
        installer.componentBytes(DownloaderArtifactRole.PYTHON)
    val version = active?.version?.tag
    val size = formatMegabytes(bytes)
    return if (version == null) size else "$version ($size)"
}

internal fun DownloaderRepository.calculateFfmpegSize(): String =
    formatMegabytes(runtimeInstaller?.componentBytes(DownloaderArtifactRole.FFMPEG) ?: 0L)

internal fun DownloaderRepository.calculateDownloaderTotalSize(): String =
    formatMegabytes((runtimeInstaller?.installedBytes() ?: 0L) + (ytDlpStore?.installedBytes() ?: 0L), decimals = 0)

private fun DownloaderRepository.runtimeToolStates(): Pair<ToolState, ToolState> {
    if (runtimeLeases.isRemovalPending(DownloaderRuntimeKey.RUNTIME) ||
        runtimeRemovalPreferences.getBoolean(REMOVAL_PENDING_KEY, false)
    ) {
        val message = removalPendingMessage()
        return ToolState(ToolInstallStatus.REMOVAL_PENDING, error = message) to
            ToolState(ToolInstallStatus.REMOVAL_PENDING, error = message)
    }
    val installer = runtimeInstaller
        ?: return ToolState(
            ToolInstallStatus.ERROR,
            error = "Downloader runtime delivery is unavailable in this build.",
        ) to ToolState(
            ToolInstallStatus.ERROR,
            error = "Downloader runtime delivery is unavailable in this build.",
        )
    return if (hasUsableDownloaderRuntime(installer)) {
        ToolState(ToolInstallStatus.INSTALLED, version = calculateYtdlpSize()) to
            ToolState(ToolInstallStatus.INSTALLED, version = calculateFfmpegSize())
    } else {
        ToolState(ToolInstallStatus.MISSING) to ToolState(ToolInstallStatus.MISSING)
    }
}

private fun DownloaderRepository.updateInstallProgress(progress: DownloaderInstallProgress) {
    val percent = (progress.fraction.coerceIn(0f, 1f) * 100).toInt()
    val status = if (progress.extracting) ToolInstallStatus.EXTRACTING
    else ToolInstallStatus.DOWNLOADING
    val label = if (progress.extracting) "Extracting ${progress.role.wireName}"
    else "${progress.role.wireName}: $percent%"
    _state.update { current ->
        when (progress.role) {
            DownloaderArtifactRole.FFMPEG -> current.copy(
                ffmpeg = ToolState(status, version = label),
            )
            DownloaderArtifactRole.YT_DLP,
            DownloaderArtifactRole.PYTHON -> current.copy(
                ytdlp = ToolState(status, version = label),
            )
        }
    }
}

private fun DownloaderRepository.setRuntimeError(message: String) {
    _state.update {
        it.copy(
            ytdlp = ToolState(ToolInstallStatus.ERROR, error = message),
            ffmpeg = ToolState(ToolInstallStatus.ERROR, error = message),
        )
    }
}

private fun DownloaderRepository.setRemovalPendingState() {
    val message = removalPendingMessage()
    _state.update {
        it.copy(
            ytdlp = ToolState(ToolInstallStatus.REMOVAL_PENDING, error = message),
            ffmpeg = ToolState(ToolInstallStatus.REMOVAL_PENDING, error = message),
        )
    }
}

private fun DownloaderRepository.removalPendingMessage(): String =
    if (runtimeLeases.isInUse(DownloaderRuntimeKey.RUNTIME)) {
        "Removal pending until active downloader work stops."
    } else {
        "Downloader removal is pending."
    }

private fun formatMegabytes(bytes: Long, decimals: Int = 1): String =
    String.format(Locale.US, "%.${decimals}f MB", bytes / (1024.0 * 1024.0))

internal const val REMOVAL_PENDING_KEY = "removal_pending"
private const val LAST_UPDATE_CHECK = "yt_dlp_last_successful_check"
private const val LAST_UPDATE_ATTEMPT = "yt_dlp_last_check_attempt"
internal const val UPDATE_CHECK_MS = 24 * 60 * 60 * 1000L
private const val UPDATE_RETRY_MS = 60 * 60 * 1000L
