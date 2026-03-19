@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class, ExperimentalTextApi::class, androidx.compose.animation.ExperimentalSharedTransitionApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.HelpOutline
import androidx.compose.material.icons.automirrored.rounded.Note
import androidx.compose.material.icons.automirrored.rounded.TextSnippet
import androidx.compose.material.icons.automirrored.rounded.VolumeUp
import androidx.compose.material.icons.rounded.Apps
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.AutoFixHigh
import androidx.compose.material.icons.rounded.SwapHoriz
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material.icons.rounded.CameraAlt
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Star
import androidx.compose.material.icons.rounded.StarOutline
import androidx.compose.material.icons.rounded.ContentCut
import androidx.compose.material.icons.rounded.Description
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.material.icons.rounded.FiberSmartRecord
import androidx.compose.material.icons.rounded.FormatQuote
import androidx.compose.material.icons.rounded.GTranslate
import androidx.compose.material.icons.rounded.Gamepad
import androidx.compose.material.icons.rounded.Image
import androidx.compose.material.icons.rounded.Keyboard
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.GridView
import androidx.compose.material.icons.rounded.Hearing
import androidx.compose.material.icons.rounded.History
import androidx.compose.material.icons.rounded.ImageSearch
import androidx.compose.material.icons.rounded.Keyboard
import androidx.compose.material.icons.rounded.Lightbulb
import androidx.compose.material.icons.rounded.Mic
import androidx.compose.material.icons.rounded.PhotoCamera
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.QrCodeScanner
import androidx.compose.material.icons.rounded.QuestionAnswer
import androidx.compose.material.icons.rounded.RecordVoiceOver
import androidx.compose.material.icons.rounded.School
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.SmartToy
import androidx.compose.material.icons.rounded.Spellcheck
import androidx.compose.material.icons.rounded.SpeakerPhone
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material.icons.rounded.Summarize
import androidx.compose.material.icons.rounded.TableChart
import androidx.compose.material.icons.rounded.TextFields
import androidx.compose.material.icons.rounded.Translate
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material.icons.rounded.VoiceChat
import androidx.compose.material.icons.rounded.Verified
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FloatingActionButtonMenu
import androidx.compose.material3.FloatingActionButtonMenuItem
import androidx.compose.material3.IconButton
import androidx.compose.material3.ToggleFloatingActionButton
import androidx.compose.material3.ToggleFloatingActionButtonDefaults.animateIcon
import androidx.compose.material3.animateFloatingActionButton
import androidx.compose.material3.HorizontalFloatingToolbar
import androidx.compose.material3.carousel.HorizontalUncontainedCarousel
import androidx.compose.material3.carousel.rememberCarouselState
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.material3.WideNavigationRail
import androidx.compose.material3.WideNavigationRailItem
import androidx.compose.material3.WideNavigationRailValue
import androidx.compose.material3.rememberWideNavigationRailState
import androidx.compose.material3.toPath
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import kotlinx.coroutines.launch
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Matrix
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.RoundedPolygon
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

internal enum class MobileShellSection(val icon: ImageVector) {
    APPS(Icons.Rounded.GridView),
    TOOLS(Icons.Rounded.Apps),
    SETTINGS(Icons.Rounded.Settings),
    HISTORY(Icons.Rounded.History);

    fun label(locale: MobileLocaleText): String = when (this) {
        APPS -> locale.shellAppsLabel
        TOOLS -> locale.shellToolsLabel
        SETTINGS -> locale.shellSettingsLabel
        HISTORY -> locale.shellHistoryLabel
    }
}

@Composable
internal fun SectionSegmentedRow(
    selectedSection: MobileShellSection,
    onSectionSelected: (MobileShellSection) -> Unit,
    locale: MobileLocaleText,
    modifier: Modifier = Modifier,
    pagerState: androidx.compose.foundation.pager.PagerState? = null,
) {
    val sections = MobileShellSection.entries
    val activeBg = MaterialTheme.colorScheme.secondaryContainer
    val inactiveBg = Color.Transparent
    val activeContent = MaterialTheme.colorScheme.onSecondaryContainer
    val inactiveContent = MaterialTheme.colorScheme.onSurfaceVariant

    HorizontalFloatingToolbar(
        expanded = true,
        modifier = modifier,
        content = {
            sections.forEachIndexed { index, section ->
                // Calculate per-tab activation fraction (0.0 = inactive, 1.0 = fully active)
                // Tracks pager scroll position in real-time during swipes
                val fraction = if (pagerState != null && pagerState.isScrollInProgress) {
                    val page = pagerState.currentPage
                    val offset = pagerState.currentPageOffsetFraction
                    when (index) {
                        page -> (1f - kotlin.math.abs(offset)).coerceIn(0f, 1f)
                        page + 1 -> offset.coerceIn(0f, 1f)
                        page - 1 -> (-offset).coerceIn(0f, 1f)
                        else -> 0f
                    }
                } else {
                    if (selectedSection == section) 1f else 0f
                }

                val bgColor = androidx.compose.ui.graphics.lerp(inactiveBg, activeBg, fraction)
                val contentColor = androidx.compose.ui.graphics.lerp(inactiveContent, activeContent, fraction)

                val isActive = fraction > 0.5f
                androidx.compose.material3.Surface(
                    onClick = { onSectionSelected(section) },
                    color = bgColor,
                    contentColor = contentColor,
                    shape = MaterialTheme.shapes.large,
                ) {
                    Row(
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                        horizontalArrangement = Arrangement.Center,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        androidx.compose.animation.AnimatedVisibility(
                            visible = isActive,
                            enter = androidx.compose.animation.fadeIn() +
                                androidx.compose.animation.expandHorizontally(),
                            exit = androidx.compose.animation.fadeOut() +
                                androidx.compose.animation.shrinkHorizontally(),
                        ) {
                            Row {
                                Icon(
                                    section.icon,
                                    contentDescription = null,
                                    modifier = Modifier.size(18.dp),
                                )
                                Spacer(Modifier.width(6.dp))
                            }
                        }
                        Text(
                            text = section.label(locale),
                            maxLines = 1,
                            style = MaterialTheme.typography.labelLarge.copy(
                                fontFamily = condensedFontSteps[2].second,
                            ),
                            fontWeight = if (isActive) FontWeight.Bold else FontWeight.Medium,
                            softWrap = false,
                        )
                    }
                }
            }
        },
    )
}

@Composable
internal fun ShellRail(
    selectedSection: MobileShellSection,
    onSectionSelected: (MobileShellSection) -> Unit,
    locale: MobileLocaleText,
    modifier: Modifier = Modifier,
) {
    val railState = rememberWideNavigationRailState(WideNavigationRailValue.Expanded)
    Card(
        modifier = modifier.width(220.dp),
        shape = MaterialTheme.shapes.extraLarge,
    ) {
        WideNavigationRail(
            state = railState,
            modifier = Modifier.fillMaxHeight(),
            header = {
                Column(
                    modifier = Modifier.padding(horizontal = 18.dp, vertical = ShellSpacing.innerPad),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(
                        text = locale.shellSectionTitle,
                        style = MaterialTheme.typography.labelLargeEmphasized,
                    )
                    Text(
                        text = locale.shellCurrentSectionLabel,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            },
        ) {
            MobileShellSection.entries.forEach { section ->
                ShellRailItem(
                    selected = selectedSection == section,
                    onClick = { onSectionSelected(section) },
                    icon = section.icon,
                    label = section.label(locale),
                    description = when (section) {
                        MobileShellSection.APPS -> locale.shellAppsDescription
                        MobileShellSection.TOOLS -> locale.shellToolsDescription
                        MobileShellSection.SETTINGS -> locale.shellSettingsDescription
                        MobileShellSection.HISTORY -> locale.shellHistoryDescription
                    },
                )
            }
        }
    }
}

@Composable
private fun ShellRailItem(
    selected: Boolean,
    onClick: () -> Unit,
    icon: ImageVector,
    label: String,
    description: String,
) {
    WideNavigationRailItem(
        selected = selected,
        onClick = onClick,
        icon = { Icon(icon, contentDescription = null) },
        label = {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(label)
                Text(
                    text = description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 2,
                )
            }
        },
        railExpanded = true,
    )
}

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
        shape = MaterialTheme.shapes.extraLarge,
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
                name = "ffmpeg",
                description = locale.toolDescFfmpeg,
                onHelpClick = { helpDialog = "ffmpeg" to locale.toolDescFfmpeg },
                icon = Icons.Rounded.GraphicEq,
                status = dlState.ffmpeg.status,
                version = null,
                error = null,
                updateStatus = dev.screengoated.toolbox.mobile.downloader.UpdateStatus.IDLE,
                locale = locale,
                onInstall = {},
                onUpdate = {},
                bundledLabel = if (dlState.ffmpeg.version != null) {
                    "${locale.toolBundled} (${dlState.ffmpeg.version})"
                } else {
                    locale.toolBundled
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
            tint = MaterialTheme.colorScheme.primary,
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
                        MaterialTheme.colorScheme.primary
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
                Button(
                    onClick = onDownload,
                    colors = ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.primary,
                    ),
                ) { Text(locale.dlDepsInstall) }
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

@Composable
internal fun QuickActionsRow(locale: MobileLocaleText) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
    ) {
        UtilityTile(
            label = locale.shellDownloadedToolsLabel,
            description = locale.shellDownloadedToolsDescription,
            icon = Icons.Rounded.Download,
            brush = Brush.linearGradient(
                listOf(
                    MaterialTheme.colorScheme.secondary,
                    MaterialTheme.colorScheme.primary,
                ),
            ),
            modifier = Modifier.weight(1f),
        )
        UtilityTile(
            label = locale.shellHelpLabel,
            description = locale.shellHelpDescription,
            icon = Icons.AutoMirrored.Rounded.HelpOutline,
            brush = Brush.linearGradient(
                listOf(
                    MaterialTheme.colorScheme.tertiary,
                    MaterialTheme.colorScheme.primary,
                ),
            ),
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun UtilityTile(
    label: String,
    description: String,
    icon: ImageVector,
    brush: Brush,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier,
        shape = MaterialTheme.shapes.extraLarge,
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(ShellSpacing.innerPad),
            verticalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
        ) {
            GradientMaskedIcon(icon, brush, modifier = Modifier.size(28.dp))
            Text(
                text = label,
                style = MaterialTheme.typography.titleSmall,
            )
            Text(
                text = description,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 2,
            )
        }
    }
}

