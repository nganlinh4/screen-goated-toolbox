@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.downloader.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.ContentPaste
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.ExpandLess
import androidx.compose.material.icons.rounded.ExpandMore
import androidx.compose.material.icons.rounded.Folder
import androidx.compose.material.icons.rounded.MusicNote
import androidx.compose.material.icons.rounded.OpenInNew
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material.icons.rounded.Videocam
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearWavyProgressIndicator
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.material3.toShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import android.content.Intent
import android.webkit.MimeTypeMap
import androidx.compose.foundation.background
import androidx.core.content.FileProvider
import dev.screengoated.toolbox.mobile.downloader.DownloadPhase
import dev.screengoated.toolbox.mobile.downloader.DownloadType
import dev.screengoated.toolbox.mobile.downloader.DownloaderSettings
import dev.screengoated.toolbox.mobile.downloader.DownloaderViewModel

@Composable
internal fun FolderBar(
    path: String,
    changeFolderLabel: String,
    deleteDepsLabel: String,
    onChangeFolder: () -> Unit,
    onDeleteDeps: () -> Unit,
    depsSize: String,
) {
    var menuExpanded by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Rounded.Folder, contentDescription = null, modifier = Modifier.size(18.dp))
        Spacer(Modifier.width(6.dp))
        Text(
            path,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.weight(1f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Box {
            IconButton(onClick = { menuExpanded = true }, modifier = Modifier.size(28.dp)) {
                Icon(Icons.Rounded.Settings, contentDescription = "Settings", modifier = Modifier.size(16.dp))
            }
            DropdownMenu(expanded = menuExpanded, onDismissRequest = { menuExpanded = false }) {
                DropdownMenuItem(
                    text = { Text(changeFolderLabel) },
                    leadingIcon = { Icon(Icons.Rounded.Folder, contentDescription = null, Modifier.size(18.dp)) },
                    onClick = {
                        menuExpanded = false
                        onChangeFolder()
                    },
                )
                DropdownMenuItem(
                    text = { Text("$deleteDepsLabel ($depsSize)", color = MaterialTheme.colorScheme.error) },
                    leadingIcon = { Icon(Icons.Rounded.Close, contentDescription = null, Modifier.size(18.dp)) },
                    onClick = {
                        menuExpanded = false
                        onDeleteDeps()
                    },
                )
            }
        }
    }
}

