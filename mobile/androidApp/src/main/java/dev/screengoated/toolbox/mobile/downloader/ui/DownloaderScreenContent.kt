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
import androidx.compose.ui.res.painterResource
import androidx.core.net.toUri
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearWavyProgressIndicator
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.webkit.MimeTypeMap
import androidx.compose.foundation.background
import androidx.core.content.FileProvider
import dev.screengoated.toolbox.mobile.downloader.DownloadPhase
import dev.screengoated.toolbox.mobile.downloader.DownloadType
import dev.screengoated.toolbox.mobile.downloader.DownloaderViewModel
import dev.screengoated.toolbox.mobile.ui.UtilityExpressiveCard
import dev.screengoated.toolbox.mobile.ui.UtilityHeaderRow

@Composable
internal fun SessionContent(
    viewModel: DownloaderViewModel,
    state: dev.screengoated.toolbox.mobile.downloader.DownloaderUiState,
    locale: dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText,
) {
    val session = state.activeSession
    val context = LocalContext.current
    val clipboardManager = remember(context) {
        context.getSystemService(ClipboardManager::class.java)
    }
    val sourceAccent = if (session.downloadType == DownloadType.VIDEO) {
        MaterialTheme.colorScheme.primary
    } else {
        MaterialTheme.colorScheme.tertiary
    }
    val preferredVideoFormat = if (session.downloadType == DownloadType.VIDEO) {
        session.selectedFormat ?: state.settings.lastVideoFormat
    } else {
        null
    }

    android.util.Log.d("SGT-DL-UI", "SessionContent: phase=${session.phase} formats=${session.availableFormats.size} isAnalyzing=${session.isAnalyzing} downloadType=${session.downloadType}")

    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        UtilityExpressiveCard(accent = sourceAccent) {
            UtilityHeaderRow(
                icon = R.drawable.ms_link,
                title = locale.dlUrlLabel,
                accent = sourceAccent,
                supporting = if (session.downloadType == DownloadType.VIDEO) {
                    locale.dlVideoLabel
                } else {
                    locale.dlAudioLabel
                },
            )
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
                                Icon(painterResource(R.drawable.ms_close), contentDescription = downloaderClearLabel(locale), modifier = Modifier.size(20.dp))
                            }
                        }
                        FilledTonalIconButton(
                            onClick = {
                                val clipboardText = clipboardManager
                                    ?.primaryClip
                                    ?.getItemAt(0)
                                    ?.coerceToText(context)
                                    ?.toString()
                                if (!clipboardText.isNullOrBlank()) {
                                    viewModel.updateUrl(clipboardText)
                                }
                            },
                            shape = MaterialShapes.Arch.toShape(),
                            modifier = Modifier
                                .offset(x = (-4).dp)
                                .graphicsLayer { rotationZ = 90f },
                        ) {
                            Icon(
                                painterResource(R.drawable.ms_content_paste),
                                contentDescription = downloaderPasteLabel(locale),
                                modifier = Modifier.graphicsLayer { rotationZ = -90f },
                            )
                        }
                    }
                },
            )

            Row(horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween)) {
                ToggleButton(
                    checked = session.downloadType == DownloadType.VIDEO,
                    onCheckedChange = { viewModel.setDownloadType(DownloadType.VIDEO) },
                    shapes = ButtonGroupDefaults.connectedLeadingButtonShapes(),
                    modifier = Modifier.weight(1f).semantics { role = Role.RadioButton },
                ) {
                    Icon(painterResource(R.drawable.ms_videocam), contentDescription = null, Modifier.size(16.dp))
                    Spacer(Modifier.width(4.dp))
                    Text(locale.dlVideoLabel)
                }
                ToggleButton(
                    checked = session.downloadType == DownloadType.AUDIO,
                    onCheckedChange = { viewModel.setDownloadType(DownloadType.AUDIO) },
                    shapes = ButtonGroupDefaults.connectedTrailingButtonShapes(),
                    modifier = Modifier.weight(1f).semantics { role = Role.RadioButton },
                ) {
                    Icon(painterResource(R.drawable.ms_music_note), contentDescription = null, Modifier.size(16.dp))
                    Spacer(Modifier.width(4.dp))
                    Text(locale.dlAudioLabel)
                }
            }

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
                Text(
                    session.analysisError,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }

        // Quality + Subtitle row
        val hasQuality = session.downloadType == DownloadType.VIDEO &&
            (session.availableFormats.isNotEmpty() || preferredVideoFormat != null)
        val hasSubs = state.settings.useSubtitles && session.availableSubtitles.isNotEmpty()
        if (hasQuality || hasSubs) {
            UtilityExpressiveCard(accent = MaterialTheme.colorScheme.secondary) {
                UtilityHeaderRow(
                    icon = R.drawable.ms_tune,
                    title = locale.dlAdvanced,
                    accent = MaterialTheme.colorScheme.secondary,
                    supporting = listOfNotNull(
                        if (hasQuality) locale.dlQualityLabel else null,
                        if (hasSubs) locale.dlSubtitleLabel else null,
                    ).joinToString(" • "),
                )
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    if (hasQuality) {
                        Box(Modifier.weight(1f)) {
                            val qualityOptions = (listOf(locale.dlBest) +
                                listOfNotNull(preferredVideoFormat) +
                                session.availableFormats).distinct()
                            DropdownSelector(
                                label = locale.dlQualityLabel,
                                options = qualityOptions,
                                selected = preferredVideoFormat ?: locale.dlBest,
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
            }
        } else if (
            state.settings.useSubtitles &&
            !session.isAnalyzing &&
            session.inputUrl.isNotBlank() &&
            session.lastUrlAnalyzed.isNotBlank()
        ) {
            UtilityExpressiveCard(accent = MaterialTheme.colorScheme.secondary) {
                Text(
                    locale.dlSubsNoneFound,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }

        UtilityExpressiveCard(accent = MaterialTheme.colorScheme.tertiary) {
            AdvancedSection(
                settings = state.settings,
                locale = locale,
                onUpdate = { viewModel.updateSettings(it) },
            )
        }

        Spacer(Modifier.height(4.dp))

        // Action area
        when (session.phase) {
            DownloadPhase.IDLE, DownloadPhase.ANALYZING -> {
                Button(
                    onClick = { viewModel.startDownload() },
                    enabled = session.inputUrl.isNotBlank(),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Icon(painterResource(R.drawable.ms_download), contentDescription = null, Modifier.size(18.dp))
                    Spacer(Modifier.width(8.dp))
                    Text(
                        if (session.isAnalyzing) {
                            downloaderStartNowText(locale, preferredVideoFormat)
                        } else {
                            locale.dlStartBtn
                        },
                    )
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
                        Icon(painterResource(R.drawable.ms_stop), contentDescription = null, Modifier.size(16.dp))
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
                                    painterResource(R.drawable.ms_check),
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
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            FilledTonalButton(
                                onClick = {
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
                                            val uri = session.finishedFileUri?.toUri()
                                                ?: FileProvider.getUriForFile(
                                                    context,
                                                    "${context.packageName}.fileprovider",
                                                    file,
                                                )
                                            val intent = Intent(Intent.ACTION_VIEW).apply {
                                                setDataAndType(uri, mime)
                                                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                                            }
                                            context.startActivity(intent)
                                        } catch (_: Exception) {
                                            android.widget.Toast
                                                .makeText(context, downloaderOpenFileFailedText(locale), android.widget.Toast.LENGTH_SHORT)
                                                .show()
                                        }
                                    }
                                },
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Icon(painterResource(R.drawable.ms_open_in_new), contentDescription = null, Modifier.size(16.dp))
                                Spacer(Modifier.width(4.dp))
                                Text(locale.dlOpenFile)
                            }
                            FilledTonalButton(
                                onClick = {
                                    session.finishedFilePath?.let { path ->
                                        val copied = runCatching {
                                            val file = java.io.File(path)
                                            val uri = session.finishedFileUri?.toUri()
                                                ?: FileProvider.getUriForFile(
                                                    context,
                                                    "${context.packageName}.fileprovider",
                                                    file,
                                                )
                                            val clip = ClipData.newUri(context.contentResolver, "SGT Video", uri)
                                            clipboardManager?.setPrimaryClip(clip) ?: error("Clipboard unavailable")
                                        }.isSuccess
                                        android.widget.Toast
                                            .makeText(
                                                context,
                                                if (copied) {
                                                    downloaderCopyVideoDoneText(locale)
                                                } else {
                                                    downloaderCopyVideoFailedText(locale)
                                                },
                                                android.widget.Toast.LENGTH_SHORT,
                                            )
                                            .show()
                                    }
                                },
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Icon(painterResource(R.drawable.ms_content_copy), contentDescription = null, Modifier.size(16.dp))
                                Spacer(Modifier.width(4.dp))
                                Text(downloaderCopyVideoLabel(locale))
                            }
                            FilledTonalButton(
                                onClick = {
                                    session.finishedFilePath?.let { path ->
                                        val folder = java.io.File(path).parentFile ?: return@let
                                        val storagePath = folder.absolutePath
                                            .removePrefix("/storage/emulated/0/")
                                            .replace("/", "%2F")
                                        val docUri =
                                            "content://com.android.externalstorage.documents/document/primary%3A$storagePath".toUri()
                                        val opened = runCatching {
                                            val intent = Intent(Intent.ACTION_VIEW).apply {
                                                setDataAndType(docUri, "vnd.android.document/directory")
                                                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                                            }
                                            context.startActivity(intent)
                                        }.recoverCatching {
                                                val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
                                                    putExtra(
                                                        android.provider.DocumentsContract.EXTRA_INITIAL_URI,
                                                        docUri,
                                                    )
                                                }
                                                context.startActivity(intent)
                                        }.isSuccess
                                        if (!opened) {
                                            android.widget.Toast
                                                .makeText(context, downloaderOpenFolderFailedText(locale), android.widget.Toast.LENGTH_SHORT)
                                                .show()
                                        }
                                    }
                                },
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Icon(painterResource(R.drawable.ms_folder), contentDescription = null, Modifier.size(16.dp))
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
}

