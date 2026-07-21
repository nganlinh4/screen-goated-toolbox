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
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
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
import androidx.annotation.DrawableRes
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ButtonGroupDefaults
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
import androidx.compose.material3.toPath
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import kotlinx.coroutines.launch
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Matrix
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.RoundedPolygon
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.updater.AppUpdateUiState

internal enum class MobileShellSection(@DrawableRes val icon: Int) { APPS(R.drawable.ms_grid_view), TOOLS(R.drawable.ms_apps), SETTINGS(R.drawable.ms_settings), HISTORY(R.drawable.ms_history);
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
                val fraction by remember(pagerState, selectedSection, section) {
                    androidx.compose.runtime.derivedStateOf {
                        if (pagerState != null && pagerState.isScrollInProgress) {
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
                    }
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
                    modifier = Modifier
                        .testTag("shell-tab-${section.name.lowercase()}")
                        .graphicsLayer {
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
                                        painterResource(section.icon),
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
internal fun SectionDetail(
    selectedSection: MobileShellSection,
    state: LiveSessionState,
    providerKeys: ProviderKeysState,
    globalTtsSettings: MobileGlobalTtsSettings,
    presetRuntimeSettings: PresetRuntimeSettings,
    historyBundle: HistoryUiBundle,
    appUpdateState: AppUpdateUiState,
    locale: MobileLocaleText,
    wideLayout: Boolean,
    settingsActions: SettingsActions,
    navActions: ShellNavActions,
    uiPreferences: dev.screengoated.toolbox.mobile.model.MobileUiPreferences = dev.screengoated.toolbox.mobile.model.MobileUiPreferences(),
    canToggle: Boolean,
    onPagerSwipeLockChanged: (Boolean) -> Unit = {},
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope? = null,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope? = null,
) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .testTag("shell-section-${selectedSection.name.lowercase()}"),
    ) {
        when (selectedSection) {
            MobileShellSection.APPS -> AppsCarouselSection(
                state = state,
                locale = locale,
                onSessionToggle = navActions.onSessionToggle,
                canToggle = canToggle,
                onDownloaderClick = navActions.onDownloaderClick,
                onDjClick = navActions.onDjClick,
                onTranslationGummyClick = navActions.onTranslationGummyClick,
                onImageTo3dClick = navActions.onImageTo3dClick,
                onImageToSvgClick = navActions.onImageToSvgClick,
                onPagerSwipeLockChanged = onPagerSwipeLockChanged,
                sharedTransitionScope = sharedTransitionScope,
                animatedVisibilityScope = animatedVisibilityScope,
            )

            MobileShellSection.TOOLS -> ToolsSection(
                locale = locale,
                onPresetClick = navActions.onPresetClick,
                onPagerSwipeLockChanged = onPagerSwipeLockChanged,
                modifier = Modifier.fillMaxSize(),
            )

            MobileShellSection.SETTINGS -> GlobalSection(
                apiKey = providerKeys.apiKey,
                cerebrasApiKey = providerKeys.cerebrasApiKey,
                groqApiKey = providerKeys.groqApiKey,
                openRouterApiKey = providerKeys.openRouterApiKey,
                ollamaUrl = providerKeys.ollamaUrl,
                globalTtsSettings = globalTtsSettings,
                presetRuntimeSettings = presetRuntimeSettings,
                overlayOpacityPercent = uiPreferences.overlayOpacityPercent,
                appUpdateState = appUpdateState,
                locale = locale,
                wideLayout = wideLayout,
                onApiKeyChanged = providerKeys.onApiKeyChanged,
                onCerebrasApiKeyChanged = providerKeys.onCerebrasApiKeyChanged,
                onGroqApiKeyChanged = providerKeys.onGroqApiKeyChanged,
                onOpenRouterApiKeyChanged = providerKeys.onOpenRouterApiKeyChanged,
                onOllamaUrlChanged = providerKeys.onOllamaUrlChanged,
                onPresetRuntimeSettingsClick = settingsActions.onPresetRuntimeSettingsClick,
                onCustomModelsClick = settingsActions.onCustomModelsClick,
                onUsageStatsClick = settingsActions.onUsageStatsClick,
                onDownloadedToolsClick = settingsActions.onDownloadedToolsClick,
                onResetDefaults = settingsActions.onResetDefaults,
                onVoiceSettingsClick = settingsActions.onVoiceSettingsClick,
                onOverlayOpacityChanged = settingsActions.onOverlayOpacityChanged,
                onCheckForAppUpdates = settingsActions.onCheckForAppUpdates,
            )

            MobileShellSection.HISTORY -> HistorySection(
                state = historyBundle.state,
                searchQuery = historyBundle.searchQuery,
                locale = locale,
                onSearchQueryChanged = historyBundle.onSearchQueryChanged,
                onClearSearchQuery = historyBundle.onClearSearchQuery,
                onMaxItemsChanged = historyBundle.onMaxItemsChanged,
                onDeleteItem = historyBundle.onDeleteItem,
                onClearAll = historyBundle.onClearItems,
            )
        }
    }
}