@Composable
internal fun SessionContent(
    viewModel: DownloaderViewModel,
    state: dev.screengoated.toolbox.mobile.downloader.DownloaderUiState,
    locale: dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText,
) {
    val session = state.activeSession
    val clipboard = LocalClipboardManager.current

    android.util.Log.d("SGT-DL-UI", "SessionContent: phase=${session.phase} formats=${session.availableFormats.size} isAnalyzing=${session.isAnalyzing} downloadType=${session.downloadType}")

    // URL input
    OutlinedTextField(
        modifier = Modifier.fillMaxWidth(),
        value = session.inputUrl,
        onValueChange = { viewModel.updateUrl(it) },
        label = { Text(locale.dlUrlLabel) },
        placeholder = { Text("https://youtube.com/watch?v=...") },
        singleLine = true,
        shape = MaterialTheme.shapes.large,
        trailingIcon = {
            Row {
                if (session.inputUrl.isNotEmpty()) {
                    IconButton(onClick = { viewModel.updateUrl("") }) {
                        Icon(Icons.Rounded.Close, contentDescription = "Clear", modifier = Modifier.size(20.dp))
                    }
                }
                FilledTonalIconButton(
                    onClick = { clipboard.getText()?.text?.let { viewModel.updateUrl(it) } },
                    shape = MaterialShapes.Arch.toShape(),
                    modifier = Modifier
                        .offset(x = (-4).dp)
                        .graphicsLayer { rotationZ = 90f },
                ) {
                    Icon(
                        Icons.Rounded.ContentPaste,
                        contentDescription = "Paste",
                        modifier = Modifier.graphicsLayer { rotationZ = -90f },
                    )
                }
            }
        },
    )

    // Video / Audio toggle
    Row(horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween)) {
        ToggleButton(
            checked = session.downloadType == DownloadType.VIDEO,
            onCheckedChange = { viewModel.setDownloadType(DownloadType.VIDEO) },
            shapes = ButtonGroupDefaults.connectedLeadingButtonShapes(),
            modifier = Modifier.weight(1f).semantics { role = Role.RadioButton },
        ) {
            Icon(Icons.Rounded.Videocam, contentDescription = null, Modifier.size(16.dp))
            Spacer(Modifier.width(4.dp))
            Text(locale.dlVideoLabel)
        }
        ToggleButton(
            checked = session.downloadType == DownloadType.AUDIO,
            onCheckedChange = { viewModel.setDownloadType(DownloadType.AUDIO) },
            shapes = ButtonGroupDefaults.connectedTrailingButtonShapes(),
            modifier = Modifier.weight(1f).semantics { role = Role.RadioButton },
        ) {
            Icon(Icons.Rounded.MusicNote, contentDescription = null, Modifier.size(16.dp))
            Spacer(Modifier.width(4.dp))
            Text(locale.dlAudioLabel)
        }
    }

    // Analysis status — below toggle, persists during download
    if (session.isAnalyzing) {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            LinearWavyProgressIndicator(
                modifier = Modifier.fillMaxWidth().height(4.dp),
                trackColor = MaterialTheme.colorScheme.surfaceContainerHighest,
            )
            Text(locale.dlScanning, style = MaterialTheme.typography.bodySmall)
        }
    }
    if (session.analysisError != null && session.phase != DownloadPhase.DOWNLOADING) {
        Text(session.analysisError, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
    }

    // Quality + Subtitle row
    val hasQuality = session.downloadType == DownloadType.VIDEO && session.availableFormats.isNotEmpty()
    val hasSubs = state.settings.useSubtitles && session.availableSubtitles.isNotEmpty()
    if (hasQuality || hasSubs) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (hasQuality) {
                Box(Modifier.weight(1f)) {
                    DropdownSelector(
                        label = locale.dlQualityLabel,
                        options = listOf(locale.dlBest) + session.availableFormats,
                        selected = session.selectedFormat ?: locale.dlBest,
                        onSelect = { viewModel.setFormat(if (it == locale.dlBest) null else it) },
                    )
                }
            }
            if (hasSubs) {
                Box(Modifier.weight(1f)) {
                    DropdownSelector(
                        label = locale.dlSubtitleLabel,
                        options = listOf(locale.dlAuto) + session.availableSubtitles,
                        selected = session.selectedSubtitle ?: locale.dlAuto,
                        onSelect = { viewModel.setSubtitle(if (it == locale.dlAuto) null else it) },
                    )
                }
            }
        }
    } else if (state.settings.useSubtitles && !session.isAnalyzing && session.inputUrl.isNotBlank() && session.lastUrlAnalyzed.isNotBlank()) {
        Text(
            locale.dlSubsNoneFound,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }

    // Advanced settings
    AdvancedSection(settings = state.settings, locale = locale, onUpdate = { viewModel.updateSettings(it) })

    Spacer(Modifier.height(4.dp))

    // Action area
    when (session.phase) {
        DownloadPhase.IDLE, DownloadPhase.ANALYZING -> {
            Button(
                onClick = { viewModel.startDownload() },
                enabled = session.inputUrl.isNotBlank(),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Icon(Icons.Rounded.Download, contentDescription = null, Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Text(if (session.isAnalyzing) locale.dlStartNowBtn else locale.dlStartBtn)
            }
        }
        DownloadPhase.DOWNLOADING -> {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                val fraction = session.progress.fraction
                if (fraction <= 0f || fraction >= 1f) {
                    // Indeterminate when stuck at 0% (starting) or 100% (merging/postprocessing)
                    LinearWavyProgressIndicator(
                        modifier = Modifier.fillMaxWidth().height(6.dp),
                        trackColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                    )
                } else {
                    LinearWavyProgressIndicator(
                        progress = { fraction },
                        modifier = Modifier.fillMaxWidth().height(6.dp),
                        trackColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                    )
                }
                Text(
                    session.progress.statusMessage.ifBlank { locale.dlStatusStarting },
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                OutlinedButton(
                    onClick = { viewModel.cancelDownload() },
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Icon(Icons.Rounded.Stop, contentDescription = null, Modifier.size(16.dp))
                    Spacer(Modifier.width(4.dp))
                    Text(locale.dlCancel)
                }
            }
        }
        DownloadPhase.FINISHED -> {
            Card(
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.tertiaryContainer),
                shape = MaterialTheme.shapes.large,
            ) {
                Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        // Sunny-shaped success badge
                        Box(
                            contentAlignment = Alignment.Center,
                            modifier = Modifier
                                .size(32.dp)
                                .background(
                                    color = MaterialTheme.colorScheme.tertiary,
                                    shape = MaterialShapes.Sunny.toShape(),
                                ),
                        ) {
                            Icon(
                                Icons.Rounded.Check,
                                contentDescription = null,
                                modifier = Modifier.size(18.dp),
                                tint = MaterialTheme.colorScheme.onTertiary,
                            )
                        }
                        Text(locale.dlStatusFinished, style = MaterialTheme.typography.titleSmall)
                    }
                    if (session.finishedFilePath != null) {
                        Text(
                            session.finishedFilePath.substringAfterLast('/'),
                            style = MaterialTheme.typography.bodySmall,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    val context = androidx.compose.ui.platform.LocalContext.current
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        FilledTonalButton(onClick = {
                            session.finishedFilePath?.let { path ->
                                try {
                                    val file = java.io.File(path)
                                    val ext = file.extension.lowercase()
                                    val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext)
                                        ?: when (ext) {
                                            "mp4", "mkv", "webm" -> "video/*"
                                            "mp3", "m4a", "opus", "ogg" -> "audio/*"
                                            else -> "*/*"
                                        }
                                    val uri = FileProvider.getUriForFile(
                                        context,
                                        "${context.packageName}.fileprovider",
                                        file,
                                    )
                                    val intent = Intent(Intent.ACTION_VIEW).apply {
                                        setDataAndType(uri, mime)
                                        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                                    }
                                    context.startActivity(intent)
                                } catch (_: Exception) {}
                            }
                        }) {
                            Icon(Icons.Rounded.OpenInNew, contentDescription = null, Modifier.size(16.dp))
                            Spacer(Modifier.width(4.dp))
                            Text(locale.dlOpenFile)
                        }
                        FilledTonalButton(onClick = {
                            session.finishedFilePath?.let { path ->
                                val folder = java.io.File(path).parentFile ?: return@let
                                // Build DocumentsContract URI for the folder
                                val storagePath = folder.absolutePath
                                    .removePrefix("/storage/emulated/0/")
                                    .replace("/", "%2F")
                                val docUri = android.net.Uri.parse(
                                    "content://com.android.externalstorage.documents/document/primary%3A$storagePath"
                                )
                                try {
                                    val intent = Intent(Intent.ACTION_VIEW).apply {
                                        setDataAndType(docUri, "vnd.android.document/directory")
                                        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                                    }
                                    context.startActivity(intent)
                                } catch (_: Exception) {
                                    // Fallback: open system file manager root
                                    try {
                                        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
                                            putExtra(
                                                android.provider.DocumentsContract.EXTRA_INITIAL_URI,
                                                docUri,
                                            )
                                        }
                                        context.startActivity(intent)
                                    } catch (_: Exception) {}
                                }
                            }
                        }) {
                            Icon(Icons.Rounded.Folder, contentDescription = null, Modifier.size(16.dp))
                            Spacer(Modifier.width(4.dp))
                            Text(locale.dlOpenFolder)
                        }
                    }
                }
            }
        }
        DownloadPhase.ERROR -> {
            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer)) {
                Column(Modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(
                        session.errorMessage ?: "Download failed",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onErrorContainer,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(onClick = { viewModel.startDownload() }) { Text(locale.dlRetry) }
                        OutlinedButton(onClick = { viewModel.toggleErrorLog() }) {
                            Text(if (session.showErrorLog) locale.dlHideLog else locale.dlShowLog)
                        }
                    }
                    AnimatedVisibility(visible = session.showErrorLog) {
                        Text(
                            session.logs.joinToString("\n"),
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.height(120.dp).verticalScroll(rememberScrollState()),
                        )
                    }
                }
            }
        }
    }
}

