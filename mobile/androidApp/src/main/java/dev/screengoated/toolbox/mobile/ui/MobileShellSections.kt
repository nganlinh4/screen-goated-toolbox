@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class, ExperimentalTextApi::class, androidx.compose.animation.ExperimentalSharedTransitionApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.animateColorAsState
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
import androidx.compose.material.icons.rounded.BarChart
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
import androidx.compose.material3.FloatingToolbarDefaults
import androidx.compose.material3.IconButton
import androidx.compose.material3.ToggleFloatingActionButton
import androidx.compose.material3.ToggleFloatingActionButtonDefaults.animateIcon
import androidx.compose.material3.animateFloatingActionButton
import androidx.compose.material3.HorizontalFloatingToolbar
import androidx.compose.material3.carousel.HorizontalUncontainedCarousel
import androidx.compose.material3.carousel.rememberCarouselState
import androidx.compose.material3.ExperimentalMaterial3Api
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import kotlinx.coroutines.launch
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.drawWithContent
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
import dev.screengoated.toolbox.mobile.history.HistoryUiState
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.updater.AppUpdateUiState

internal enum class MobileShellSection(val icon: ImageVector) { APPS(Icons.Rounded.GridView), TOOLS(Icons.Rounded.Apps), SETTINGS(Icons.Rounded.Settings), HISTORY(Icons.Rounded.History);
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
    val activeBg = MaterialTheme.colorScheme.primaryContainer
    val inactiveBg = MaterialTheme.colorScheme.surfaceContainerHigh.copy(alpha = 0.62f)
    val activeContent = MaterialTheme.colorScheme.onPrimaryContainer
    val inactiveContent = MaterialTheme.colorScheme.onSurfaceVariant

    HorizontalFloatingToolbar(
        expanded = true,
        modifier = modifier,
        content = {
            sections.forEachIndexed { index, section ->
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
                val iconBg = androidx.compose.ui.graphics.lerp(
                    MaterialTheme.colorScheme.surfaceContainerHighest.copy(alpha = 0.62f),
                    MaterialTheme.colorScheme.secondaryContainer,
                    fraction,
                )
                val scale by animateFloatAsState(
                    targetValue = 0.95f + (fraction * 0.05f),
                    animationSpec = spring(
                        dampingRatio = Spring.DampingRatioMediumBouncy,
                        stiffness = Spring.StiffnessMediumLow,
                    ),
                    label = "section-pill-$index",
                )

                val isActive = fraction > 0.5f
                androidx.compose.material3.Surface(
                    onClick = { onSectionSelected(section) },
                    color = bgColor,
                    contentColor = contentColor,
                    tonalElevation = if (fraction > 0f) 3.dp else 0.dp,
                    shadowElevation = if (fraction > 0.6f) 8.dp else 0.dp,
                    shape = MaterialTheme.shapes.extraLarge,
                    modifier = Modifier.graphicsLayer {
                        scaleX = scale
                        scaleY = scale
                    },
                ) {
                    Row(
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
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
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Box(
                                    modifier = Modifier
                                        .size(28.dp)
                                        .background(iconBg, CircleShape),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    Icon(
                                        section.icon,
                                        contentDescription = null,
                                        modifier = Modifier.size(16.dp),
                                    )
                                }
                                Spacer(Modifier.width(8.dp))
                            }
                        }
                        Text(
                            text = section.label(locale),
                            maxLines = 1,
                            style = MaterialTheme.typography.labelLarge.copy(
                                fontFamily = condensedFontSteps[if (isActive) 1 else 2].second,
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
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow.copy(alpha = 0.92f),
        ),
    ) {
        Box(
            modifier = Modifier
                .fillMaxHeight()
                .background(
                    Brush.verticalGradient(
                        listOf(
                            MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.16f),
                            MaterialTheme.colorScheme.surfaceContainerLow,
                            MaterialTheme.colorScheme.tertiaryContainer.copy(alpha = 0.1f),
                        ),
                    ),
                ),
        ) {
            WideNavigationRail(
                state = railState,
                modifier = Modifier.fillMaxHeight(),
                header = {
                    Column(
                        modifier = Modifier.padding(horizontal = 18.dp, vertical = ShellSpacing.innerPad),
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        Text(
                            text = locale.shellSectionTitle,
                            style = MaterialTheme.typography.labelLargeEmphasized,
                        )
                        StatusChip(
                            label = selectedSection.label(locale),
                            accent = MaterialTheme.colorScheme.primary,
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
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow.copy(alpha = 0.92f),
        ),
    ) {
        Box(
            modifier = Modifier.background(
                Brush.linearGradient(
                    listOf(
                        MaterialTheme.colorScheme.surfaceContainerLow,
                        MaterialTheme.colorScheme.surfaceContainerHighest.copy(alpha = 0.9f),
                    ),
                ),
            ),
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(ShellSpacing.innerPad),
                verticalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
            ) {
                Box(
                    modifier = Modifier
                        .size(42.dp)
                        .background(
                            brush = Brush.radialGradient(
                                listOf(
                                    MaterialTheme.colorScheme.surfaceBright,
                                    MaterialTheme.colorScheme.surfaceContainerHighest,
                                ),
                            ),
                            shape = MaterialTheme.shapes.large,
                        ),
                    contentAlignment = Alignment.Center,
                ) {
                    GradientMaskedIcon(icon, brush, modifier = Modifier.size(24.dp))
                }
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
    historyState: HistoryUiState,
    historySearchQuery: String,
    appUpdateState: AppUpdateUiState,
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
    onHistorySearchQueryChanged: (String) -> Unit = {},
    onClearHistorySearchQuery: () -> Unit = {},
    onHistoryMaxItemsChanged: (Int) -> Unit = {},
    onDeleteHistoryItem: (Long) -> Unit = {},
    onClearHistoryItems: () -> Unit = {},
    onCheckForAppUpdates: () -> Unit = {},
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
            appUpdateState = appUpdateState,
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
            onCheckForAppUpdates = onCheckForAppUpdates,
        )

        MobileShellSection.HISTORY -> HistorySection(
            state = historyState,
            searchQuery = historySearchQuery,
            locale = locale,
            onSearchQueryChanged = onHistorySearchQueryChanged,
            onClearSearchQuery = onClearHistorySearchQuery,
            onMaxItemsChanged = onHistoryMaxItemsChanged,
            onDeleteItem = onDeleteHistoryItem,
            onClearAll = onClearHistoryItems,
        )
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
