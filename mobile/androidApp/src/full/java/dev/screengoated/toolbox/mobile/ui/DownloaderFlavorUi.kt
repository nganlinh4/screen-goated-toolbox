@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.downloader.DownloaderHolder
import dev.screengoated.toolbox.mobile.downloader.DownloaderViewModel
import dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus
import dev.screengoated.toolbox.mobile.downloader.UpdateStatus
import dev.screengoated.toolbox.mobile.downloader.ui.DownloaderScreen
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

/** Video-downloader tools card. Sideload-only; the Play flavor stubs this out. */
@Composable
internal fun DownloaderToolsCard(
    locale: MobileLocaleText,
    onHelp: (Pair<String, String>) -> Unit,
) {
    val context = LocalContext.current
    val downloaderRepository = remember(context) { DownloaderHolder.get(context) }
    val downloaderState by downloaderRepository.state.collectAsState()

    ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.primary) {
        Text(
            text = "Video Downloader",
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.Bold,
        )
        DownloadedToolRow(
            name = "yt-dlp",
            icon = R.drawable.ms_emoji_symbols,
            statusText = when (downloaderState.ytdlp.status) {
                ToolInstallStatus.INSTALLED ->
                    downloaderState.ytdlp.version ?: locale.dlDepsReady
                ToolInstallStatus.DOWNLOADING -> locale.dlDepsDownloading
                ToolInstallStatus.EXTRACTING -> locale.dlDepsExtracting
                ToolInstallStatus.CHECKING -> locale.dlDepsChecking
                ToolInstallStatus.ERROR -> downloaderState.ytdlp.error ?: locale.dlStatusError
                ToolInstallStatus.MISSING -> locale.dlDepsNotInstalled
            },
            statusColor = when (downloaderState.ytdlp.status) {
                ToolInstallStatus.INSTALLED -> MaterialTheme.colorScheme.primary
                ToolInstallStatus.ERROR -> MaterialTheme.colorScheme.error
                else -> MaterialTheme.colorScheme.onSurfaceVariant
            },
            accent = if (downloaderState.ytdlp.status == ToolInstallStatus.INSTALLED) {
                MaterialTheme.colorScheme.primary
            } else {
                MaterialTheme.colorScheme.tertiary
            },
            onHelpClick = { onHelp("yt-dlp" to locale.downloader.toolDescYtdlp ) },
            progressFraction = if (
                downloaderState.ytdlp.status == ToolInstallStatus.DOWNLOADING ||
                downloaderState.ytdlp.status == ToolInstallStatus.EXTRACTING ||
                downloaderState.ytdlp.status == ToolInstallStatus.CHECKING
            ) {
                -1f
            } else {
                null
            },
            updateText = downloaderUpdateText(downloaderState.ytdlpUpdate, locale),
            updateColor = downloaderUpdateColor(downloaderState.ytdlpUpdate),
            action = when (downloaderState.ytdlp.status) {
                ToolInstallStatus.MISSING,
                ToolInstallStatus.ERROR -> ToolAction(
                    text = locale.dlDepsInstall,
                    role = ToolActionRole.TONAL,
                    onClick = { downloaderRepository.installTools() },
                )
                ToolInstallStatus.INSTALLED -> ToolAction(
                    text = if (downloaderState.ytdlpUpdate == UpdateStatus.CHECKING) "..." else locale.downloader.toolUpdate,
                    role = ToolActionRole.TONAL,
                    enabled = downloaderState.ytdlpUpdate != UpdateStatus.CHECKING,
                    onClick = { downloaderRepository.checkUpdates() },
                )
                else -> null
            },
        )
        DownloadedToolRow(
            name = "FFmpeg",
            icon = R.drawable.ms_display_settings,
            statusText = if (downloaderState.ffmpeg.status == ToolInstallStatus.INSTALLED) {
                downloaderState.ffmpeg.version ?: locale.dlDepsReady
            } else {
                locale.dlDepsNotInstalled
            },
            statusColor = if (downloaderState.ffmpeg.status == ToolInstallStatus.INSTALLED) {
                MaterialTheme.colorScheme.primary
            } else {
                MaterialTheme.colorScheme.onSurfaceVariant
            },
            accent = MaterialTheme.colorScheme.tertiary,
            onHelpClick = { onHelp("FFmpeg" to locale.downloader.toolDescFfmpeg ) },
            progressFraction = if (
                downloaderState.ffmpeg.status == ToolInstallStatus.DOWNLOADING ||
                downloaderState.ffmpeg.status == ToolInstallStatus.EXTRACTING ||
                downloaderState.ffmpeg.status == ToolInstallStatus.CHECKING
            ) {
                -1f
            } else {
                null
            },
            action = if (downloaderState.ffmpeg.status == ToolInstallStatus.MISSING) {
                ToolAction(
                    text = locale.dlDepsInstall,
                    role = ToolActionRole.TONAL,
                    onClick = { downloaderRepository.installTools() },
                )
            } else {
                null
            },
        )
    }
}

/** Full downloader screen. Sideload-only; the Play flavor stubs this out. */
@Composable
internal fun DownloaderScreenWrapper(locale: MobileLocaleText, onBack: () -> Unit) {
    val context = LocalContext.current
    val vm: DownloaderViewModel = viewModel(
        factory = DownloaderViewModel.factory(DownloaderHolder.get(context)),
    )
    DownloaderScreen(viewModel = vm, locale = locale, onBack = onBack)
}

private fun downloaderUpdateText(
    status: UpdateStatus,
    locale: MobileLocaleText,
): String? = when (status) {
    UpdateStatus.UPDATE_AVAILABLE -> locale.downloader.toolUpdated
    UpdateStatus.UP_TO_DATE -> locale.downloader.toolUpToDate
    UpdateStatus.CHECKING -> locale.downloader.toolUpdating
    UpdateStatus.ERROR -> locale.downloader.toolUpdateFailed
    else -> null
}

@Composable
private fun downloaderUpdateColor(
    status: UpdateStatus,
): Color = when (status) {
    UpdateStatus.UPDATE_AVAILABLE -> MaterialTheme.colorScheme.primary
    UpdateStatus.ERROR -> MaterialTheme.colorScheme.error
    else -> MaterialTheme.colorScheme.onSurfaceVariant
}
