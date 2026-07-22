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
import dev.screengoated.toolbox.mobile.creation.DepthPreviewModelManager
import dev.screengoated.toolbox.mobile.creation.DepthPreviewModelStatus
import dev.screengoated.toolbox.mobile.service.moonshine.MoonshineLanguage
import dev.screengoated.toolbox.mobile.service.moonshine.MoonshineModelManager
import dev.screengoated.toolbox.mobile.service.moonshine.ZipformerLanguage
import dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun DownloadedToolsDialog(
    locale: MobileLocaleText,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current

    // Moonshine + Zipformer
    val moonshineManager = remember { MoonshineModelManager(context) }
    val moonshineStatuses by moonshineManager.moonshineStatuses.collectAsState()
    val zipformerStatuses by moonshineManager.zipformerStatuses.collectAsState()
    val depthPreviewManager = remember { DepthPreviewModelManager.get(context) }
    val depthPreviewStatus by depthPreviewManager.status.collectAsState()

    // Per-engine native runtimes
    val nativeLibManager = remember { NativeLibManager(context) }
    val ortStatus by nativeLibManager.status(NativeLibManager.Engine.ORT).collectAsState()
    val moonshineRtStatus by nativeLibManager.status(NativeLibManager.Engine.MOONSHINE).collectAsState()
    val sherpaRtStatus by nativeLibManager.status(NativeLibManager.Engine.SHERPA).collectAsState()

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
            // Card 1: Moonshine Voice
            ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.secondary) {
                Text(
                    text = "Moonshine Voice",
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.Bold,
                )
                NativeRuntimeRow(
                    name = locale.downloader.toolRuntimeMoonshine,
                    status = moonshineRtStatus,
                    locale = locale,
                    helpDesc = locale.downloader.toolRuntimeMoonshineDesc,
                    onDownload = { nativeLibManager.startDownload(NativeLibManager.Engine.MOONSHINE) },
                    onDelete = { nativeLibManager.delete(NativeLibManager.Engine.MOONSHINE) },
                    onHelp = { helpDialog = it },
                )
                NativeRuntimeRow(
                    name = locale.downloader.toolRuntimeOrt,
                    status = ortStatus,
                    locale = locale,
                    helpDesc = locale.downloader.toolRuntimeOrtDesc,
                    onDownload = { nativeLibManager.startDownload(NativeLibManager.Engine.ORT) },
                    onDelete = { nativeLibManager.delete(NativeLibManager.Engine.ORT) },
                    onHelp = { helpDialog = it },
                )
                Text(
                    text = moonshineLanguageSummary(locale),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                for (lang in MoonshineLanguage.entries) {
                    val status = moonshineStatuses[lang] ?: MoonshineModelManager.MoonshineLangStatus.Missing
                    DownloadedToolRow(
                        name = lang.displayName,
                        icon = R.drawable.ms_mic,
                        statusText = when (status) {
                            is MoonshineModelManager.MoonshineLangStatus.Missing ->
                                locale.dlDepsNotInstalled
                            is MoonshineModelManager.MoonshineLangStatus.Downloading ->
                                downloadingStatus(locale, status.progress)
                            is MoonshineModelManager.MoonshineLangStatus.Installed ->
                                installedStatus(locale, status.sizeBytes)
                            is MoonshineModelManager.MoonshineLangStatus.Error ->
                                status.message
                        },
                        statusColor = when (status) {
                            is MoonshineModelManager.MoonshineLangStatus.Installed ->
                                MaterialTheme.colorScheme.tertiary
                            is MoonshineModelManager.MoonshineLangStatus.Error ->
                                MaterialTheme.colorScheme.error
                            else -> MaterialTheme.colorScheme.onSurfaceVariant
                        },
                        accent = MaterialTheme.colorScheme.secondary,
                        onHelpClick = {
                            helpDialog = lang.displayName to moonshineHelpText(locale)
                        },
                        progressFraction = if (status is MoonshineModelManager.MoonshineLangStatus.Downloading) {
                            status.progress
                        } else {
                            null
                        },
                        action = when (status) {
                            is MoonshineModelManager.MoonshineLangStatus.Missing,
                            is MoonshineModelManager.MoonshineLangStatus.Error -> ToolAction(
                                text = locale.dlDepsInstall,
                                role = ToolActionRole.TONAL,
                                onClick = { moonshineManager.startDownloadMoonshine(lang) },
                            )
                            is MoonshineModelManager.MoonshineLangStatus.Installed -> ToolAction(
                                text = locale.downloader.toolDelete,
                                role = ToolActionRole.DESTRUCTIVE,
                                onClick = { moonshineManager.deleteMoonshine(lang) },
                            )
                            else -> null
                        },
                    )
                }
            }

            // Card 2: Zipformer ASR
            ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.secondary) {
                Text(
                    text = "Zipformer ASR",
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.Bold,
                )
                NativeRuntimeRow(
                    name = locale.downloader.toolRuntimeSherpa,
                    status = sherpaRtStatus,
                    locale = locale,
                    helpDesc = locale.downloader.toolRuntimeSherpaDesc,
                    onDownload = { nativeLibManager.startDownload(NativeLibManager.Engine.SHERPA) },
                    onDelete = { nativeLibManager.delete(NativeLibManager.Engine.SHERPA) },
                    onHelp = { helpDialog = it },
                )
                Text(
                    text = zipformerLanguageSummary(locale),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                for (lang in ZipformerLanguage.entries) {
                    val status = zipformerStatuses[lang] ?: MoonshineModelManager.ZipformerLangStatus.Missing
                    DownloadedToolRow(
                        name = lang.displayName,
                        icon = R.drawable.ms_translate,
                        statusText = when (status) {
                            is MoonshineModelManager.ZipformerLangStatus.Missing ->
                                locale.dlDepsNotInstalled
                            is MoonshineModelManager.ZipformerLangStatus.Downloading ->
                                downloadingStatus(locale, status.progress)
                            is MoonshineModelManager.ZipformerLangStatus.Installed ->
                                installedStatus(locale, status.sizeBytes)
                            is MoonshineModelManager.ZipformerLangStatus.Error ->
                                status.message
                        },
                        statusColor = when (status) {
                            is MoonshineModelManager.ZipformerLangStatus.Installed ->
                                MaterialTheme.colorScheme.tertiary
                            is MoonshineModelManager.ZipformerLangStatus.Error ->
                                MaterialTheme.colorScheme.error
                            else -> MaterialTheme.colorScheme.onSurfaceVariant
                        },
                        accent = MaterialTheme.colorScheme.secondary,
                        onHelpClick = {
                            helpDialog = lang.displayName to zipformerHelpText(locale)
                        },
                        progressFraction = if (status is MoonshineModelManager.ZipformerLangStatus.Downloading) {
                            status.progress
                        } else {
                            null
                        },
                        action = when (status) {
                            is MoonshineModelManager.ZipformerLangStatus.Missing,
                            is MoonshineModelManager.ZipformerLangStatus.Error -> ToolAction(
                                text = locale.dlDepsInstall,
                                role = ToolActionRole.TONAL,
                                onClick = { moonshineManager.startDownloadZipformer(lang) },
                            )
                            is MoonshineModelManager.ZipformerLangStatus.Installed -> ToolAction(
                                text = locale.downloader.toolDelete,
                                role = ToolActionRole.DESTRUCTIVE,
                                onClick = { moonshineManager.deleteZipformer(lang) },
                            )
                            else -> null
                        },
                    )
                }
            }

            ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.tertiary) {
                Text(
                    text = locale.creationApps.common.previewTools,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.Bold,
                )
                DownloadedToolRow(
                    name = locale.creationApps.common.depthPreviewModel,
                    icon = R.drawable.ms_layers,
                    statusText = when (val status = depthPreviewStatus) {
                        DepthPreviewModelStatus.Missing -> locale.dlDepsNotInstalled
                        is DepthPreviewModelStatus.Downloading ->
                            downloadingStatus(locale, status.progress)
                        is DepthPreviewModelStatus.Ready ->
                            installedStatus(locale, status.sizeBytes)
                        is DepthPreviewModelStatus.Failed -> status.message
                    },
                    statusColor = when (depthPreviewStatus) {
                        is DepthPreviewModelStatus.Ready -> MaterialTheme.colorScheme.tertiary
                        is DepthPreviewModelStatus.Failed -> MaterialTheme.colorScheme.error
                        else -> MaterialTheme.colorScheme.onSurfaceVariant
                    },
                    accent = MaterialTheme.colorScheme.tertiary,
                    onHelpClick = {
                        helpDialog = locale.creationApps.common.depthPreviewModel to
                            locale.creationApps.common.depthPreviewDescription
                    },
                    progressFraction = (depthPreviewStatus as? DepthPreviewModelStatus.Downloading)
                        ?.progress,
                    action = when (depthPreviewStatus) {
                        DepthPreviewModelStatus.Missing,
                        is DepthPreviewModelStatus.Failed -> ToolAction(
                            text = locale.dlDepsInstall,
                            role = ToolActionRole.TONAL,
                            onClick = depthPreviewManager::startInstall,
                        )
                        is DepthPreviewModelStatus.Ready -> ToolAction(
                            text = locale.downloader.toolDelete,
                            role = ToolActionRole.DESTRUCTIVE,
                            onClick = depthPreviewManager::delete,
                        )
                        is DepthPreviewModelStatus.Downloading -> null
                    },
                )
            }

            // Video downloader tools — sideload-only; stubbed out on the Play flavor.
            DownloaderToolsCard(locale = locale, onHelp = { helpDialog = it })
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