@Composable
internal fun SectionDetail(
    selectedSection: MobileShellSection,
    state: LiveSessionState,
    apiKey: String,
    cerebrasApiKey: String,
    groqApiKey: String,
    openRouterApiKey: String,
    ollamaUrl: String,
    globalTtsSettings: MobileGlobalTtsSettings,
    presetRuntimeSettings: PresetRuntimeSettings,
    locale: MobileLocaleText,
    wideLayout: Boolean,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onGroqApiKeyChanged: (String) -> Unit,
    onOpenRouterApiKeyChanged: (String) -> Unit,
    onOllamaUrlChanged: (String) -> Unit,
    onPresetRuntimeSettingsClick: () -> Unit,
    onUsageStatsClick: () -> Unit,
    onResetDefaults: () -> Unit = {},
    onVoiceSettingsClick: () -> Unit,
    uiPreferences: dev.screengoated.toolbox.mobile.model.MobileUiPreferences = dev.screengoated.toolbox.mobile.model.MobileUiPreferences(),
    onOverlayOpacityChanged: (Int) -> Unit = {},
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
    onDownloaderClick: () -> Unit = {},
    onDjClick: () -> Unit = {},
    onPresetClick: (String) -> Unit = {},
    onPagerSwipeLockChanged: (Boolean) -> Unit = {},
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope? = null,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope? = null,
) {
    when (selectedSection) {
        MobileShellSection.APPS -> AppsCarouselSection(
            state = state,
            locale = locale,
            onSessionToggle = onSessionToggle,
            canToggle = canToggle,
            onDownloaderClick = onDownloaderClick,
            onDjClick = onDjClick,
            onPagerSwipeLockChanged = onPagerSwipeLockChanged,
            sharedTransitionScope = sharedTransitionScope,
            animatedVisibilityScope = animatedVisibilityScope,
        )

        MobileShellSection.TOOLS -> ToolsSection(
            locale = locale,
            onPresetClick = onPresetClick,
            onPagerSwipeLockChanged = onPagerSwipeLockChanged,
            modifier = Modifier.fillMaxSize(),
        )

        MobileShellSection.SETTINGS -> GlobalSection(
            apiKey = apiKey,
            cerebrasApiKey = cerebrasApiKey,
            groqApiKey = groqApiKey,
            openRouterApiKey = openRouterApiKey,
            ollamaUrl = ollamaUrl,
            globalTtsSettings = globalTtsSettings,
            presetRuntimeSettings = presetRuntimeSettings,
            overlayOpacityPercent = uiPreferences.overlayOpacityPercent,
            locale = locale,
            wideLayout = wideLayout,
            onApiKeyChanged = onApiKeyChanged,
            onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
            onGroqApiKeyChanged = onGroqApiKeyChanged,
            onOpenRouterApiKeyChanged = onOpenRouterApiKeyChanged,
            onOllamaUrlChanged = onOllamaUrlChanged,
            onPresetRuntimeSettingsClick = onPresetRuntimeSettingsClick,
            onUsageStatsClick = onUsageStatsClick,
            onResetDefaults = onResetDefaults,
            onVoiceSettingsClick = onVoiceSettingsClick,
            onOverlayOpacityChanged = onOverlayOpacityChanged,
        )

        MobileShellSection.HISTORY -> PlaceholderSection(
            label = locale.shellHistoryLabel,
            description = locale.shellHistoryDescription,
            locale = locale,
        )
    }
}

private data class AppSlot(val shape: RoundedPolygon, val color: Color)

private data class ShapeInstance(
    val shape: RoundedPolygon,
    val xFrac: Float, val yFrac: Float,
    val sizeFrac: Float, val alpha: Float,
    val rotation: Float,
)

private val allDecoShapes by lazy { listOf(
    MaterialShapes.Arch, MaterialShapes.Arrow, MaterialShapes.Boom, MaterialShapes.Bun,
    MaterialShapes.Burst, MaterialShapes.Circle, MaterialShapes.ClamShell,
    MaterialShapes.Clover4Leaf, MaterialShapes.Clover8Leaf,
    MaterialShapes.Cookie12Sided, MaterialShapes.Cookie4Sided, MaterialShapes.Cookie6Sided,
    MaterialShapes.Cookie7Sided, MaterialShapes.Cookie9Sided,
    MaterialShapes.Diamond, MaterialShapes.Fan, MaterialShapes.Flower, MaterialShapes.Gem,
    MaterialShapes.Ghostish, MaterialShapes.Heart, MaterialShapes.Oval, MaterialShapes.Pentagon,
    MaterialShapes.Pill, MaterialShapes.PixelCircle, MaterialShapes.PixelTriangle,
    MaterialShapes.Puffy, MaterialShapes.PuffyDiamond, MaterialShapes.SemiCircle,
    MaterialShapes.Slanted, MaterialShapes.SoftBoom, MaterialShapes.SoftBurst,
    MaterialShapes.Square, MaterialShapes.Sunny, MaterialShapes.Triangle, MaterialShapes.VerySunny,
) }

/** Place shapes with collision detection — no overlapping. */
private fun generateNonOverlappingShapes(seed: Long): List<ShapeInstance> {
    val rng = java.util.Random(seed)
    val placed = mutableListOf<ShapeInstance>()
    var attempts = 0
    while (placed.size < 6 && attempts < 80) {
        attempts++
        val sizeFrac = 0.15f + rng.nextFloat() * 0.75f // tiny to huge
        val xFrac = -0.05f + rng.nextFloat() * 1.10f   // allow overflow left/right
        val yFrac = -0.10f + rng.nextFloat() * 1.20f    // allow overflow top/bottom
        val collides = placed.any { other ->
            val dx = xFrac - other.xFrac
            val dy = yFrac - other.yFrac
            val minDist = (sizeFrac + other.sizeFrac) * 0.32f
            dx * dx + dy * dy < minDist * minDist
        }
        if (!collides) {
            placed.add(ShapeInstance(
                shape = allDecoShapes[rng.nextInt(allDecoShapes.size)],
                xFrac = xFrac, yFrac = yFrac,
                sizeFrac = sizeFrac,
                alpha = 0.10f + rng.nextFloat() * 0.16f,
                rotation = rng.nextFloat() * 360f,
            ))
        }
    }
    return placed
}

/**
 * Non-overlapping shapes with smooth morphing + slight spin + spring bounce.
 * Each shape periodically morphs to another MaterialShape via Morph(A,B).toPath(progress).
 * During the morph, a slight rotation is applied (spring bounce).
 * Idle: morph every 3-6s. Scrolling/active: morph every 0.8-1.6s.
 */
