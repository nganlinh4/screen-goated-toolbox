@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class, ExperimentalTextApi::class, androidx.compose.animation.ExperimentalSharedTransitionApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.background
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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.HelpOutline
import androidx.compose.material.icons.automirrored.rounded.Note
import androidx.compose.material.icons.automirrored.rounded.TextSnippet
import androidx.compose.material.icons.automirrored.rounded.VolumeUp
import androidx.compose.material.icons.rounded.Apps
import androidx.compose.material.icons.rounded.AutoFixHigh
import androidx.compose.material.icons.rounded.CameraAlt
import androidx.compose.material.icons.rounded.ContentCut
import androidx.compose.material.icons.rounded.Description
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.material.icons.rounded.FiberSmartRecord
import androidx.compose.material.icons.rounded.FormatQuote
import androidx.compose.material.icons.rounded.GTranslate
import androidx.compose.material.icons.rounded.Gamepad
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
import androidx.compose.material3.FilledTonalButton
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
import androidx.compose.ui.unit.dp
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.RoundedPolygon
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
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
            Text(text = name, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold)
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
            Text(text = name, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold)
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
    globalTtsSettings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    wideLayout: Boolean,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onVoiceSettingsClick: () -> Unit,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
    onDownloaderClick: () -> Unit = {},
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
            sharedTransitionScope = sharedTransitionScope,
            animatedVisibilityScope = animatedVisibilityScope,
        )

        MobileShellSection.TOOLS -> ToolsSection(locale = locale)

        MobileShellSection.SETTINGS -> GlobalSection(
            apiKey = apiKey,
            cerebrasApiKey = cerebrasApiKey,
            globalTtsSettings = globalTtsSettings,
            locale = locale,
            wideLayout = wideLayout,
            onApiKeyChanged = onApiKeyChanged,
            onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
            onVoiceSettingsClick = onVoiceSettingsClick,
        )

        MobileShellSection.HISTORY -> PlaceholderSection(
            label = locale.shellHistoryLabel,
            description = locale.shellHistoryDescription,
            locale = locale,
        )
    }
}