@Composable
private fun NativeRuntimeRow(
    name: String,
    status: NativeLibManager.Status,
    locale: MobileLocaleText,
    helpDesc: String,
    onDownload: () -> Unit,
    onDelete: () -> Unit,
    onHelp: (Pair<String, String>) -> Unit,
) {
    DownloadedToolRow(
        name = name,
        icon = R.drawable.ms_display_settings,
        statusText = when (status) {
            is NativeLibManager.Status.Missing -> locale.dlDepsNotInstalled
            is NativeLibManager.Status.Downloading ->
                downloadingStatus(locale, status.progress)
            is NativeLibManager.Status.Installed ->
                installedStatus(locale, status.sizeBytes)
            is NativeLibManager.Status.Error -> status.message
        },
        statusColor = when (status) {
            is NativeLibManager.Status.Installed -> MaterialTheme.colorScheme.tertiary
            is NativeLibManager.Status.Error -> MaterialTheme.colorScheme.error
            else -> MaterialTheme.colorScheme.onSurfaceVariant
        },
        accent = MaterialTheme.colorScheme.secondary,
        onHelpClick = { onHelp(name to helpDesc) },
        progressFraction = if (status is NativeLibManager.Status.Downloading) {
            status.progress
        } else {
            null
        },
        action = when (status) {
            is NativeLibManager.Status.Missing,
            is NativeLibManager.Status.Error -> ToolAction(
                text = locale.dlDepsInstall,
                role = ToolActionRole.TONAL,
                onClick = onDownload,
            )
            is NativeLibManager.Status.Installed -> ToolAction(
                text = locale.downloader.toolDelete,
                role = ToolActionRole.DESTRUCTIVE,
                onClick = onDelete,
            )
            else -> null
        },
    )
}

private fun downloadingStatus(locale: MobileLocaleText, progress: Float): String =
    "${locale.dlDepsDownloading} (${(progress * 100).toInt()}%)"

private fun installedStatus(locale: MobileLocaleText, sizeBytes: Long): String =
    "${locale.dlDepsReady} (${formatMb(sizeBytes)} MB)"

private fun moonshineLanguageSummary(locale: MobileLocaleText): String = locale.downloader.moonshineLanguageSummary

private fun zipformerLanguageSummary(locale: MobileLocaleText): String = locale.downloader.zipformerLanguageSummary

private fun moonshineHelpText(locale: MobileLocaleText): String = locale.downloader.moonshineHelpText

private fun zipformerHelpText(locale: MobileLocaleText): String = locale.downloader.zipformerHelpText

private fun formatMb(bytes: Long): String = "%.1f".format(bytes / (1024.0 * 1024.0))

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
