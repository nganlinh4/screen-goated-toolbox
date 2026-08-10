package dev.screengoated.toolbox.mobile.downloader

import androidx.core.content.edit
import dev.screengoated.toolbox.mobile.service.nativelibs.RuntimeLeaseRegistry
import java.util.Locale
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal enum class DownloaderRuntimeKey {
    RUNTIME,
}

internal fun DownloaderRepository.refreshDownloaderTools() {
    scope.launch {
        val state = withContext(Dispatchers.IO) { runtimeToolStates() }
        _state.update { it.copy(ytdlp = state.first, ffmpeg = state.second) }
    }
}

internal fun DownloaderRepository.startDownloaderInstall() {
    val installer = runtimeInstaller
    if (installer == null) {
        setRuntimeError("Downloader runtime delivery is unavailable in this build.")
        return
    }
    if (installJob?.isActive == true || installer.isInstalled() ||
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
                installer.install { progress -> updateInstallProgress(progress) }
            }
            val states = withContext(Dispatchers.IO) { runtimeToolStates() }
            _state.update { it.copy(ytdlp = states.first, ffmpeg = states.second) }
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
    runtimeRemovalPreferences.edit { putBoolean(REMOVAL_PENDING_KEY, true) }
    setRemovalPendingState()
    runtimeLeases.requestRemoval(DownloaderRuntimeKey.RUNTIME)
}

internal fun DownloaderRepository.finishDownloaderRemoval() {
    try {
        val removed = runtimeInstaller?.remove() ?: true
        check(removed) { "Downloader files could not be removed. Try again." }
        runtimeRemovalPreferences.edit { putBoolean(REMOVAL_PENDING_KEY, false) }
        runtimeLeases.completeRemoval(DownloaderRuntimeKey.RUNTIME)
        _state.update {
            it.copy(
                ytdlp = ToolState(ToolInstallStatus.MISSING),
                ffmpeg = ToolState(ToolInstallStatus.MISSING),
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
    if (!installer.isInstalled()) return null
    return runtimeLeases.acquire(listOf(DownloaderRuntimeKey.RUNTIME))
}

internal fun DownloaderRepository.executeYtDlp(
    request: YtDlpCommand,
    processId: String? = null,
    callback: ((Float, Long, String) -> Unit)? = null,
): YtDlpProcessResult = requireNotNull(processHost) {
    "Downloader runtime delivery is unavailable in this build"
}.execute(request, processId, callback)

internal fun DownloaderRepository.destroyYtDlpProcess(processId: String): Boolean =
    processHost?.destroy(processId) ?: false

internal fun DownloaderRepository.calculateYtdlpSize(): String {
    val installer = runtimeInstaller ?: return "0 MB"
    val bytes = installer.componentBytes(DownloaderArtifactRole.YT_DLP) +
        installer.componentBytes(DownloaderArtifactRole.PYTHON)
    val version = runtimeDelivery?.version?.substringBefore("-android-")
    val size = formatMegabytes(bytes)
    return if (version == null) size else "$version ($size)"
}

internal fun DownloaderRepository.calculateFfmpegSize(): String =
    formatMegabytes(runtimeInstaller?.componentBytes(DownloaderArtifactRole.FFMPEG) ?: 0L)

internal fun DownloaderRepository.calculateDownloaderTotalSize(): String =
    formatMegabytes(runtimeInstaller?.installedBytes() ?: 0L, decimals = 0)

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
    return if (installer.isInstalled()) {
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