private data class AppSlot(val shape: RoundedPolygon, val color: Color)

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
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope? = null,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope? = null,
) {
    val isLandscape = LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE

    if (isLandscape) {
        AppsHorizontalCarousel(state, locale, onSessionToggle, canToggle, onDownloaderClick, sharedTransitionScope, animatedVisibilityScope)
    } else {
        AppsVerticalCarousel(state, locale, onSessionToggle, canToggle, onDownloaderClick, sharedTransitionScope, animatedVisibilityScope)
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
                    else -> Modifier
                },
            ),
    ) {
        when (index) {
            0 -> LiveTranslateCarouselTile(state = state, locale = locale, onSessionToggle = onSessionToggle, canToggle = canToggle)
            1 -> AppTile(slot = appSlots[1], title = locale.appVideoDownloaderTitle, icon = Icons.Rounded.Download)
            2 -> AppTile(slot = appSlots[2], title = locale.appDjTitle, icon = Icons.Rounded.GraphicEq)
            else -> AppTile(slot = appSlots[index], title = locale.comingSoonLabel, icon = null)
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
                AppsItemContent(index, state, locale, onSessionToggle, canToggle, onDownloaderClick, sharedTransitionScope, animatedVisibilityScope)
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
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope?,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope?,
) {
    val screenH = LocalConfiguration.current.screenHeightDp.dp
    val itemWidth = 200.dp
    val carouselHeight = (screenH - 100.dp).coerceIn(160.dp, 300.dp)
    val fadeSize = 24.dp
    val bgColor = MaterialTheme.colorScheme.background

    Box(modifier = Modifier.fillMaxWidth().height(carouselHeight)) {
        HorizontalUncontainedCarousel(
            state = rememberCarouselState { appSlots.size },
            itemWidth = itemWidth,
            modifier = Modifier.fillMaxSize(),
            itemSpacing = 8.dp,
            contentPadding = PaddingValues(start = 4.dp, end = fadeSize),
        ) { index ->
            Card(
                modifier = Modifier.fillMaxSize().maskClip(MaterialTheme.shapes.extraLarge),
                shape = MaterialTheme.shapes.extraLarge,
                colors = CardDefaults.cardColors(containerColor = appSlots[index].color.copy(alpha = 0.15f)),
            ) {
                AppsItemContent(index, state, locale, onSessionToggle, canToggle, onDownloaderClick, sharedTransitionScope, animatedVisibilityScope)
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
    val slot = appSlots[0]
    val morph = remember { Morph(slot.shape, slot.shape) }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(if (isRunning) slot.color.copy(alpha = 0.30f) else slot.color.copy(alpha = 0.15f)),
    ) {
        Canvas(
            modifier = Modifier
                .size(130.dp)
                .align(Alignment.CenterEnd),
        ) {
            val path = morph.toPath(progress = 0f)
            val s = size.minDimension
            val bounds = path.getBounds()
            val pathSize = maxOf(bounds.width, bounds.height).takeIf { it > 0f } ?: 1f
            val cx = bounds.left + bounds.width / 2f
            val cy = bounds.top + bounds.height / 2f
            val scale = s * 0.85f / pathSize
            val matrix = Matrix()
            matrix.translate(s / 2f, s / 2f)
            matrix.scale(scale, scale)
            matrix.translate(-cx, -cy)
            path.transform(matrix)
            drawPath(path, color = slot.color.copy(alpha = 0.28f))
        }
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

@Composable
private fun AppTile(
    slot: AppSlot,
    title: String,
    icon: ImageVector?,
) {
    val morph = remember(slot) { Morph(slot.shape, slot.shape) }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(slot.color.copy(alpha = 0.15f)),
    ) {
        Canvas(
            modifier = Modifier
                .size(100.dp)
                .align(Alignment.CenterEnd),
        ) {
            val path = morph.toPath(progress = 0f)
            val s = size.minDimension
            val bounds = path.getBounds()
            val pathSize = maxOf(bounds.width, bounds.height).takeIf { it > 0f } ?: 1f
            val cx = bounds.left + bounds.width / 2f
            val cy = bounds.top + bounds.height / 2f
            val scale = s * 0.85f / pathSize
            val matrix = Matrix()
            matrix.translate(s / 2f, s / 2f)
            matrix.scale(scale, scale)
            matrix.translate(-cx, -cy)
            path.transform(matrix)
            drawPath(path, color = slot.color.copy(alpha = 0.28f))
        }
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
                    modifier = Modifier.size(36.dp),
                )
                Spacer(Modifier.width(14.dp))
            }
            Text(
                text = title,
                fontWeight = FontWeight.Bold,
                fontSize = 22.sp,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Tools Section — mirrors Windows sidebar preset categories
// ---------------------------------------------------------------------------

private data class ToolPresetItem(
    val id: String,
    val nameEn: String,
    val nameVi: String,
    val nameKo: String,
    val icon: ImageVector,
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
)

private val toolCategories = listOf(
    ToolCategory(
        labelGetter = { it.toolsCategoryImage },
        accentColor = Color(0xFF5C9CE6),
        presets = listOf(
            ToolPresetItem("translate", "Translate region", "Dịch vùng", "영역 번역", Icons.Rounded.Translate),
            ToolPresetItem("extract_retranslate", "Trans (ACCURATE)", "Dịch vùng (CHUẨN)", "영역 번역 (정확)", Icons.Rounded.Verified),
            ToolPresetItem("translate_auto_paste", "Trans (Auto paste)", "Dịch vùng (Tự dán)", "영역 번역 (자동 붙.)", Icons.Rounded.ContentCut),
            ToolPresetItem("ocr", "Extract text", "Lấy text từ ảnh", "텍스트 추출", Icons.Rounded.TextFields),
            ToolPresetItem("ocr_read", "Read this region", "Đọc vùng này", "영역 읽기", Icons.AutoMirrored.Rounded.VolumeUp),
            ToolPresetItem("desc", "Describe image", "Mô tả ảnh", "이미지 설명", Icons.Rounded.Description),
            ToolPresetItem("ask_image", "Ask about image", "Hỏi về ảnh", "이미지 질문", Icons.Rounded.ImageSearch),
            ToolPresetItem("summarize", "Summarize region", "Tóm tắt vùng", "영역 요약", Icons.Rounded.Summarize),
            ToolPresetItem("extract_table", "Extract Table", "Trích bảng", "표 추출", Icons.Rounded.TableChart),
            ToolPresetItem("fact_check", "Fact Check", "Kiểm chứng", "정보 확인", Icons.Rounded.Verified),
            ToolPresetItem("quick_screenshot", "Quick Screenshot", "Chụp MH nhanh", "빠른 스크린샷", Icons.Rounded.PhotoCamera),
            ToolPresetItem("qr_scanner", "QR Scanner", "Quét mã QR", "QR 스캔", Icons.Rounded.QrCodeScanner),
            ToolPresetItem("hang_image", "Image Overlay", "Treo ảnh", "이미지 오버레이", Icons.Rounded.CameraAlt),
        ),
    ),
    ToolCategory(
        labelGetter = { it.toolsCategoryTextSelect },
        accentColor = Color(0xFF5DB882),
        presets = listOf(
            ToolPresetItem("translate_select", "Translate", "Dịch", "번역", Icons.Rounded.GTranslate),
            ToolPresetItem("read_aloud", "Read aloud", "Đọc to", "크게 읽기", Icons.Rounded.RecordVoiceOver),
            ToolPresetItem("translate_arena", "Trans (Arena)", "Dịch (Arena)", "번역 (아레나)", Icons.Rounded.Translate),
            ToolPresetItem("fix_grammar", "Fix Grammar", "Sửa ngữ pháp", "문법 수정", Icons.Rounded.Spellcheck),
            ToolPresetItem("rephrase", "Rephrase", "Viết lại", "다시 쓰기", Icons.Rounded.FormatQuote),
            ToolPresetItem("make_formal", "Make Formal", "Chuyên nghiệp hóa", "공식적으로", Icons.Rounded.AutoFixHigh),
            ToolPresetItem("explain", "Explain", "Giải thích", "설명", Icons.Rounded.Lightbulb),
            ToolPresetItem("ask_text", "Ask about text", "Hỏi về text", "텍스트 질문", Icons.Rounded.QuestionAnswer),
            ToolPresetItem("edit_as_follows", "Edit as follows", "Sửa như sau", "다음과 같이 수정", Icons.Rounded.Edit),
            ToolPresetItem("101_on_this", "101 on this", "Tất tần tật", "이것의 모든 것", Icons.Rounded.School),
            ToolPresetItem("hang_text", "Text Overlay", "Treo text", "텍스트 오버레이", Icons.AutoMirrored.Rounded.TextSnippet),
        ),
    ),
    ToolCategory(
        labelGetter = { it.toolsCategoryTextInput },
        accentColor = Color(0xFF5DB882),
        presets = listOf(
            ToolPresetItem("ask_ai", "Ask AI", "Hỏi AI", "AI 질문", Icons.Rounded.SmartToy),
            ToolPresetItem("internet_search", "Internet Search", "Tìm kiếm internet", "인터넷 검색", Icons.Rounded.Search),
            ToolPresetItem("make_game", "Make a Game", "Tạo con game", "게임 만들기", Icons.Rounded.Gamepad),
            ToolPresetItem("quick_note", "Quick Note", "Note nhanh", "빠른 메모", Icons.AutoMirrored.Rounded.Note),
            ToolPresetItem("trans_retrans_typing", "Trans+Retrans", "Dịch+Dịch lại", "번역+재번역", Icons.Rounded.Translate),
        ),
    ),
    ToolCategory(
        labelGetter = { it.toolsCategoryMicRecording },
        accentColor = Color(0xFFDCA850),
        presets = listOf(
            ToolPresetItem("transcribe", "Transcribe speech", "Lời nói thành văn", "음성 받아쓰기", Icons.Rounded.Mic),
            ToolPresetItem("fix_pronunciation", "Fix pronunciation", "Chỉnh phát âm", "발음 교정", Icons.Rounded.RecordVoiceOver),
            ToolPresetItem("quick_ai_question", "Quick AI Question", "Hỏi nhanh AI", "빠른 AI 질문", Icons.Rounded.VoiceChat),
            ToolPresetItem("voice_search", "Voice Search", "Nói để search", "음성 검색", Icons.Rounded.Search),
            ToolPresetItem("quick_record", "Quick Record", "Thu âm nhanh", "빠른 녹음", Icons.Rounded.FiberSmartRecord),
            ToolPresetItem("transcribe_retranslate", "Quick 4NR reply", "Trả lời ng.nc.ngoài", "빠른 외국인 답변", Icons.Rounded.Translate),
        ),
    ),
    ToolCategory(
        labelGetter = { it.toolsCategoryDeviceAudio },
        accentColor = Color(0xFFDCA850),
        presets = listOf(
            ToolPresetItem("realtime_audio_translate", "Live Translate", "Dịch cabin", "실시간 음성 번역", Icons.Rounded.Hearing),
            ToolPresetItem("study_language", "Study language", "Học ngoại ngữ", "언어 학습", Icons.Rounded.School),
            ToolPresetItem("record_device", "Device Record", "Thu âm máy", "시스템 녹음", Icons.Rounded.SpeakerPhone),
            ToolPresetItem("continuous_writing_online", "Continuous Writing", "Viết liên tục", "연속 입력", Icons.Rounded.Keyboard),
            ToolPresetItem("transcribe_english_offline", "Transcribe English", "Chép lời TA", "영어 받아쓰기", Icons.Rounded.GraphicEq),
        ),
    ),
)

@Composable
internal fun ToolsSection(locale: MobileLocaleText) {
    val lang = locale.languageOptions.firstOrNull { it.label.contains("English") }?.let { null }
        ?: locale.let {
            when {
                it.turnOn == "Bật" -> "vi"
                it.turnOn == "켜기" -> "ko"
                else -> "en"
            }
        }
    Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
        toolCategories.forEach { category ->
            ToolCategoryRow(
                label = category.labelGetter(locale),
                accentColor = category.accentColor,
                presets = category.presets,
                lang = lang,
            )
        }
    }
}

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
) {
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
            modifier = Modifier
                .fillMaxWidth()
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
            Card(
                modifier = Modifier
                    .fillMaxSize()
                    .maskClip(MaterialTheme.shapes.large),
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
        }
    }
}

@Composable
internal fun GlobalSection(
    apiKey: String,
    cerebrasApiKey: String,
    globalTtsSettings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    wideLayout: Boolean,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onVoiceSettingsClick: () -> Unit,
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
                    locale = locale,
                    onApiKeyChanged = onApiKeyChanged,
                    onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                    modifier = Modifier.weight(1.15f),
                )
                VoiceSettingsCard(
                    globalTtsSettings = globalTtsSettings,
                    locale = locale,
                    onVoiceSettingsClick = onVoiceSettingsClick,
                    modifier = Modifier.weight(0.85f),
                )
            }
        } else {
            CredentialsCard(
                apiKey = apiKey,
                cerebrasApiKey = cerebrasApiKey,
                locale = locale,
                onApiKeyChanged = onApiKeyChanged,
                onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                modifier = Modifier.fillMaxWidth(),
            )
            VoiceSettingsCard(
                globalTtsSettings = globalTtsSettings,
                locale = locale,
                onVoiceSettingsClick = onVoiceSettingsClick,
            )
        }
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