@Composable
private fun AnimatedShapesCanvas(
    color: Color,
    seed: Long,
    isScrolling: Boolean = false,
    modifier: Modifier = Modifier,
) {
    val placements = remember(seed) { generateNonOverlappingShapes(seed) }

    // Per-shape morph state: tracks the current from→to morph pair + generation counter
    // The generation counter drives the animateFloatAsState target flip (0f↔1f)
    data class MorphPair(
        val from: RoundedPolygon,
        val to: RoundedPolygon,
        val gen: Int,
        val spinDelta: Float,
    )

    @Composable
    fun rememberAnimatedShape(i: Int, inst: ShapeInstance): Triple<Morph, Float, Float> {
        var pair by remember { mutableStateOf(MorphPair(inst.shape, inst.shape, 0, 0f)) }

        val intervalMs = if (isScrolling) (800L + i * 200L) else (3000L + i * 1500L)
        LaunchedEffect(isScrolling, i) {
            val rng = java.util.Random(seed + i * 17L)
            while (true) {
                kotlinx.coroutines.delay(intervalMs)
                val nextShape = allDecoShapes[rng.nextInt(allDecoShapes.size)]
                val spinDelta = (rng.nextFloat() - 0.5f) * 30f // ±15° spin during morph
                pair = MorphPair(pair.to, nextShape, pair.gen + 1, spinDelta)
            }
        }

        // Morph progress: animate 0→1 each time gen changes (odd→1, even→0)
        val morphTarget = if (pair.gen % 2 == 0) 0f else 1f
        val morphProgress by animateFloatAsState(
            targetValue = morphTarget,
            animationSpec = spring(
                dampingRatio = Spring.DampingRatioNoBouncy,
                stiffness = Spring.StiffnessVeryLow,
            ),
            label = "morph-$i",
        )
        // Actual progress within the current pair: how far from→to
        val t = if (pair.gen % 2 == 0) (1f - morphProgress) else morphProgress

        // Spin: slight rotation during morph (spring bounce)
        val spinTarget = inst.rotation + pair.spinDelta * pair.gen
        val spin by animateFloatAsState(
            targetValue = spinTarget,
            animationSpec = spring(
                dampingRatio = Spring.DampingRatioMediumBouncy,
                stiffness = Spring.StiffnessLow,
            ),
            label = "spin-$i",
        )

        val morph = remember(pair.from, pair.to) { Morph(pair.from, pair.to) }
        return Triple(morph, t, spin)
    }

    val animated = placements.mapIndexed { i, inst ->
        val (morph, progress, spin) = rememberAnimatedShape(i, inst)
        Triple(inst, Triple(morph, progress, spin), Unit)
    }

    Canvas(modifier = modifier.fillMaxSize()) {
        animated.forEach { (inst, anim, _) ->
            val (morph, progress, spin) = anim
            val path = morph.toPath(progress = progress)
            val s = size.minDimension * inst.sizeFrac
            if (s < 1f) return@forEach
            val bounds = path.getBounds()
            val pathSize = maxOf(bounds.width, bounds.height).takeIf { it > 0f } ?: 1f
            val cx = bounds.left + bounds.width / 2f
            val cy = bounds.top + bounds.height / 2f
            val scale = s / pathSize
            val matrix = Matrix()
            matrix.translate(size.width * inst.xFrac, size.height * inst.yFrac)
            matrix.rotateZ(spin)
            matrix.scale(scale, scale)
            matrix.translate(-cx, -cy)
            path.transform(matrix)
            drawPath(path, color = color.copy(alpha = inst.alpha))
        }
    }
}

private val appSlots = listOf(
    AppSlot(MaterialShapes.Sunny,        Color(0xFF4DB6AC)), // Live Translate — teal
    AppSlot(MaterialShapes.SemiCircle,   Color(0xFFEF9A9A)), // placeholder — coral
    AppSlot(MaterialShapes.Heart,        Color(0xFFCE93D8)), // placeholder — purple
    AppSlot(MaterialShapes.Cookie4Sided, Color(0xFFFFCC80)), // placeholder — amber
    AppSlot(MaterialShapes.Clover4Leaf,  Color(0xFF90CAF9)), // placeholder — blue
)

@Composable
internal fun AppsCarouselSection(
    state: LiveSessionState,
    locale: MobileLocaleText,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
    onDownloaderClick: () -> Unit = {},
    onDjClick: () -> Unit = {},
    onPagerSwipeLockChanged: (Boolean) -> Unit = {},
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope? = null,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope? = null,
) {
    val isLandscape = LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE

    if (isLandscape) {
        AppsHorizontalCarousel(
            state,
            locale,
            onSessionToggle,
            canToggle,
            onDownloaderClick,
            onDjClick,
            onPagerSwipeLockChanged,
            sharedTransitionScope,
            animatedVisibilityScope,
        )
    } else {
        AppsVerticalCarousel(state, locale, onSessionToggle, canToggle, onDownloaderClick, onDjClick, sharedTransitionScope, animatedVisibilityScope)
    }
}

@Composable
private fun AppsItemContent(
    index: Int,
    state: LiveSessionState,
    locale: MobileLocaleText,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
    onDownloaderClick: () -> Unit,
    onDjClick: () -> Unit,
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope?,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope?,
) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .then(
                when (index) {
                    1 -> {
                        val sharedMod = if (sharedTransitionScope != null && animatedVisibilityScope != null) {
                            with(sharedTransitionScope) {
                                Modifier.sharedBounds(
                                    sharedContentState = rememberSharedContentState("downloader-card"),
                                    animatedVisibilityScope = animatedVisibilityScope,
                                    resizeMode = androidx.compose.animation.SharedTransitionScope.ResizeMode.RemeasureToBounds,
                                )
                            }
                        } else Modifier
                        sharedMod.then(Modifier.clickable(onClick = onDownloaderClick))
                    }
                    2 -> Modifier.clickable(onClick = onDjClick)
                    else -> Modifier
                },
            ),
    ) {
        when (index) {
            0 -> LiveTranslateCarouselTile(state = state, locale = locale, onSessionToggle = onSessionToggle, canToggle = canToggle)
            1 -> AppTile(slot = appSlots[1], title = locale.appVideoDownloaderTitle, icon = Icons.Rounded.Download)
            2 -> AppTile(slot = appSlots[2], title = locale.appDjTitle, icon = Icons.Rounded.GraphicEq)
            else -> EmptyAppTile(slot = appSlots[index])
        }
    }
}

@Composable
private fun AppsVerticalCarousel(
    state: LiveSessionState,
    locale: MobileLocaleText,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
    onDownloaderClick: () -> Unit,
    onDjClick: () -> Unit,
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope?,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope?,
) {
    val screenH = LocalConfiguration.current.screenHeightDp.dp
    val available = (screenH - 170.dp).coerceAtLeast(320.dp)
    val itemHeight = ((available - 40.dp) / 3.2f).coerceIn(140.dp, 200.dp)
    val carouselHeight = available.coerceAtMost(700.dp)
    val fadeSize = 32.dp
    val bgColor = MaterialTheme.colorScheme.background

    Box(modifier = Modifier.fillMaxWidth().height(carouselHeight)) {
        VerticalUncontainedCarousel(
            itemCount = appSlots.size,
            itemHeight = itemHeight,
            modifier = Modifier.fillMaxWidth().height(carouselHeight),
            itemSpacing = 8.dp,
            contentPadding = PaddingValues(top = 4.dp, bottom = fadeSize),
        ) { index ->
            Box(modifier = Modifier.fillMaxSize().maskClip(MaterialTheme.shapes.extraLarge)) {
                AppsItemContent(index, state, locale, onSessionToggle, canToggle, onDownloaderClick, onDjClick, sharedTransitionScope, animatedVisibilityScope)
            }
        }
        Box(modifier = Modifier.fillMaxWidth().height(fadeSize).background(Brush.verticalGradient(listOf(bgColor, Color.Transparent))))
        Box(modifier = Modifier.fillMaxWidth().height(fadeSize).align(Alignment.BottomStart).background(Brush.verticalGradient(listOf(Color.Transparent, bgColor))))
    }
}

