@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.downloader.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.Folder
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearWavyProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import dev.screengoated.toolbox.mobile.downloader.DownloaderViewModel
import dev.screengoated.toolbox.mobile.downloader.ToolInstallStatus

@Composable
fun DownloaderScreen(
    viewModel: DownloaderViewModel,
    locale: dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText,
    onBack: () -> Unit,
) {
    val state by viewModel.state.collectAsState()
    val folderPicker = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocumentTree(),
    ) { uri ->
        if (uri != null) {
            val path = uri.path?.replace("/tree/primary:", "/storage/emulated/0/")
            viewModel.setDownloadPath(path)
        }
    }

    val focusManager = androidx.compose.ui.platform.LocalFocusManager.current
    Scaffold(
        modifier = Modifier.pointerInput(Unit) {
            detectTapGestures(onTap = { focusManager.clearFocus() })
        },
        topBar = {
            TopAppBar(
                title = {},
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Rounded.ArrowBack, contentDescription = null)
                    }
                },
                actions = {
                    if (state.toolsReady) {
                        val dlDisplayPath = state.settings.customDownloadPath ?: "Downloads/SGT"
                        val context = androidx.compose.ui.platform.LocalContext.current
                        val actualDir = remember(dlDisplayPath) { viewModel.getDownloadDir() }
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier.clickable {
                                try {
                                    val storagePath = actualDir.absolutePath
                                        .removePrefix("/storage/emulated/0/")
                                        .replace("/", "%2F")
                                    val docUri = android.net.Uri.parse(
                                        "content://com.android.externalstorage.documents/document/primary%3A$storagePath"
                                    )
                                    val intent = android.content.Intent(android.content.Intent.ACTION_VIEW).apply {
                                        setDataAndType(docUri, "vnd.android.document/directory")
                                        addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
                                    }
                                    context.startActivity(intent)
                                } catch (_: Exception) {}
                            },
                        ) {
                            Icon(Icons.Rounded.Folder, contentDescription = null, modifier = Modifier.size(16.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                            Spacer(Modifier.width(4.dp))
                            Text(
                                text = dlDisplayPath,
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.widthIn(max = 160.dp),
                            )
                        }
                        Spacer(Modifier.width(4.dp))
                        var menuExpanded by remember { mutableStateOf(false) }
                        Box {
                            IconButton(onClick = { menuExpanded = true }) {
                                Icon(Icons.Rounded.Settings, contentDescription = null, modifier = Modifier.size(20.dp))
                            }
                            DropdownMenu(expanded = menuExpanded, onDismissRequest = { menuExpanded = false }) {
                                DropdownMenuItem(
                                    text = { Text(locale.dlChangeFolder) },
                                    leadingIcon = { Icon(Icons.Rounded.Folder, contentDescription = null, Modifier.size(18.dp)) },
                                    onClick = {
                                        menuExpanded = false
                                        folderPicker.launch(null)
                                    },
                                )
                                DropdownMenuItem(
                                    text = { Text(locale.dlDeleteDeps + " (~80 MB)", color = MaterialTheme.colorScheme.error) },
                                    leadingIcon = { Icon(Icons.Rounded.Close, contentDescription = null, Modifier.size(18.dp)) },
                                    onClick = {
                                        menuExpanded = false
                                        viewModel.deleteTools()
                                    },
                                )
                            }
                        }
                    }
                },
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(horizontal = 16.dp),
        ) {
            if (!state.toolsReady) {
                // Tool install UI
                ToolInstallSection(
                    ytdlp = state.ytdlp,
                    ffmpeg = state.ffmpeg,
                    locale = locale,
                    onInstall = { viewModel.installTools() },
                )
            } else {
                // Tab strip
                TabStrip(
                    tabs = state.sessions.map { it.tabName },
                    activeIndex = state.activeTabIndex,
                    onTabClick = { viewModel.switchTab(it) },
                    onTabClose = { viewModel.closeTab(it) },
                    onAddTab = { viewModel.addTab() },
                )

                HorizontalDivider(modifier = Modifier.padding(vertical = 4.dp))

                // Scrollable session content
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    SessionContent(viewModel = viewModel, state = state, locale = locale)
                    Spacer(Modifier.height(16.dp))
                }
            }
        }
    }
}

