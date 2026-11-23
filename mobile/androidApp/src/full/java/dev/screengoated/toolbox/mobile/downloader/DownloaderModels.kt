package dev.screengoated.toolbox.mobile.downloader

import kotlinx.serialization.Serializable

enum class ToolInstallStatus {
    CHECKING, MISSING, DOWNLOADING, EXTRACTING, INSTALLED, REMOVAL_PENDING, ERROR
}

enum class YtDlpUpdateStatus { IDLE, CHECKING, UPDATED, UP_TO_DATE, FAILED }

data class ToolState(
    val status: ToolInstallStatus = ToolInstallStatus.MISSING,
    val progress: Float = 0f,
    val version: String? = null,
    val error: String? = null,
    val retryable: Boolean = false,
)

enum class DownloadPhase {
    IDLE, ANALYZING, DOWNLOADING, FINISHED, ERROR
}

@Serializable
enum class DownloadType { VIDEO, AUDIO }

data class DownloadProgress(
    val fraction: Float = 0f,
    val statusMessage: String = "",
)

data class DownloadSessionState(
    val id: Int,
    val tabName: String,
    val inputUrl: String = "",
    val phase: DownloadPhase = DownloadPhase.IDLE,
    val progress: DownloadProgress = DownloadProgress(),
    val downloadType: DownloadType = DownloadType.VIDEO,
    val availableFormats: List<String> = emptyList(),
    val selectedFormat: String? = null,
    val availableSubtitles: List<String> = emptyList(),
    val selectedSubtitle: String? = null,
    val analysisError: String? = null,
    val finishedFilePath: String? = null,
    val finishedFileUri: String? = null,
    val finishedQuality: Int? = null,
    val errorMessage: String? = null,
    val logs: List<String> = emptyList(),
    val showErrorLog: Boolean = false,
    val isAnalyzing: Boolean = false,
    val analysisGeneration: Long = 0,
    val lastUrlAnalyzed: String = "",
    val processId: String? = null,
)

internal fun newDownloadSession(id: Int, tabName: String, type: DownloadType, preferredFormat: String?): DownloadSessionState =
    DownloadSessionState(id = id, tabName = tabName, downloadType = type, selectedFormat = preferredFormat)

@Serializable
data class DownloaderSettings(
    val customDownloadPath: String? = null,
    val useMetadata: Boolean = true,
    val useSponsorBlock: Boolean = false,
    val useSubtitles: Boolean = false,
    val usePlaylist: Boolean = false,
    val downloadType: DownloadType = DownloadType.VIDEO,
    val lastVideoFormat: String? = null,
    val selectedSubtitle: String? = null,
)

data class DownloaderUiState(
    val ytdlp: ToolState = ToolState(),
    val ffmpeg: ToolState = ToolState(),
    val ytDlpUpdate: YtDlpUpdateStatus = YtDlpUpdateStatus.IDLE,
    val sessions: List<DownloadSessionState> = listOf(
        DownloadSessionState(id = 1, tabName = "Tab 1"),
    ),
    val activeTabIndex: Int = 0,
    val settings: DownloaderSettings = DownloaderSettings(),
) {
    val toolsReady: Boolean
        get() = ytdlp.status == ToolInstallStatus.INSTALLED ||
            ytdlp.status == ToolInstallStatus.REMOVAL_PENDING
    val activeSession: DownloadSessionState
        get() = sessions[activeTabIndex.coerceIn(sessions.indices)]
}