@Composable
private fun AppsHorizontalCarousel(
    state: LiveSessionState,
    locale: MobileLocaleText,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
    onDownloaderClick: () -> Unit,
    onDjClick: () -> Unit,
    onPagerSwipeLockChanged: (Boolean) -> Unit,
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope?,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope?,
) {
    val screenH = LocalConfiguration.current.screenHeightDp.dp
    val screenW = LocalConfiguration.current.screenWidthDp.dp
    val itemWidth = ((screenW - 120.dp) / 3.2f).coerceIn(220.dp, 320.dp)
    val carouselHeight = (screenH - 100.dp).coerceIn(160.dp, 300.dp)
    val fadeSize = 24.dp
    val bgColor = MaterialTheme.colorScheme.background
    val carouselState = rememberCarouselState { appSlots.size }

    Box(modifier = Modifier.fillMaxWidth().height(carouselHeight)) {
        HorizontalUncontainedCarousel(
            state = carouselState,
            itemWidth = itemWidth,
            modifier = Modifier
                .fillMaxSize()
                .lockPagerForCarouselDrag(
                    canScrollBackward = { carouselState.canScrollBackward },
                    canScrollForward = { carouselState.canScrollForward },
                    onPagerSwipeLockChanged = onPagerSwipeLockChanged,
                ),
            itemSpacing = 8.dp,
            contentPadding = PaddingValues(start = 4.dp, end = fadeSize),
        ) { index ->
            Card(
                modifier = Modifier.fillMaxSize().maskClip(MaterialTheme.shapes.extraLarge),
                shape = MaterialTheme.shapes.extraLarge,
                colors = CardDefaults.cardColors(containerColor = appSlots[index].color.copy(alpha = 0.15f)),
            ) {
                AppsItemContent(index, state, locale, onSessionToggle, canToggle, onDownloaderClick, onDjClick, sharedTransitionScope, animatedVisibilityScope)
            }
        }
        // Left fade
        Box(modifier = Modifier.fillMaxHeight().width(fadeSize).background(Brush.horizontalGradient(listOf(bgColor, Color.Transparent))))
        // Right fade
        Box(modifier = Modifier.fillMaxHeight().width(fadeSize).align(Alignment.CenterEnd).background(Brush.horizontalGradient(listOf(Color.Transparent, bgColor))))
    }
}

@Composable
private fun LiveTranslateCarouselTile(
    state: LiveSessionState,
    locale: MobileLocaleText,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
) {
    val isRunning = state.phase in setOf(
        SessionPhase.STARTING, SessionPhase.LISTENING, SessionPhase.TRANSLATING,
    )
    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
    val slot = appSlots[0]
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(if (isRunning) slot.color.copy(alpha = 0.30f) else slot.color.copy(alpha = 0.15f)),
    ) {
        AnimatedShapesCanvas(
            color = slot.color,
            seed = slot.color.hashCode().toLong() xor 0x42L,
            isScrolling = isRunning, // morph faster when live translate is active
        )
        val stretchedFamily = remember {
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
                FontFamily(
                    androidx.compose.ui.text.font.Font(
                        resId = dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
                        weight = FontWeight.Black,
                        variationSettings = androidx.compose.ui.text.font.FontVariation.Settings(
                            androidx.compose.ui.text.font.FontVariation.weight(FontWeight.Black.weight),
                            androidx.compose.ui.text.font.FontVariation.Setting("ROND", 100f),
                            androidx.compose.ui.text.font.FontVariation.Setting("wdth", 125f),
                        ),
                    ),
                )
            } else {
                FontFamily.Default
            }
        }
        if (isLandscape) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 18.dp, vertical = 16.dp),
                verticalArrangement = Arrangement.SpaceBetween,
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Icon(
                        Icons.Rounded.Translate,
                        contentDescription = null,
                        tint = slot.color,
                        modifier = Modifier.size(40.dp),
                    )
                    Text(
                        text = locale.shellLiveTitle,
                        fontFamily = stretchedFamily,
                        fontWeight = FontWeight.Black,
                        fontSize = 22.sp,
                        lineHeight = 24.sp,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 3,
                        modifier = Modifier.weight(1f),
                    )
                }
                Button(
                    onClick = onSessionToggle,
                    enabled = canToggle,
                    shape = CircleShape,
                    colors = if (isRunning) {
                        ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)
                    } else {
                        ButtonDefaults.buttonColors()
                    },
                    modifier = Modifier.align(Alignment.End),
                ) {
                    Icon(
                        if (isRunning) Icons.Rounded.Stop else Icons.Rounded.PlayArrow,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                    )
                    Spacer(Modifier.width(ButtonDefaults.IconSpacing))
                    Text(if (isRunning) locale.turnOff else locale.turnOn)
                }
            }
        } else {
            Row(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 20.dp, vertical = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    Icons.Rounded.Translate,
                    contentDescription = null,
                    tint = slot.color,
                    modifier = Modifier.size(44.dp),
                )
                Spacer(Modifier.width(14.dp))
                Column(modifier = Modifier.weight(1f)) {
                    val words = locale.shellLiveTitle.split(" ", limit = 2)
                    if (words.isNotEmpty()) {
                        Text(
                            text = words[0],
                            fontFamily = stretchedFamily,
                            fontWeight = FontWeight.Black,
                            fontSize = 28.sp,
                            lineHeight = 32.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                    if (words.size > 1) {
                        Text(
                            text = words[1],
                            fontWeight = FontWeight.Bold,
                            fontSize = 26.sp,
                            lineHeight = 30.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                }
                Column(
                    horizontalAlignment = Alignment.End,
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Button(
                        onClick = onSessionToggle,
                        enabled = canToggle,
                        shape = CircleShape,
                        colors = if (isRunning) {
                            ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)
                        } else {
                            ButtonDefaults.buttonColors()
                        },
                    ) {
                        Icon(
                            if (isRunning) Icons.Rounded.Stop else Icons.Rounded.PlayArrow,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(ButtonDefaults.IconSpacing))
                        Text(if (isRunning) locale.turnOff else locale.turnOn)
                    }
                }
            }
        }
    }
}

