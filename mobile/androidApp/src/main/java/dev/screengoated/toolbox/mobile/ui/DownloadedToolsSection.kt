@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material.icons.rounded.Mic
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
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
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

            DownloadedToolRow(
                name = "Parakeet",
                description = locale.toolDescParakeet,
                onHelpClick = { helpDialog = "Parakeet" to locale.toolDescParakeet },
                note = "(English only)",
                icon = Icons.Rounded.Mic,
                state = modelState,
                locale = locale,
                onDownload = { scope.launch { manager.download() } },
                onDelete = { manager.delete() },
            )

            // yt-dlp and ffmpeg — managed by the video downloader
            val app = (context.applicationContext
                as dev.screengoated.toolbox.mobile.SgtMobileApplication)
            val dlRepo = app.appContainer.downloaderRepository
            val dlState = dlRepo.state.collectAsState().value

            VideoToolRow(
                name = "yt-dlp + Python",
                description = locale.toolDescYtdlp,
                onHelpClick = { helpDialog = "yt-dlp + Python" to locale.toolDescYtdlp },
                icon = Icons.Rounded.Download,
                status = dlState.ytdlp.status,
                version = dlState.ytdlp.version,
                error = dlState.ytdlp.error,
                updateStatus = dlState.ytdlpUpdate,
                locale = locale,
                onInstall = { dlRepo.installTools() },
                onUpdate = { dlRepo.checkUpdates() },
                showUpdateButton = true,
                bundledLabel = locale.toolBundled,
            )

            VideoToolRow(
                name = "ffmpeg + python",
                description = locale.toolDescFfmpeg,
                onHelpClick = { helpDialog = "ffmpeg" to locale.toolDescFfmpeg },
                icon = Icons.Rounded.GraphicEq,
                status = dlState.ffmpeg.status,
                version = if (dlState.ffmpeg.status == dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.INSTALLED) {
                    dlState.ffmpeg.version
                } else null,
                error = null,
                updateStatus = dev.screengoated.toolbox.mobile.downloader.UpdateStatus.IDLE,
                locale = locale,
                onInstall = { dlRepo.installTools() },
                onUpdate = {},
                bundledLabel = if (dlState.ffmpeg.status == dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.INSTALLED) {
                    "Installed (on-demand)"
                } else {
                    "Not downloaded"
                },
            )
        }
    }
}

@Composable
private fun DownloadedToolRow(
    name: String,
    description: String,
    onHelpClick: () -> Unit,
    note: String,
    icon: ImageVector,
    state: dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState,
    locale: MobileLocaleText,
    onDownload: () -> Unit,
    onDelete: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            modifier = Modifier.size(24.dp),
            tint = MaterialTheme.colorScheme.secondary,
        )
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(text = name, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold)
                IconButton(
                    onClick = onHelpClick,
                    modifier = Modifier.size(28.dp),
                ) {
                    Icon(
                        imageVector = Icons.Rounded.Info,
                        contentDescription = description,
                        modifier = Modifier.size(16.dp),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            Text(
                text = when (state) {
                    is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Missing -> locale.dlDepsNotInstalled
                    is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Downloading ->
                        "Downloading ${state.currentFile} (${state.progress.toInt()}%)"
                    is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Installed -> {
                        val mb = state.sizeBytes / (1024.0 * 1024.0)
                        "Installed (%.1f MB) $note".format(mb)
                    }
                    is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Deleting -> "Deleting..."
                    is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Error -> state.message
                },
                style = MaterialTheme.typography.bodySmall,
                color = when (state) {
                    is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Installed ->
                        MaterialTheme.colorScheme.tertiary
                    is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Error ->
                        MaterialTheme.colorScheme.error
                    else -> MaterialTheme.colorScheme.onSurfaceVariant
                },
            )
            if (state is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Downloading) {
                val fraction = state.progress / 100f
                if (fraction <= 0f || fraction >= 1f) {
                    androidx.compose.material3.LinearWavyProgressIndicator(
                        modifier = Modifier.fillMaxWidth().padding(top = 4.dp).height(5.dp),
                    )
                } else {
                    androidx.compose.material3.LinearWavyProgressIndicator(
                        progress = { fraction },
                        modifier = Modifier.fillMaxWidth().padding(top = 4.dp).height(5.dp),
                    )
                }
            }
        }
        when (state) {
            is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Missing,
            is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Error -> {
                FilledTonalButton(onClick = onDownload) { Text(locale.dlDepsInstall) }
            }
            is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Installed -> {
                Button(
                    onClick = onDelete,
                    colors = ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.error,
                    ),
                ) { Text(locale.toolDelete) }
            }
            else -> {}
        }
    }
}