@Composable
internal fun DropdownSelector(
    label: String,
    options: List<String>,
    selected: String,
    onSelect: (String) -> Unit,
    header: String? = null,
) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        OutlinedButton(onClick = { expanded = true }) {
            Text("$label $selected", style = MaterialTheme.typography.bodySmall)
            Spacer(Modifier.width(4.dp))
            Icon(
                if (expanded) Icons.Rounded.ExpandLess else Icons.Rounded.ExpandMore,
                contentDescription = null,
                Modifier.size(16.dp),
            )
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            if (header != null) {
                DropdownMenuItem(
                    text = {
                        Text(
                            header,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    },
                    onClick = {},
                    enabled = false,
                )
                HorizontalDivider()
            }
            options.forEach { opt ->
                DropdownMenuItem(
                    text = { Text(opt) },
                    onClick = { onSelect(opt); expanded = false },
                )
            }
        }
    }
}

@Composable
internal fun AdvancedSection(
    settings: DownloaderSettings,
    locale: dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText,
    onUpdate: ((DownloaderSettings) -> DownloaderSettings) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    Column {
        OutlinedButton(onClick = { expanded = !expanded }) {
            Icon(Icons.Rounded.Settings, contentDescription = null, Modifier.size(14.dp))
            Spacer(Modifier.width(4.dp))
            Text(locale.dlAdvanced, style = MaterialTheme.typography.labelSmall)
            Spacer(Modifier.width(2.dp))
            Icon(
                if (expanded) Icons.Rounded.ExpandLess else Icons.Rounded.ExpandMore,
                contentDescription = null,
                Modifier.size(14.dp),
            )
        }
        AnimatedVisibility(visible = expanded) {
            Column(Modifier.padding(top = 8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                ToggleRow(locale.dlOptMetadata, settings.useMetadata) { onUpdate { s -> s.copy(useMetadata = it) } }
                ToggleRow(locale.dlOptSponsorblock, settings.useSponsorBlock) { onUpdate { s -> s.copy(useSponsorBlock = it) } }
                ToggleRow(locale.dlOptSubtitles, settings.useSubtitles) { onUpdate { s -> s.copy(useSubtitles = it) } }
                ToggleRow(locale.dlOptPlaylist, settings.usePlaylist) { onUpdate { s -> s.copy(usePlaylist = it) } }
            }
        }
    }
}

@Composable
internal fun ToggleRow(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, style = MaterialTheme.typography.bodySmall, modifier = Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}