@Composable
private fun AppTile(
    slot: AppSlot,
    title: String,
    icon: ImageVector?,
) {
    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(slot.color.copy(alpha = 0.15f)),
    ) {
        AnimatedShapesCanvas(
            color = slot.color,
            seed = slot.color.hashCode().toLong(),
        )
        val stretchedFamily = remember {
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
                FontFamily(
                    androidx.compose.ui.text.font.Font(
                        resId = dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
                        weight = FontWeight.Black,
                        variationSettings = androidx.compose.ui.text.font.FontVariation.Settings(
                            androidx.compose.ui.text.font.FontVariation.weight(FontWeight.Black.weight),
                            androidx.compose.ui.text.font.FontVariation.Setting("ROND", 100f),
                            androidx.compose.ui.text.font.FontVariation.Setting("wdth", 125f),
                        ),
                    ),
                )
            } else {
                FontFamily.Default
            }
        }
        if (isLandscape) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 18.dp, vertical = 16.dp),
                verticalArrangement = Arrangement.SpaceBetween,
            ) {
                if (icon != null) {
                    Icon(
                        icon,
                        contentDescription = null,
                        tint = slot.color,
                        modifier = Modifier.size(40.dp),
                    )
                }
                Text(
                    text = title,
                    fontFamily = stretchedFamily,
                    fontWeight = FontWeight.Black,
                    fontSize = 22.sp,
                    lineHeight = 24.sp,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 3,
                )
            }
        } else {
            Row(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 20.dp, vertical = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (icon != null) {
                    Icon(
                        icon,
                        contentDescription = null,
                        tint = slot.color,
                        modifier = Modifier.size(44.dp),
                    )
                    Spacer(Modifier.width(14.dp))
                }
                Column(modifier = Modifier.weight(1f)) {
                    val words = title.split(" ", limit = 2)
                    if (words.isNotEmpty()) {
                        Text(
                            text = words[0],
                            fontFamily = stretchedFamily,
                            fontWeight = FontWeight.Black,
                            fontSize = 28.sp,
                            lineHeight = 32.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                    if (words.size > 1) {
                        Text(
                            text = words[1],
                            fontWeight = FontWeight.Bold,
                            fontSize = 26.sp,
                            lineHeight = 30.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tools Section — mirrors Windows sidebar preset categories
// ---------------------------------------------------------------------------

@Composable
private fun EmptyAppTile(slot: AppSlot) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(slot.color.copy(alpha = 0.15f)),
    ) {
        AnimatedShapesCanvas(
            color = slot.color,
            seed = slot.color.hashCode().toLong(),
        )
    }
}

private data class ToolPresetItem(
    val id: String,
    val nameEn: String,
    val nameVi: String,
    val nameKo: String,
    val icon: ImageVector,
    /** If true, `id` is the full preset ID. If false, needs "preset_" prefix. */
    val isFullId: Boolean = false,
) {
    fun name(lang: String): String = when (lang) {
        "vi" -> nameVi
        "ko" -> nameKo
        else -> nameEn
    }

    /** Split the label into two balanced lines at the best word boundary. */
    fun balancedName(lang: String): String {
        val raw = name(lang)
        val words = raw.split(" ")
        if (words.size <= 1) return "$raw\n " // pad single-word to keep 2-line height
        var bestIdx = 1
        var bestMax = Int.MAX_VALUE
        for (i in 1 until words.size) {
            val top = words.subList(0, i).joinToString(" ")
            val bot = words.subList(i, words.size).joinToString(" ")
            val m = maxOf(top.length, bot.length)
            if (m < bestMax) { bestMax = m; bestIdx = i }
        }
        val top = words.subList(0, bestIdx).joinToString(" ")
        val bot = words.subList(bestIdx, words.size).joinToString(" ")
        return "$top\n$bot"
    }
}

private data class ToolCategory(
    val labelGetter: (MobileLocaleText) -> String,
    val accentColor: Color,
    val presets: List<ToolPresetItem>,
    /** Preset types that belong to this category (for routing custom presets). */
    val acceptsTypes: Set<dev.screengoated.toolbox.mobile.shared.preset.PresetType> = emptySet(),
)

private val toolCategories = listOf(
    // Column 1: Image presets (matches Windows order exactly)
    ToolCategory(
        labelGetter = { it.toolsCategoryImage },
        accentColor = Color(0xFF5C9CE6),
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE),
        presets = listOf(
            ToolPresetItem("translate", "Translate region", "Dịch vùng", "영역 번역", Icons.Rounded.Translate),
            ToolPresetItem("extract_retranslate", "Trans (ACCURATE)", "Dịch vùng (CHUẨN)", "영역 번역 (정확)", Icons.Rounded.Verified),
            ToolPresetItem("translate_auto_paste", "Trans (Auto paste)", "Dịch vùng (Tự dán)", "영역 번역 (자동 붙.)", Icons.Rounded.ContentCut),
            ToolPresetItem("extract_table", "Extract Table", "Trích bảng", "표 추출", Icons.Rounded.TableChart),
            ToolPresetItem("translate_retranslate", "Trans+Retrans", "Dịch vùng+Dịch lại", "번역+재번역", Icons.Rounded.Translate),
            ToolPresetItem("extract_retrans_retrans", "Trans (ACC)+Retrans", "D.vùng (CHUẨN)+D.lại", "번역(정확)+재번역", Icons.Rounded.Verified),
            ToolPresetItem("ocr", "Extract text", "Lấy text từ ảnh", "텍스트 추출", Icons.Rounded.TextFields),
            ToolPresetItem("ocr_read", "Read this region", "Đọc vùng này", "영역 읽기", Icons.AutoMirrored.Rounded.VolumeUp),
            ToolPresetItem("quick_screenshot", "Quick Screenshot", "Chụp MH nhanh", "빠른 스크린샷", Icons.Rounded.PhotoCamera),
            ToolPresetItem("qr_scanner", "QR Scanner", "Quét mã QR", "QR 스캔", Icons.Rounded.QrCodeScanner),
            ToolPresetItem("summarize", "Summarize region", "Tóm tắt vùng", "영역 요약", Icons.Rounded.Summarize),
            ToolPresetItem("desc", "Describe image", "Mô tả ảnh", "이미지 설명", Icons.Rounded.Description),
            ToolPresetItem("ask_image", "Ask about image", "Hỏi về ảnh", "이미지 질문", Icons.Rounded.ImageSearch),
            ToolPresetItem("fact_check", "Fact Check", "Kiểm chứng thông tin", "정보 확인", Icons.Rounded.Verified),
            ToolPresetItem("omniscient_god", "Omniscient God", "Thần Trí tuệ", "전지전능", Icons.Rounded.AutoAwesome),
            ToolPresetItem("hang_image", "Image Overlay", "Treo ảnh", "이미지 오버레이", Icons.Rounded.CameraAlt),
        ),
    ),
    // Column 2a: Text Select presets
    ToolCategory(
        labelGetter = { it.toolsCategoryTextSelect },
        accentColor = Color(0xFF5DB882),
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT),
        presets = listOf(
            ToolPresetItem("read_aloud", "Read aloud", "Đọc to", "크게 읽기", Icons.Rounded.RecordVoiceOver),
            ToolPresetItem("translate_select", "Translate", "Dịch", "번역", Icons.Rounded.GTranslate),
            ToolPresetItem("translate_arena", "Trans (Arena)", "Dịch (Arena)", "번역 (아레나)", Icons.Rounded.Translate),
            ToolPresetItem("trans_retrans_select", "Trans+Retrans", "Dịch+Dịch lại", "번역+재번역", Icons.Rounded.Translate),
            ToolPresetItem("select_translate_replace", "Trans & Replace", "Dịch và Thay", "번역 후 교체", Icons.Rounded.SwapHoriz),
            ToolPresetItem("fix_grammar", "Fix Grammar", "Sửa ngữ pháp", "문법 수정", Icons.Rounded.Spellcheck),
            ToolPresetItem("rephrase", "Rephrase", "Viết lại", "다시 쓰기", Icons.Rounded.FormatQuote),
            ToolPresetItem("make_formal", "Make Formal", "Chuyên nghiệp hóa", "공식적으로", Icons.Rounded.AutoFixHigh),
            ToolPresetItem("explain", "Explain", "Giải thích", "설명", Icons.Rounded.Lightbulb),
            ToolPresetItem("ask_text", "Ask about text...", "Hỏi về text...", "텍스트 질문", Icons.Rounded.QuestionAnswer),
            ToolPresetItem("edit_as_follows", "Edit as follows:", "Sửa như sau:", "다음과 같이 수정:", Icons.Rounded.Edit),
            ToolPresetItem("101_on_this", "101 on this", "Tất tần tật", "이것의 모든 것", Icons.Rounded.School),
            ToolPresetItem("hang_text", "Text Overlay", "Treo text", "텍스트 오버레이", Icons.AutoMirrored.Rounded.TextSnippet),
        ),
    ),
    // Column 2b: Text Input (Type) presets
    ToolCategory(
        labelGetter = { it.toolsCategoryTextInput },
        accentColor = Color(0xFF5DB882),
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT),
        presets = listOf(
            ToolPresetItem("trans_retrans_typing", "Trans+Retrans (Type)", "Dịch+Dịch lại (Tự gõ)", "번역+재번역 (입력)", Icons.Rounded.Translate),
            ToolPresetItem("ask_ai", "Ask AI", "Hỏi AI", "AI 질문", Icons.Rounded.SmartToy),
            ToolPresetItem("internet_search", "Internet Search", "Tìm kiếm internet", "인터넷 검색", Icons.Rounded.Search),
            ToolPresetItem("make_game", "Make a Game", "Tạo con game", "게임 만들기", Icons.Rounded.Gamepad),
            ToolPresetItem("quick_note", "Quick Note", "Note nhanh", "빠른 메모", Icons.AutoMirrored.Rounded.Note),
        ),
    ),
    // Column 3a: Mic presets
    ToolCategory(
        labelGetter = { it.toolsCategoryMicRecording },
        accentColor = Color(0xFFDCA850),
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC),
        presets = listOf(
            ToolPresetItem("transcribe", "Transcribe speech", "Lời nói thành văn", "음성 받아쓰기", Icons.Rounded.Mic),
            ToolPresetItem("continuous_writing_online", "Continuous Writing", "Viết liên tục", "연속 입력", Icons.Rounded.Keyboard),
            ToolPresetItem("fix_pronunciation", "Fix pronunciation", "Chỉnh phát âm", "발음 교정", Icons.Rounded.RecordVoiceOver),
            ToolPresetItem("transcribe_retranslate", "Quick 4NR reply 1", "Trả lời ng.nc.ngoài 1", "빠른 외국인 답변 1", Icons.Rounded.Translate),
            ToolPresetItem("quicker_foreigner_reply", "Quick 4NR reply 2", "Trả lời ng.nc.ngoài 2", "빠른 외국인 답변 2", Icons.Rounded.Translate),
            ToolPresetItem("quick_ai_question", "Quick AI Question", "Hỏi nhanh AI", "빠른 AI 질문", Icons.Rounded.VoiceChat),
            ToolPresetItem("voice_search", "Voice Search", "Nói để search", "음성 검색", Icons.Rounded.Search),
            ToolPresetItem("quick_record", "Quick Record", "Thu âm nhanh", "빠른 녹음", Icons.Rounded.FiberSmartRecord),
        ),
    ),
    // Column 3b: Device Audio presets
    ToolCategory(
        labelGetter = { it.toolsCategoryDeviceAudio },
        accentColor = Color(0xFFDCA850),
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO),
        presets = listOf(
            ToolPresetItem("study_language", "Study language", "Học ngoại ngữ", "언어 학습", Icons.Rounded.School),
            ToolPresetItem("record_device", "Device Record", "Thu âm máy", "시스템 녹음", Icons.Rounded.SpeakerPhone),
            ToolPresetItem("transcribe_english_offline", "Transcribe English", "Chép lời TA", "영어 받아쓰기", Icons.Rounded.GraphicEq),
        ),
    ),
)

