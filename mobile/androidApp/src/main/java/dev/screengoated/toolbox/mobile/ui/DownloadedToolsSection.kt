@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus
import dev.screengoated.toolbox.mobile.downloader.UpdateStatus
import dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.launch

@Composable
internal fun DownloadedToolsSection(locale: MobileLocaleText) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val manager = remember {
        (context.applicationContext as dev.screengoated.toolbox.mobile.SgtMobileApplication)
            .appContainer.parakeetModelManager
    }
    val modelState by manager.state.collectAsState()
    val scope = rememberCoroutineScope()
    var helpDialog by remember { mutableStateOf<Pair<String, String>?>(null) }

    helpDialog?.let { (title, desc) ->
        androidx.compose.material3.AlertDialog(
            onDismissRequest = { helpDialog = null },
            title = { Text(title) },
            text = { Text(desc) },
            confirmButton = {
                androidx.compose.material3.TextButton(onClick = { helpDialog = null }) {
                    Text("OK")
                }
            },
        )
    }

    val app = context.applicationContext as dev.screengoated.toolbox.mobile.SgtMobileApplication
    val downloaderRepository = app.appContainer.downloaderRepository
    val downloaderState = downloaderRepository.state.collectAsState().value
    val parakeetState = modelState

    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
    ) {
        Column(
            modifier = Modifier.padding(ShellSpacing.innerPad),
            verticalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
        ) {
            Text(
                text = locale.shellDownloadedToolsLabel,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.Bold,
            )
            Text(
                text = locale.dlDepsRequired,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            DownloadedToolRow(
                name = "Parakeet",
                icon = R.drawable.ms_raven,
                statusText = when (val state = parakeetState) {
                    is ParakeetModelState.Missing -> locale.dlDepsNotInstalled
                    is ParakeetModelState.Downloading ->
                        "Downloading ${state.currentFile} (${state.progress.toInt()}%)"
                    is ParakeetModelState.Installed -> {
                        val mb = state.sizeBytes / (1024.0 * 1024.0)
                        "Installed (%.1f MB) (English only)".format(mb)
                    }
                    is ParakeetModelState.Deleting -> "Deleting..."
                    is ParakeetModelState.Error -> state.message
                },
                statusColor = when (parakeetState) {
                    is ParakeetModelState.Installed -> MaterialTheme.colorScheme.tertiary
                    is ParakeetModelState.Error -> MaterialTheme.colorScheme.error
                    else -> MaterialTheme.colorScheme.onSurfaceVariant
                },
                accent = MaterialTheme.colorScheme.secondary,
                onHelpClick = { helpDialog = "Parakeet" to locale.toolDescParakeet },
                progressFraction = if (parakeetState is ParakeetModelState.Downloading) {
                    parakeetState.progress / 100f
                } else {
                    null
                },
                action = when (parakeetState) {
                    is ParakeetModelState.Missing,
                    is ParakeetModelState.Error -> ToolAction(
                        text = locale.dlDepsInstall,
                        role = ToolActionRole.TONAL,
                        onClick = { scope.launch { manager.download() } },
                    )
                    is ParakeetModelState.Installed -> ToolAction(
                        text = locale.toolDelete,
                        role = ToolActionRole.DESTRUCTIVE,
                        onClick = { manager.delete() },
                    )
                    else -> null
                },
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
                onHelpClick = { helpDialog = "yt-dlp" to locale.toolDescYtdlp },
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
                        text = if (downloaderState.ytdlpUpdate == UpdateStatus.CHECKING) "..." else locale.toolUpdate,
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
                onHelpClick = { helpDialog = "FFmpeg" to locale.toolDescFfmpeg },
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
}

@Composable
private fun DownloadedToolRow(
    name: String,
    @androidx.annotation.DrawableRes icon: Int,
    statusText: String,
    statusColor: androidx.compose.ui.graphics.Color,
    accent: androidx.compose.ui.graphics.Color,
    onHelpClick: () -> Unit,
    progressFraction: Float? = null,
    updateText: String? = null,
    updateColor: androidx.compose.ui.graphics.Color = MaterialTheme.colorScheme.onSurfaceVariant,
    action: ToolAction? = null,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
    ) {
        Icon(
            painter = painterResource(icon),
            contentDescription = null,
            modifier = Modifier.size(22.dp),
            tint = accent,
        )
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(
                    text = name,
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.SemiBold,
                )
                IconButton(
                    onClick = onHelpClick,
                    modifier = Modifier.size(24.dp),
                ) {
                    Icon(
                        painter = painterResource(R.drawable.ms_info),
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            Text(
                text = statusText,
                style = MaterialTheme.typography.bodySmall,
                color = statusColor,
            )
            if (progressFraction != null) {
                if (progressFraction <= 0f || progressFraction >= 1f) {
                    androidx.compose.material3.LinearWavyProgressIndicator(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(5.dp),
                    )
                } else {
                    androidx.compose.material3.LinearWavyProgressIndicator(
                        progress = { progressFraction },
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(5.dp),
                    )
                }
            }
            if (!updateText.isNullOrBlank()) {
                Text(
                    text = updateText,
                    style = MaterialTheme.typography.labelSmall,
                    color = updateColor,
                )
            }
        }
        if (action != null) {
            when (action.role) {
                ToolActionRole.TONAL -> {
                    FilledTonalButton(
                        onClick = action.onClick,
                        enabled = action.enabled,
                    ) {
                        Text(action.text)
                    }
                }
                ToolActionRole.DESTRUCTIVE -> {
                    Button(
                        onClick = action.onClick,
                        enabled = action.enabled,
                        colors = ButtonDefaults.buttonColors(
                            containerColor = MaterialTheme.colorScheme.error,
                        ),
                    ) {
                        Text(action.text)
                    }
                }
            }
        }
    }
}

private fun downloaderUpdateText(
    status: UpdateStatus,
    locale: MobileLocaleText,
): String? = when (status) {
    UpdateStatus.UPDATE_AVAILABLE -> locale.toolUpdated
    UpdateStatus.UP_TO_DATE -> locale.toolUpToDate
    UpdateStatus.CHECKING -> locale.toolUpdating
    UpdateStatus.ERROR -> locale.toolUpdateFailed
    else -> null
}

@Composable
private fun downloaderUpdateColor(
    status: UpdateStatus,
): androidx.compose.ui.graphics.Color = when (status) {
    UpdateStatus.UPDATE_AVAILABLE -> MaterialTheme.colorScheme.primary
    UpdateStatus.ERROR -> MaterialTheme.colorScheme.error
    else -> MaterialTheme.colorScheme.onSurfaceVariant
}

private data class ToolAction(
    val text: String,
    val role: ToolActionRole,
    val enabled: Boolean = true,
    val onClick: () -> Unit,
)

private enum class ToolActionRole {
    TONAL,
    DESTRUCTIVE,
}