@Composable
private fun ToolInstallSection(
    ytdlp: dev.screengoated.toolbox.mobile.downloader.ToolState,
    ffmpeg: dev.screengoated.toolbox.mobile.downloader.ToolState,
    locale: dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText,
    onInstall: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(Icons.Rounded.Download, contentDescription = null, modifier = Modifier.size(64.dp))
        Spacer(Modifier.height(16.dp))
        Text(locale.dlDepsRequired, style = MaterialTheme.typography.titleLarge)
        Spacer(Modifier.height(24.dp))

        // yt-dlp status
        ToolStatusRow("yt-dlp", ytdlp, locale)
        Spacer(Modifier.height(8.dp))
        ToolStatusRow("ffmpeg", ffmpeg, locale)
        Spacer(Modifier.height(24.dp))

        when (ytdlp.status) {
            ToolInstallStatus.MISSING, ToolInstallStatus.ERROR -> {
                if (ytdlp.error != null) {
                    Text(ytdlp.error, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
                    Spacer(Modifier.height(8.dp))
                }
                Button(onClick = onInstall, modifier = Modifier.fillMaxWidth(0.7f)) {
                    Text(locale.dlDepsInstall)
                }
            }
            ToolInstallStatus.DOWNLOADING, ToolInstallStatus.EXTRACTING -> {
                LinearWavyProgressIndicator(
                    modifier = Modifier.fillMaxWidth(0.7f).height(6.dp),
                    trackColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    ytdlp.version ?: if (ytdlp.status == ToolInstallStatus.EXTRACTING) locale.dlDepsExtracting else locale.dlDepsDownloading,
                    style = MaterialTheme.typography.bodySmall,
                    textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                )
            }
            ToolInstallStatus.CHECKING -> {
                LinearWavyProgressIndicator(
                    modifier = Modifier.fillMaxWidth(0.7f).height(6.dp),
                    trackColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                )
                Spacer(Modifier.height(8.dp))
                Text(locale.dlDepsChecking, style = MaterialTheme.typography.bodySmall)
            }
            else -> {}
        }
    }
}

@Composable
private fun ToolStatusRow(name: String, tool: dev.screengoated.toolbox.mobile.downloader.ToolState, locale: dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText) {
    Row(
        modifier = Modifier.fillMaxWidth(0.7f),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(name, style = MaterialTheme.typography.bodyMedium)
        Text(
            when (tool.status) {
                ToolInstallStatus.INSTALLED -> locale.dlDepsReady
                ToolInstallStatus.MISSING -> locale.dlDepsNotInstalled
                ToolInstallStatus.DOWNLOADING -> locale.dlDepsDownloading
                ToolInstallStatus.EXTRACTING -> locale.dlDepsExtracting
                ToolInstallStatus.CHECKING -> locale.dlDepsChecking
                ToolInstallStatus.ERROR -> tool.error ?: locale.dlStatusError
            },
            style = MaterialTheme.typography.bodySmall,
            color = if (tool.status == ToolInstallStatus.INSTALLED) {
                MaterialTheme.colorScheme.primary
            } else if (tool.status == ToolInstallStatus.ERROR) {
                MaterialTheme.colorScheme.error
            } else {
                MaterialTheme.colorScheme.onSurfaceVariant
            },
        )
    }
}

@Composable
private fun TabStrip(
    tabs: List<String>,
    activeIndex: Int,
    onTabClick: (Int) -> Unit,
    onTabClose: (Int) -> Unit,
    onAddTab: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        tabs.forEachIndexed { idx, name ->
            FilterChip(
                selected = idx == activeIndex,
                onClick = { onTabClick(idx) },
                label = { Text(name, style = MaterialTheme.typography.labelSmall) },
                trailingIcon = {
                    IconButton(
                        onClick = { onTabClose(idx) },
                        modifier = Modifier.size(16.dp),
                    ) {
                        Icon(Icons.Rounded.Close, contentDescription = "Close", modifier = Modifier.size(12.dp))
                    }
                },
            )
        }
        IconButton(onClick = onAddTab, modifier = Modifier.size(32.dp)) {
            Icon(Icons.Rounded.Add, contentDescription = "New tab", modifier = Modifier.size(18.dp))
        }
    }
}