@Composable
internal fun ToolsSection(
    locale: MobileLocaleText,
    onPresetClick: (String) -> Unit = {},
    onPagerSwipeLockChanged: (Boolean) -> Unit = {},
    modifier: Modifier = Modifier,
) {
    val presetRepository = (LocalContext.current.applicationContext as SgtMobileApplication)
        .appContainer
        .presetRepository
    val presetCatalog by presetRepository.catalogState.collectAsState()
    val favoritePresetIds by remember(presetCatalog) {
        derivedStateOf {
            presetCatalog.presets
                .filter { it.preset.isFavorite }
                .map { it.preset.id }
                .toSet()
        }
    }
    val lang = locale.languageOptions.firstOrNull { it.label.contains("English") }?.let { null }
        ?: locale.let {
            when {
                it.turnOn == "Bật" -> "vi"
                it.turnOn == "켜기" -> "ko"
                else -> "en"
            }
        }
    var toolbarMode by remember { mutableStateOf(ToolbarMode.NONE) }
    var fabMenuExpanded by rememberSaveable { mutableStateOf(false) }

    if (fabMenuExpanded) {
        androidx.activity.compose.BackHandler { fabMenuExpanded = false }
    }

    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE

    Box(modifier = modifier.fillMaxSize()) {
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 0.dp),
            contentPadding = PaddingValues(
                bottom = 136.dp,
                end = if (isLandscape) 24.dp else 0.dp,
            ),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            items(toolCategories) { category ->
                // Merge static presets with custom presets from catalog
                val customItems = presetCatalog.presets
                    .filter { !it.isBuiltIn && it.preset.presetType in category.acceptsTypes }
                    .map { resolved ->
                        ToolPresetItem(
                            id = resolved.preset.id,
                            nameEn = resolved.preset.nameEn,
                            nameVi = resolved.preset.nameVi,
                            nameKo = resolved.preset.nameKo,
                            icon = Icons.Rounded.AutoAwesome,
                            isFullId = true,
                        )
                    }
                // Filter out hidden/deleted built-in presets
                val catalogIds = presetCatalog.presets.map { it.preset.id }.toSet()
                val visibleBuiltIns = category.presets.filter { "preset_${it.id}" in catalogIds }
                val effectivePresets = visibleBuiltIns + customItems
                ToolCategoryRow(
                    label = category.labelGetter(locale),
                    accentColor = category.accentColor,
                    presets = effectivePresets,
                    lang = lang,
                    onPresetClick = onPresetClick,
                    onPagerSwipeLockChanged = onPagerSwipeLockChanged,
                    toolbarMode = toolbarMode,
                    favoritePresetIds = favoritePresetIds,
                    onFavoriteToggle = { presetId ->
                        presetRepository.toggleFavorite(presetId)
                    },
                    onDuplicate = { presetId ->
                        val newId = presetRepository.duplicatePreset(presetId, lang)
                        android.util.Log.d("PresetTools", "DUPLICATE preset=$presetId → newId=$newId")
                        android.util.Log.d("PresetTools", "Catalog size after: ${presetRepository.catalogState.value.presets.size}")
                        android.util.Log.d("PresetTools", "Custom presets: ${presetRepository.catalogState.value.presets.filter { !it.isBuiltIn }.map { it.preset.nameEn }}")
                    },
                    onDelete = { presetId ->
                        android.util.Log.d("PresetTools", "DELETE preset=$presetId")
                        presetRepository.deletePreset(presetId)
                        android.util.Log.d("PresetTools", "Catalog size after: ${presetRepository.catalogState.value.presets.size}")
                    },
                )
            }
        }

        Box(
            modifier = Modifier
                .fillMaxWidth()
                .align(Alignment.BottomCenter)
                .navigationBarsPadding()
                .padding(horizontal = 8.dp, vertical = 8.dp),
        ) {
            HorizontalFloatingToolbar(
                modifier = Modifier.align(Alignment.BottomStart),
                expanded = true,
                content = {
                    IconButton(onClick = {
                        toolbarMode = if (toolbarMode == ToolbarMode.DUPLICATE) ToolbarMode.NONE else ToolbarMode.DUPLICATE
                    }) {
                        Icon(
                            Icons.Rounded.ContentCopy,
                            contentDescription = "Duplicate",
                            tint = if (toolbarMode == ToolbarMode.DUPLICATE) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    IconButton(onClick = {
                        toolbarMode = if (toolbarMode == ToolbarMode.FAVORITE) ToolbarMode.NONE else ToolbarMode.FAVORITE
                    }) {
                        Icon(
                            Icons.Rounded.Star,
                            contentDescription = "Favorite",
                            tint = if (toolbarMode == ToolbarMode.FAVORITE) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    IconButton(onClick = {
                        toolbarMode = if (toolbarMode == ToolbarMode.DELETE) ToolbarMode.NONE else ToolbarMode.DELETE
                    }) {
                        Icon(
                            Icons.Rounded.Delete,
                            contentDescription = "Delete",
                            tint = if (toolbarMode == ToolbarMode.DELETE) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                },
            )

            FloatingActionButtonMenu(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .offset(y = 8.dp),
                expanded = fabMenuExpanded,
                button = {
                    ToggleFloatingActionButton(
                        modifier = Modifier.animateFloatingActionButton(
                            visible = true,
                            alignment = Alignment.BottomEnd,
                        ),
                        checked = fabMenuExpanded,
                        onCheckedChange = { fabMenuExpanded = !fabMenuExpanded },
                    ) {
                        val imageVector by remember {
                            derivedStateOf {
                                if (checkedProgress > 0.5f) Icons.Rounded.Close else Icons.Rounded.Add
                            }
                        }
                        Icon(
                            painter = rememberVectorPainter(imageVector),
                            contentDescription = "Create",
                            modifier = Modifier.animateIcon({ checkedProgress }),
                        )
                    }
                },
            ) {
                FloatingActionButtonMenuItem(
                    onClick = {
                        fabMenuExpanded = false
                        presetRepository.createCustomPreset(
                            type = dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE,
                            lang = lang,
                        )
                    },
                    icon = { Icon(Icons.Rounded.Image, contentDescription = null, modifier = if (isLandscape) Modifier.size(18.dp) else Modifier) },
                    text = { Text(locale.toolsCategoryImage, style = if (isLandscape) MaterialTheme.typography.labelSmall else MaterialTheme.typography.labelLarge) },
                    modifier = if (isLandscape) Modifier.height(40.dp) else Modifier,
                )
                FloatingActionButtonMenuItem(
                    onClick = {
                        fabMenuExpanded = false
                        presetRepository.createCustomPreset(
                            type = dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT,
                            lang = lang,
                        )
                    },
                    icon = { Icon(Icons.Rounded.TextFields, contentDescription = null, modifier = if (isLandscape) Modifier.size(18.dp) else Modifier) },
                    text = { Text(locale.toolsCategoryTextSelect, style = if (isLandscape) MaterialTheme.typography.labelSmall else MaterialTheme.typography.labelLarge) },
                    modifier = if (isLandscape) Modifier.height(40.dp) else Modifier,
                )
                FloatingActionButtonMenuItem(
                    onClick = {
                        fabMenuExpanded = false
                        presetRepository.createCustomPreset(
                            type = dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT,
                            lang = lang,
                        )
                    },
                    icon = { Icon(Icons.Rounded.Keyboard, contentDescription = null, modifier = if (isLandscape) Modifier.size(18.dp) else Modifier) },
                    text = { Text(locale.toolsCategoryTextInput, style = if (isLandscape) MaterialTheme.typography.labelSmall else MaterialTheme.typography.labelLarge) },
                    modifier = if (isLandscape) Modifier.height(40.dp) else Modifier,
                )
                FloatingActionButtonMenuItem(
                    onClick = {
                        fabMenuExpanded = false
                        presetRepository.createCustomPreset(
                            type = dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC,
                            lang = lang,
                        )
                    },
                    icon = { Icon(Icons.Rounded.Mic, contentDescription = null, modifier = if (isLandscape) Modifier.size(18.dp) else Modifier) },
                    text = { Text(locale.toolsCategoryMicRecording, style = if (isLandscape) MaterialTheme.typography.labelSmall else MaterialTheme.typography.labelLarge) },
                    modifier = if (isLandscape) Modifier.height(40.dp) else Modifier,
                )
                FloatingActionButtonMenuItem(
                    onClick = {
                        fabMenuExpanded = false
                        presetRepository.createCustomPreset(
                            type = dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO,
                            lang = lang,
                        )
                    },
                    icon = { Icon(Icons.Rounded.SpeakerPhone, contentDescription = null, modifier = if (isLandscape) Modifier.size(18.dp) else Modifier) },
                    text = { Text(locale.toolsCategoryDeviceAudio, style = if (isLandscape) MaterialTheme.typography.labelSmall else MaterialTheme.typography.labelLarge) },
                    modifier = if (isLandscape) Modifier.height(40.dp).padding(bottom = 8.dp) else Modifier.padding(bottom = 12.dp),
                )
            }
        }
    }
}

private enum class ToolbarMode { NONE, DUPLICATE, FAVORITE, DELETE }

/** Font family at a specific wdth axis value. */
private fun flexFontFamily(wdth: Int): FontFamily {
    return if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
        FontFamily(
            androidx.compose.ui.text.font.Font(
                resId = dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
                weight = FontWeight.Medium,
                variationSettings = androidx.compose.ui.text.font.FontVariation.Settings(
                    androidx.compose.ui.text.font.FontVariation.weight(FontWeight.Medium.weight),
                    androidx.compose.ui.text.font.FontVariation.Setting("ROND", 100f),
                    androidx.compose.ui.text.font.FontVariation.Setting("wdth", wdth.toFloat()),
                ),
            ),
        )
    } else {
        FontFamily.Default
    }
}

/** Condense steps: 100 → 90 → 80 → 70 → 62 (Google Sans Flex minimum). */
private val condensedFontSteps: List<Pair<Int, FontFamily>> by lazy {
    listOf(100, 90, 80, 70, 62).map { wdth -> wdth to flexFontFamily(wdth) }
}

/** Stretch steps: 100 → 110 → 120 → 125 (Google Sans Flex maximum). */
private val stretchedFontSteps: List<Pair<Int, FontFamily>> by lazy {
    listOf(100, 110, 120, 125).map { wdth -> wdth to flexFontFamily(wdth) }
}

private fun fontFamilyForIndex(idx: Int): FontFamily = when {
    idx > 0 -> stretchedFontSteps.getOrElse(idx) { stretchedFontSteps.last() }.second
    idx < 0 -> condensedFontSteps.getOrElse(-idx) { condensedFontSteps.last() }.second
    else -> condensedFontSteps[0].second
}

// Cache settled font width index per text string across recompositions/page revisits
private val flexWidthCache = HashMap<String, Int>(64)

/** Single-line text that independently auto-adjusts wdth: stretches short text, condenses long. */
@Composable
private fun AutoFlexLine(
    text: String,
    color: Color,
    modifier: Modifier = Modifier,
) {
    val style = MaterialTheme.typography.labelLarge
    val cached = flexWidthCache[text]
    var stretchIdx by remember(text) { mutableIntStateOf(cached ?: 0) }
    var tryStretch by remember(text) { mutableIntStateOf(if (cached != null) 0 else 1) }
    val fontFamily = remember(stretchIdx) { fontFamilyForIndex(stretchIdx) }

    Text(
        text = text,
        style = style,
        fontFamily = fontFamily,
        fontWeight = FontWeight.Medium,
        color = color,
        maxLines = 1,
        textAlign = androidx.compose.ui.text.style.TextAlign.Start,
        modifier = modifier,
        onTextLayout = { result ->
            if (result.hasVisualOverflow) {
                if (tryStretch > 0) {
                    tryStretch = 0
                    stretchIdx = 1
                }
                if (-stretchIdx < condensedFontSteps.lastIndex) {
                    stretchIdx -= 1
                }
            } else if (tryStretch > 0 && tryStretch <= stretchedFontSteps.lastIndex) {
                stretchIdx = tryStretch
                tryStretch++
            } else {
                flexWidthCache[text] = stretchIdx
            }
        },
    )
}

/** Two independently flex-width lines from a balanced name split. */
@Composable
private fun AutoFlexTwoLines(
    text: String,
    color: Color,
    modifier: Modifier = Modifier,
) {
    val parts = text.split("\n", limit = 2)
    val line1 = parts[0]
    val line2 = if (parts.size > 1) parts[1].trim() else ""
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.Start,
    ) {
        AutoFlexLine(text = line1, color = color, modifier = Modifier.fillMaxWidth())
        if (line2.isNotEmpty()) {
            AutoFlexLine(text = line2, color = color, modifier = Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun ToolCategoryRow(
    label: String,
    accentColor: Color,
    presets: List<ToolPresetItem>,
    lang: String,
    onPresetClick: (String) -> Unit = {},
    onPagerSwipeLockChanged: (Boolean) -> Unit = {},
    toolbarMode: ToolbarMode = ToolbarMode.NONE,
    favoritePresetIds: Set<String> = emptySet(),
    onFavoriteToggle: (String) -> Unit = {},
    onDuplicate: (String) -> Unit = {},
    onDelete: (String) -> Unit = {},
) {
    val trailingClearance = 12.dp
    // Track presets being deleted for fade-out animation
    var deletingIds by remember { mutableStateOf(emptySet<String>()) }

    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        // Category label
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.padding(horizontal = 4.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(6.dp)
                    .background(accentColor, CircleShape),
            )
            Spacer(Modifier.width(6.dp))
            Text(
                text = label,
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.SemiBold,
                color = accentColor,
            )
        }

        val bgColor = MaterialTheme.colorScheme.background
        val fadePx = with(androidx.compose.ui.platform.LocalDensity.current) { 24.dp.toPx() }
        val carouselState = rememberCarouselState { presets.size }
        // Auto-scroll to end when a new preset is added
        val prevCount = remember { mutableIntStateOf(presets.size) }
        val scope = rememberCoroutineScope()
        LaunchedEffect(presets.size) {
            if (presets.size > prevCount.intValue) {
                scope.launch {
                    try { carouselState.animateScrollToItem(presets.lastIndex) } catch (_: Exception) {}
                }
            }
            prevCount.intValue = presets.size
        }
        val scrollFraction by remember {
            derivedStateOf {
                val max = (presets.size - 1).coerceAtLeast(1)
                carouselState.currentItem.toFloat() / max.toFloat()
            }
        }
        HorizontalUncontainedCarousel(
            state = carouselState,
            itemWidth = 150.dp,
            itemSpacing = 8.dp,
            contentPadding = PaddingValues(start = 4.dp, end = trailingClearance),
            modifier = Modifier
                .fillMaxWidth()
                .lockPagerForCarouselDrag(
                    canScrollBackward = { carouselState.canScrollBackward },
                    canScrollForward = { carouselState.canScrollForward },
                    onPagerSwipeLockChanged = onPagerSwipeLockChanged,
                )
                .drawWithContent {
                    drawContent()
                    val rightAlpha = (1f - scrollFraction).coerceIn(0f, 1f)
                    if (rightAlpha > 0.01f) {
                        drawRect(
                            brush = Brush.horizontalGradient(
                                colors = listOf(Color.Transparent, bgColor.copy(alpha = rightAlpha)),
                                startX = size.width - fadePx,
                                endX = size.width,
                            ),
                        )
                    }
                    val leftAlpha = scrollFraction.coerceIn(0f, 1f)
                    if (leftAlpha > 0.01f) {
                        drawRect(
                            brush = Brush.horizontalGradient(
                                colors = listOf(bgColor.copy(alpha = leftAlpha), Color.Transparent),
                                startX = 0f,
                                endX = fadePx,
                            ),
                        )
                    }
                },
        ) { index ->
            val preset = presets[index]
            val presetId = if (preset.isFullId) preset.id else "preset_${preset.id}"
            val isActionMode = toolbarMode != ToolbarMode.NONE
            val isFavorite = presetId in favoritePresetIds
            val isDeleting = presetId in deletingIds

            // Animate fade-out + shrink when deleting
            val deleteAlpha by animateFloatAsState(
                targetValue = if (isDeleting) 0f else 1f,
                animationSpec = spring(stiffness = Spring.StiffnessMediumLow),
                label = "del-alpha-$index",
                finishedListener = { value ->
                    if (value == 0f) {
                        deletingIds = deletingIds - presetId
                        onDelete(presetId)
                    }
                },
            )
            val deleteScale by animateFloatAsState(
                targetValue = if (isDeleting) 0.6f else 1f,
                animationSpec = spring(
                    dampingRatio = Spring.DampingRatioMediumBouncy,
                    stiffness = Spring.StiffnessMediumLow,
                ),
                label = "del-scale-$index",
            )
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .graphicsLayer {
                        alpha = deleteAlpha
                        scaleX = deleteScale
                        scaleY = deleteScale
                    }
                    .maskClip(MaterialTheme.shapes.large)
                    .clickable(enabled = !isActionMode && !isDeleting) { onPresetClick(presetId) },
            ) {
                Card(
                    modifier = Modifier.fillMaxSize(),
                    shape = MaterialTheme.shapes.large,
                    colors = CardDefaults.cardColors(
                        containerColor = accentColor.copy(alpha = 0.15f),
                    ),
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxSize()
                            .padding(horizontal = 10.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(
                            preset.icon,
                            contentDescription = null,
                            tint = accentColor,
                            modifier = Modifier.size(28.dp),
                        )
                        Spacer(Modifier.width(8.dp))
                        AutoFlexTwoLines(
                            text = preset.balancedName(lang),
                            color = MaterialTheme.colorScheme.onSurface,
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
                if (toolbarMode == ToolbarMode.FAVORITE) {
                    IconButton(
                        onClick = { onFavoriteToggle(presetId) },
                        modifier = Modifier
                            .align(Alignment.CenterEnd)
                            .padding(end = 10.dp)
                            .size(40.dp)
                            .background(
                                color = MaterialTheme.colorScheme.surface.copy(alpha = 0.88f),
                                shape = CircleShape,
                            ),
                    ) {
                        Icon(
                            imageVector = if (isFavorite) Icons.Rounded.Star else Icons.Rounded.StarOutline,
                            contentDescription = null,
                            tint = if (isFavorite) Color(0xFFFFC107) else MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                if (toolbarMode == ToolbarMode.DUPLICATE) {
                    IconButton(
                        onClick = { onDuplicate(presetId) },
                        modifier = Modifier
                            .align(Alignment.CenterEnd)
                            .padding(end = 10.dp)
                            .size(40.dp)
                            .background(
                                color = MaterialTheme.colorScheme.surface.copy(alpha = 0.88f),
                                shape = CircleShape,
                            ),
                    ) {
                        Icon(
                            imageVector = Icons.Rounded.ContentCopy,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.primary,
                        )
                    }
                }
                if (toolbarMode == ToolbarMode.DELETE && !isDeleting) {
                    IconButton(
                        onClick = { deletingIds = deletingIds + presetId },
                        modifier = Modifier
                            .align(Alignment.CenterEnd)
                            .padding(end = 10.dp)
                            .size(40.dp)
                            .background(
                                color = MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.88f),
                                shape = CircleShape,
                            ),
                    ) {
                        Icon(
                            imageVector = Icons.Rounded.Delete,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.error,
                        )
                    }
                }
            } // Box
        }
    }
}

private fun Modifier.lockPagerForCarouselDrag(
    canScrollBackward: () -> Boolean,
    canScrollForward: () -> Boolean,
    onPagerSwipeLockChanged: (Boolean) -> Unit,
): Modifier = pointerInput(onPagerSwipeLockChanged) {
    awaitEachGesture {
        awaitFirstDown(requireUnconsumed = false)
        onPagerSwipeLockChanged(
            shouldLockPagerForCarouselTouch(
                canScrollBackward = canScrollBackward(),
                canScrollForward = canScrollForward(),
            ),
        )
        try {
            while (true) {
                val event = awaitPointerEvent()
                val change = event.changes.firstOrNull() ?: break
                if (!change.pressed) break
                val deltaX = change.positionChange().x
                if (deltaX != 0f) {
                    onPagerSwipeLockChanged(
                        shouldLockPagerForCarouselTouch(
                            canScrollBackward = canScrollBackward(),
                            canScrollForward = canScrollForward(),
                        ),
                    )
                }
            }
        } finally {
            onPagerSwipeLockChanged(false)
        }
    }
}

internal fun shouldLockPagerForCarouselTouch(
    canScrollBackward: Boolean,
    canScrollForward: Boolean,
): Boolean = canScrollBackward || canScrollForward

@Composable
internal fun GlobalSection(
    apiKey: String,
    cerebrasApiKey: String,
    groqApiKey: String,
    openRouterApiKey: String,
    ollamaUrl: String,
    globalTtsSettings: MobileGlobalTtsSettings,
    presetRuntimeSettings: PresetRuntimeSettings,
    overlayOpacityPercent: Int,
    locale: MobileLocaleText,
    wideLayout: Boolean,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onGroqApiKeyChanged: (String) -> Unit,
    onOpenRouterApiKeyChanged: (String) -> Unit,
    onOllamaUrlChanged: (String) -> Unit,
    onPresetRuntimeSettingsClick: () -> Unit,
    onUsageStatsClick: () -> Unit,
    onResetDefaults: () -> Unit,
    onVoiceSettingsClick: () -> Unit,
    onOverlayOpacityChanged: (Int) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(ShellSpacing.cardGap)) {
        if (wideLayout) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ShellSpacing.cardGap),
            ) {
                CredentialsCard(
                    apiKey = apiKey,
                    cerebrasApiKey = cerebrasApiKey,
                    groqApiKey = groqApiKey,
                    openRouterApiKey = openRouterApiKey,
                    ollamaUrl = ollamaUrl,
                    locale = locale,
                    onApiKeyChanged = onApiKeyChanged,
                    onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                    onGroqApiKeyChanged = onGroqApiKeyChanged,
                    onOpenRouterApiKeyChanged = onOpenRouterApiKeyChanged,
                    onOllamaUrlChanged = onOllamaUrlChanged,
                    modifier = Modifier.weight(1.15f),
                )
                VoiceSettingsCard(
                    globalTtsSettings = globalTtsSettings,
                    locale = locale,
                    onVoiceSettingsClick = onVoiceSettingsClick,
                    modifier = Modifier.weight(0.85f),
                )
            }
            PresetRuntimeCard(
                settings = presetRuntimeSettings,
                locale = locale,
                onClick = onPresetRuntimeSettingsClick,
            )
            UsageStatsCard(
                locale = locale,
                onClick = onUsageStatsClick,
            )
        } else {
            CredentialsCard(
                apiKey = apiKey,
                cerebrasApiKey = cerebrasApiKey,
                groqApiKey = groqApiKey,
                openRouterApiKey = openRouterApiKey,
                ollamaUrl = ollamaUrl,
                locale = locale,
                onApiKeyChanged = onApiKeyChanged,
                onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                onGroqApiKeyChanged = onGroqApiKeyChanged,
                onOpenRouterApiKeyChanged = onOpenRouterApiKeyChanged,
                onOllamaUrlChanged = onOllamaUrlChanged,
                modifier = Modifier.fillMaxWidth(),
            )
            VoiceSettingsCard(
                globalTtsSettings = globalTtsSettings,
                locale = locale,
                onVoiceSettingsClick = onVoiceSettingsClick,
            )
            PresetRuntimeCard(
                settings = presetRuntimeSettings,
                locale = locale,
                onClick = onPresetRuntimeSettingsClick,
            )
            UsageStatsCard(
                locale = locale,
                onClick = onUsageStatsClick,
            )
        }
        OverlayOpacityCard(
            opacityPercent = overlayOpacityPercent,
            locale = locale,
            onOpacityChanged = onOverlayOpacityChanged,
        )
        ResetDefaultsCard(
            locale = locale,
            onClick = onResetDefaults,
        )
        DownloadedToolsSection(locale = locale)
    }
}

@Composable
internal fun PlaceholderSection(
    label: String,
    description: String,
    locale: MobileLocaleText,
) {
    Card(shape = MaterialTheme.shapes.extraLarge) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(ShellSpacing.innerPad),
            verticalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
        ) {
            StatusChip(
                label = locale.shellPlaceholderBadge,
                accent = MaterialTheme.colorScheme.outline,
            )
            Text(
                text = label,
                style = MaterialTheme.typography.titleLargeEmphasized,
            )
            Text(
                text = description,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            androidx.compose.material3.HorizontalDivider()
            Text(
                text = locale.shellPlaceholderMessage,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