@Composable
private fun VideoToolRow(
    name: String,
    description: String,
    onHelpClick: () -> Unit = {},
    icon: ImageVector,
    status: dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus,
    version: String?,
    error: String?,
    updateStatus: dev.screengoated.toolbox.mobile.downloader.UpdateStatus,
    locale: MobileLocaleText,
    onInstall: () -> Unit,
    onUpdate: () -> Unit,
    showUpdateButton: Boolean = false,
    bundledLabel: String? = null,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            modifier = Modifier.size(24.dp),
            tint = if (status == dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.INSTALLED) {
                MaterialTheme.colorScheme.primary
            } else {
                MaterialTheme.colorScheme.onSurfaceVariant
            },
        )
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(text = name, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold)
                IconButton(
                    onClick = onHelpClick,
                    modifier = Modifier.size(28.dp),
                ) {
                    Icon(
                        imageVector = Icons.Rounded.Info,
                        contentDescription = description,
                        modifier = Modifier.size(16.dp),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            Text(
                text = when (status) {
                    dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.INSTALLED ->
                        bundledLabel ?: (version ?: locale.dlDepsReady)
                    dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.DOWNLOADING -> locale.dlDepsDownloading
                    dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.EXTRACTING -> locale.dlDepsExtracting
                    dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.CHECKING -> locale.dlDepsChecking
                    dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.ERROR -> error ?: locale.dlStatusError
                    dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.MISSING -> locale.dlDepsNotInstalled
                },
                style = MaterialTheme.typography.bodySmall,
                color = when (status) {
                    dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.INSTALLED ->
                        MaterialTheme.colorScheme.primary
                    dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.ERROR ->
                        MaterialTheme.colorScheme.error
                    else -> MaterialTheme.colorScheme.onSurfaceVariant
                },
            )
            if (status == dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.DOWNLOADING ||
                status == dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.EXTRACTING ||
                status == dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.CHECKING
            ) {
                androidx.compose.material3.LinearWavyProgressIndicator(
                    modifier = Modifier.fillMaxWidth().padding(top = 4.dp).height(5.dp),
                )
            }
            // Update status
            when (updateStatus) {
                dev.screengoated.toolbox.mobile.downloader.UpdateStatus.UPDATE_AVAILABLE -> Text(
                    locale.toolUpdated,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.primary,
                )
                dev.screengoated.toolbox.mobile.downloader.UpdateStatus.UP_TO_DATE -> Text(
                    locale.toolUpToDate,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                dev.screengoated.toolbox.mobile.downloader.UpdateStatus.CHECKING -> Text(
                    locale.toolUpdating,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                dev.screengoated.toolbox.mobile.downloader.UpdateStatus.ERROR -> Text(
                    locale.toolUpdateFailed,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
                else -> {}
            }
        }
        when (status) {
            dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.MISSING,
            dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.ERROR -> {
                if (bundledLabel == null) {
                    Button(onClick = onInstall) { Text(locale.dlDepsInstall) }
                }
            }
            dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus.INSTALLED -> {
                if (showUpdateButton) {
                    val isUpdating = updateStatus == dev.screengoated.toolbox.mobile.downloader.UpdateStatus.CHECKING
                    Button(onClick = onUpdate, enabled = !isUpdating) {
                        Text(if (isUpdating) "..." else locale.toolUpdate)
                    }
                }
            }
            else -> {}
        }
    }
}

@Composable
private fun PlaceholderToolRow(name: String, icon: ImageVector, comingSoon: String) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            modifier = Modifier.size(24.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
        )
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = name,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
            )
            Text(
                text = comingSoon,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f),
            )
        }
    }
}
