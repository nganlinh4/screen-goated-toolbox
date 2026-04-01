@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.downloader.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
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
import dev.screengoated.toolbox.mobile.ui.UtilityActionButton
import dev.screengoated.toolbox.mobile.ui.UtilityExpressiveCard
import dev.screengoated.toolbox.mobile.ui.UtilityHeaderRow
import dev.screengoated.toolbox.mobile.ui.UtilityStatusChip

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
                        Icon(painterResource(R.drawable.ms_arrow_back), contentDescription = null)
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
                            Icon(painterResource(R.drawable.ms_folder), contentDescription = null, modifier = Modifier.size(16.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
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
                                Icon(painterResource(R.drawable.ms_settings), contentDescription = null, modifier = Modifier.size(20.dp))
                            }
                            DropdownMenu(expanded = menuExpanded, onDismissRequest = { menuExpanded = false }) {
                                DropdownMenuItem(
                                    text = { Text(locale.dlChangeFolder) },
                                    leadingIcon = { Icon(painterResource(R.drawable.ms_folder), contentDescription = null, Modifier.size(18.dp)) },
                                    onClick = {
                                        menuExpanded = false
                                        folderPicker.launch(null)
                                    },
                                )
                                DropdownMenuItem(
                                    text = { Text(locale.dlDeleteDeps + " (${viewModel.totalDepsSize()})", color = MaterialTheme.colorScheme.error) },
                                    leadingIcon = { Icon(painterResource(R.drawable.ms_close), contentDescription = null, Modifier.size(18.dp)) },
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
    Box(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
    ) {
        UtilityExpressiveCard(
            accent = MaterialTheme.colorScheme.primary,
            modifier = Modifier
                .align(Alignment.Center)
                .fillMaxWidth()
                .widthIn(max = 460.dp),
        ) {
            UtilityHeaderRow(
                icon = R.drawable.ms_person_play,
                title = locale.dlDepsRequired,
                accent = MaterialTheme.colorScheme.primary,
                supporting = locale.shellDownloadedToolsLabel,
            )
            ToolStatusRow("yt-dlp", ytdlp, locale)
            ToolStatusRow("ffmpeg", ffmpeg, locale)

            when (ytdlp.status) {
                ToolInstallStatus.MISSING, ToolInstallStatus.ERROR -> {
                    if (ytdlp.error != null) {
                        Text(
                            ytdlp.error,
                            color = MaterialTheme.colorScheme.error,
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                    UtilityActionButton(
                        text = locale.dlDepsInstall,
                        accent = MaterialTheme.colorScheme.primary,
                        onClick = onInstall,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Icon(
                            painterResource(R.drawable.ms_download),
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                            tint = MaterialTheme.colorScheme.primary,
                        )
                    }
                }
                ToolInstallStatus.DOWNLOADING, ToolInstallStatus.EXTRACTING -> {
                    LinearWavyProgressIndicator(
                        modifier = Modifier.fillMaxWidth().height(6.dp),
                        trackColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                    )
                    Text(
                        ytdlp.version ?: if (ytdlp.status == ToolInstallStatus.EXTRACTING) {
                            locale.dlDepsExtracting
                        } else {
                            locale.dlDepsDownloading
                        },
                        style = MaterialTheme.typography.bodySmall,
                        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                    )
                }
                ToolInstallStatus.CHECKING -> {
                    LinearWavyProgressIndicator(
                        modifier = Modifier.fillMaxWidth().height(6.dp),
                        trackColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                    )
                    Text(locale.dlDepsChecking, style = MaterialTheme.typography.bodySmall)
                }
                else -> Unit
            }
        }
    }
}

@Composable
private fun ToolStatusRow(name: String, tool: dev.screengoated.toolbox.mobile.downloader.ToolState, locale: dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            name,
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.weight(1f),
        )
        UtilityStatusChip(
            text = when (tool.status) {
                ToolInstallStatus.INSTALLED -> locale.dlDepsReady
                ToolInstallStatus.MISSING -> locale.dlDepsNotInstalled
                ToolInstallStatus.DOWNLOADING -> locale.dlDepsDownloading
                ToolInstallStatus.EXTRACTING -> locale.dlDepsExtracting
                ToolInstallStatus.CHECKING -> locale.dlDepsChecking
                ToolInstallStatus.ERROR -> tool.error ?: locale.dlStatusError
            },
            accent = when (tool.status) {
                ToolInstallStatus.INSTALLED -> MaterialTheme.colorScheme.primary
                ToolInstallStatus.ERROR -> MaterialTheme.colorScheme.error
                else -> MaterialTheme.colorScheme.tertiary
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
                        Icon(painterResource(R.drawable.ms_close), contentDescription = "Close", modifier = Modifier.size(12.dp))
                    }
                },
            )
        }
        IconButton(onClick = onAddTab, modifier = Modifier.size(32.dp)) {
            Icon(painterResource(R.drawable.ms_add), contentDescription = "New tab", modifier = Modifier.size(18.dp))
        }
    }
}
