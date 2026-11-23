@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearWavyProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.creation.CreationJobManager
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeManager
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeStatus
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun DownloadedToolsDialog(
    locale: MobileLocaleText,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current

    val creationRuntimeManager = remember { CreationRuntimeManager.get(context) }
    val creationRuntimeStatus by creationRuntimeManager.status.collectAsState()

    var helpDialog by remember { mutableStateOf<Pair<String, String>?>(null) }

    helpDialog?.let { (title, desc) ->
        AlertDialog(
            onDismissRequest = { helpDialog = null },
            title = { Text(title) },
            text = { Text(desc) },
            confirmButton = {
                TextButton(onClick = { helpDialog = null }) {
                    Text(locale.closeLabel)
                }
            },
        )
    }

    ExpressiveDialogSurface(
        title = locale.shellDownloadedToolsLabel,
        closeLabel = locale.closeLabel,
        icon = R.drawable.ms_download,
        accent = MaterialTheme.colorScheme.tertiary,
        morphPair = DialogCloseMorphPair,
        onDismiss = onDismiss,
        fitContentHeight = false,
        maxHeight = 700.dp,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {

            ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.tertiary) {
                Text(
                    text = locale.creationApps.common.tools.previewTools,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.Bold,
                )
                DownloadedToolRow(
                    name = locale.creationApps.common.tools.creationRuntime,
                    icon = R.drawable.ms_deployed_code,
                    statusText = when (val status = creationRuntimeStatus) {
                        CreationRuntimeStatus.Missing -> locale.dlDepsNotInstalled
                        is CreationRuntimeStatus.Downloading ->
                            downloadingStatus(locale, status.progress)
                        is CreationRuntimeStatus.Ready ->
                            installedStatus(locale, status.sizeBytes)
                        is CreationRuntimeStatus.RemovalPending -> status.message
                        is CreationRuntimeStatus.Failed -> status.message
                    },
                    statusColor = when (creationRuntimeStatus) {
                        is CreationRuntimeStatus.Ready -> MaterialTheme.colorScheme.tertiary
                        is CreationRuntimeStatus.Failed -> MaterialTheme.colorScheme.error
                        else -> MaterialTheme.colorScheme.onSurfaceVariant
                    },
                    accent = MaterialTheme.colorScheme.tertiary,
                    onHelpClick = {
                        helpDialog = locale.creationApps.common.tools.creationRuntime to
                            locale.creationApps.common.tools.creationRuntimeDescription
                    },
                    progressFraction = (creationRuntimeStatus as? CreationRuntimeStatus.Downloading)
                        ?.progress,
                    action = when (val status = creationRuntimeStatus) {
                        CreationRuntimeStatus.Missing,
                        is CreationRuntimeStatus.Failed -> ToolAction(
                            text = locale.dlDepsInstall,
                            role = ToolActionRole.TONAL,
                            onClick = creationRuntimeManager::startInstall,
                        )
                        is CreationRuntimeStatus.Ready -> ToolAction(
                            text = locale.downloader.toolDelete,
                            role = ToolActionRole.DESTRUCTIVE,
                            onClick = { CreationJobManager.get(context).removeRuntime() },
                        )
                        is CreationRuntimeStatus.RemovalPending ->
                            status.takeIf { it.retryable }?.let {
                                ToolAction(
                                    text = locale.downloader.toolDelete,
                                    role = ToolActionRole.DESTRUCTIVE,
                                    onClick = { CreationJobManager.get(context).removeRuntime() },
                                )
                            }
                        is CreationRuntimeStatus.Downloading -> null
                    },
                )
            }

            // Video downloader tools — sideload-only; stubbed out on the Play flavor.
            DownloaderToolsCard(locale = locale, onHelp = { helpDialog = it })

            // Shown only while files from removed offline engines remain on this device.
            LegacyOfflineModelsCard(locale = locale, onHelp = { helpDialog = it })
        }
    }
}

@Composable
internal fun DownloadedToolRow(
    name: String,
    @androidx.annotation.DrawableRes icon: Int,
    statusText: String,
    statusColor: Color,
    accent: Color,
    onHelpClick: () -> Unit,
    progressFraction: Float? = null,
    updateText: String? = null,
    updateColor: Color = MaterialTheme.colorScheme.onSurfaceVariant,
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
                    LinearWavyProgressIndicator(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(5.dp),
                    )
                } else {
                    LinearWavyProgressIndicator(
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

private fun downloadingStatus(locale: MobileLocaleText, progress: Float): String =
    "${locale.dlDepsDownloading} (${(progress * 100).toInt()}%)"

private fun installedStatus(locale: MobileLocaleText, sizeBytes: Long): String =
    "${locale.dlDepsReady} (${formatMb(sizeBytes)} MB)"

internal fun formatMb(bytes: Long): String = "%.1f".format(bytes / (1024.0 * 1024.0))

internal data class ToolAction(
    val text: String,
    val role: ToolActionRole,
    val enabled: Boolean = true,
    val onClick: () -> Unit,
)

internal enum class ToolActionRole {
    TONAL,
    DESTRUCTIVE,
}
