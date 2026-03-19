@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, androidx.compose.animation.ExperimentalSharedTransitionApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.ui.i18n.MobileUiLanguageOption
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.launch

internal data class ShellSectionLayoutBehavior(
    val usesOuterScroll: Boolean,
    val usesViewportFooter: Boolean,
)

internal fun MobileShellSection.layoutBehavior(): ShellSectionLayoutBehavior = when (this) {
    MobileShellSection.APPS -> ShellSectionLayoutBehavior(
        usesOuterScroll = false,
        usesViewportFooter = false,
    )
    MobileShellSection.TOOLS -> ShellSectionLayoutBehavior(
        usesOuterScroll = false,
        usesViewportFooter = true,
    )
    MobileShellSection.SETTINGS, MobileShellSection.HISTORY -> ShellSectionLayoutBehavior(
        usesOuterScroll = true,
        usesViewportFooter = false,
    )
}

@Composable
internal fun MobileShellSurface(
    state: LiveSessionState,
    apiKey: String,
    cerebrasApiKey: String,
    groqApiKey: String,
    openRouterApiKey: String,
    ollamaUrl: String,
    globalTtsSettings: MobileGlobalTtsSettings,
    presetRuntimeSettings: PresetRuntimeSettings,
    locale: MobileLocaleText,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onGroqApiKeyChanged: (String) -> Unit,
    onOpenRouterApiKeyChanged: (String) -> Unit,
    onOllamaUrlChanged: (String) -> Unit,
    onPresetRuntimeSettingsClick: () -> Unit,
    onUsageStatsClick: () -> Unit = {},
    onVoiceSettingsClick: () -> Unit,
    uiPreferences: dev.screengoated.toolbox.mobile.model.MobileUiPreferences = dev.screengoated.toolbox.mobile.model.MobileUiPreferences(),
    onOverlayOpacityChanged: (Int) -> Unit = {},
    showEmbeddedHeader: Boolean = false,
    appHeaderTitle: String = "",
    uiLanguage: String = "en",
    languageOptions: List<MobileUiLanguageOption> = emptyList(),
    onUiLanguageSelected: (String) -> Unit = {},
    themeMode: MobileThemeMode = MobileThemeMode.SYSTEM,
    onThemeCycleRequested: () -> Unit = {},
    onSessionToggle: () -> Unit,
    onDownloaderClick: () -> Unit = {},
    onDjClick: () -> Unit = {},
    onPresetClick: (String) -> Unit = {},
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope? = null,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope? = null,
) {
    val isActive = state.phase in setOf(
        SessionPhase.STARTING,
        SessionPhase.LISTENING,
        SessionPhase.TRANSLATING,
    )
    val canToggle = true

    // Remember last tab across app restarts
    val context = androidx.compose.ui.platform.LocalContext.current
    val prefs = remember { context.getSharedPreferences("sgt_shell", android.content.Context.MODE_PRIVATE) }
    var selectedSection by rememberSaveable {
        val saved = prefs.getString("last_tab", null)
        val initial = runCatching { MobileShellSection.valueOf(saved ?: "") }.getOrDefault(MobileShellSection.APPS)
        mutableStateOf(initial)
    }
    LaunchedEffect(selectedSection) {
        prefs.edit().putString("last_tab", selectedSection.name).apply()
    }

    BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
        // Wide rail layout only for portrait tablets (wide AND tall).
        // Landscape phones (wide but short) use the pager layout to avoid
        // VerticalCarousel-in-scrollable-parent crashes.
        val wideLayout = maxWidth >= 760.dp && maxHeight >= maxWidth
        if (wideLayout) {
            Row(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 20.dp, vertical = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(18.dp),
            ) {
                ShellRail(
                    selectedSection = selectedSection,
                    onSectionSelected = { selectedSection = it },
                    locale = locale,
                    modifier = Modifier.fillMaxHeight(),
                )
                val wideScrollState = rememberScrollState()
                val wideNeedsScroll = selectedSection.layoutBehavior().usesOuterScroll
                if (wideNeedsScroll) {
                    Column(
                        modifier = Modifier
                            .weight(1f)
                            .widthIn(max = 960.dp)
                            .verticalScroll(wideScrollState),
                        verticalArrangement = Arrangement.spacedBy(ShellSpacing.sectionGap),
                    ) {
                        SectionDetail(
                            selectedSection = selectedSection,
                            state = state,
                            apiKey = apiKey,
                            cerebrasApiKey = cerebrasApiKey,
                            groqApiKey = groqApiKey,
                            openRouterApiKey = openRouterApiKey,
                            ollamaUrl = ollamaUrl,
                            globalTtsSettings = globalTtsSettings,
                            presetRuntimeSettings = presetRuntimeSettings,
                            locale = locale,
                            wideLayout = true,
                            onApiKeyChanged = onApiKeyChanged,
                            onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                            onGroqApiKeyChanged = onGroqApiKeyChanged,
                            onOpenRouterApiKeyChanged = onOpenRouterApiKeyChanged,
                            onOllamaUrlChanged = onOllamaUrlChanged,
                            onPresetRuntimeSettingsClick = onPresetRuntimeSettingsClick,
                            onUsageStatsClick = onUsageStatsClick,
                            onVoiceSettingsClick = onVoiceSettingsClick,
                            uiPreferences = uiPreferences,
                            onOverlayOpacityChanged = onOverlayOpacityChanged,
                            onSessionToggle = onSessionToggle,
                            canToggle = canToggle,
                            onDownloaderClick = onDownloaderClick,
                            onDjClick = onDjClick,
                            onPresetClick = onPresetClick,
                            sharedTransitionScope = sharedTransitionScope,
                            animatedVisibilityScope = animatedVisibilityScope,
                        )
                    }
                } else {
                    Box(
                        modifier = Modifier
                            .weight(1f)
                            .widthIn(max = 960.dp)
                            .fillMaxSize(),
                    ) {
                        SectionDetail(
                            selectedSection = selectedSection,
                            state = state,
                            apiKey = apiKey,
                            cerebrasApiKey = cerebrasApiKey,
                            groqApiKey = groqApiKey,
                            openRouterApiKey = openRouterApiKey,
                            ollamaUrl = ollamaUrl,
                            globalTtsSettings = globalTtsSettings,
                            presetRuntimeSettings = presetRuntimeSettings,
                            locale = locale,
                            wideLayout = true,
                            onApiKeyChanged = onApiKeyChanged,
                            onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                            onGroqApiKeyChanged = onGroqApiKeyChanged,
                            onOpenRouterApiKeyChanged = onOpenRouterApiKeyChanged,
                            onOllamaUrlChanged = onOllamaUrlChanged,
                            onPresetRuntimeSettingsClick = onPresetRuntimeSettingsClick,
                            onUsageStatsClick = onUsageStatsClick,
                            onVoiceSettingsClick = onVoiceSettingsClick,
                            uiPreferences = uiPreferences,
                            onOverlayOpacityChanged = onOverlayOpacityChanged,
                            onSessionToggle = onSessionToggle,
                            canToggle = canToggle,
                            onDownloaderClick = onDownloaderClick,
                            onDjClick = onDjClick,
                            onPresetClick = onPresetClick,
                            sharedTransitionScope = sharedTransitionScope,
                            animatedVisibilityScope = animatedVisibilityScope,
                        )
                    }
                }
            }
        } else {
            val sections = MobileShellSection.entries
            val pagerState = rememberPagerState { sections.size }
            val scope = rememberCoroutineScope()
            var pagerSwipeLocked by remember { mutableStateOf(false) }

            var navigating by remember { mutableStateOf(false) }

            LaunchedEffect(pagerState) {
                snapshotFlow { pagerState.settledPage }.collect { page ->
                    if (!navigating) {
                        selectedSection = sections[page]
                    }
                }
            }

            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 20.dp, vertical = 4.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(ShellSpacing.sectionGap),
            ) {
                val onSectionRequested: (MobileShellSection) -> Unit = { section ->
                    if (section != selectedSection) {
                        selectedSection = section
                        navigating = true
                        scope.launch {
                            val target = section.ordinal
                            val current = pagerState.currentPage
                            val distance = kotlin.math.abs(target - current)
                            if (distance > 1) {
                                val neighbor = if (target > current) target - 1 else target + 1
                                pagerState.scrollToPage(neighbor)
                            }
                            pagerState.animateScrollToPage(target)
                            navigating = false
                        }
                    }
                }

                val tabsModifier = Modifier.pointerInput(Unit) {
                    var totalDrag = 0f
                    detectHorizontalDragGestures(
                        onDragStart = { totalDrag = 0f },
                        onHorizontalDrag = { _, dragAmount -> totalDrag += dragAmount },
                        onDragEnd = {
                            val threshold = 80f
                            val current = pagerState.currentPage
                            val target = when {
                                totalDrag < -threshold && current < sections.lastIndex -> current + 1
                                totalDrag > threshold && current > 0 -> current - 1
                                else -> null
                            }
                            if (target != null) {
                                scope.launch {
                                    pagerState.animateScrollToPage(target)
                                }
                            }
                        },
                    )
                }

                if (showEmbeddedHeader) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(top = 8.dp, start = 4.dp, end = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        LanguageMorphToggle(
                            uiLanguage = uiLanguage,
                            languageOptions = languageOptions,
                            onLanguageSelected = onUiLanguageSelected,
                        )
                        Box(
                            modifier = Modifier.weight(1f),
                            contentAlignment = Alignment.Center,
                        ) {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(10.dp),
                            ) {
                                SgtBrandBadge(size = 28.dp, showBackground = false)
                                Text(
                                    text = appHeaderTitle,
                                    style = MaterialTheme.typography.titleMedium,
                                    fontWeight = FontWeight.SemiBold,
                                    color = MaterialTheme.colorScheme.onSurface,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }
                        Box(
                            modifier = Modifier.weight(1.2f),
                            contentAlignment = Alignment.Center,
                        ) {
                            SectionSegmentedRow(
                                selectedSection = selectedSection,
                                onSectionSelected = onSectionRequested,
                                locale = locale,
                                pagerState = pagerState,
                                modifier = tabsModifier,
                            )
                        }
                        ThemeMorphToggle(
                            themeMode = themeMode,
                            onClick = onThemeCycleRequested,
                            contentDescription = "${locale.themeCycleLabel}: ${locale.themeModeLabels[themeMode]}",
                        )
                    }
                } else {
                    SectionSegmentedRow(
                        selectedSection = selectedSection,
                        onSectionSelected = onSectionRequested,
                        locale = locale,
                        pagerState = pagerState,
                        modifier = tabsModifier,
                    )
                }
                HorizontalPager(
                    state = pagerState,
                    modifier = Modifier
                        .fillMaxSize()
                        .weight(1f),
                    userScrollEnabled = !pagerSwipeLocked,
                    beyondViewportPageCount = 1,
                    pageSpacing = 16.dp,
                    key = { sections[it].name },
                ) { page ->
                    val section = sections[page]
                    val needsScroll = section.layoutBehavior().usesOuterScroll
                    val scrollState = rememberScrollState()
                    if (needsScroll) {
                        Column(
                            modifier = Modifier
                                .fillMaxSize()
                                .verticalScroll(scrollState),
                            verticalArrangement = Arrangement.spacedBy(ShellSpacing.sectionGap),
                        ) {
                            SectionDetail(
                                selectedSection = section,
                                state = state,
                                apiKey = apiKey,
                                cerebrasApiKey = cerebrasApiKey,
                                groqApiKey = groqApiKey,
                                openRouterApiKey = openRouterApiKey,
                                ollamaUrl = ollamaUrl,
                                globalTtsSettings = globalTtsSettings,
                                presetRuntimeSettings = presetRuntimeSettings,
                                locale = locale,
                                wideLayout = false,
                                onApiKeyChanged = onApiKeyChanged,
                                onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                                onGroqApiKeyChanged = onGroqApiKeyChanged,
                                onOpenRouterApiKeyChanged = onOpenRouterApiKeyChanged,
                                onOllamaUrlChanged = onOllamaUrlChanged,
                                onPresetRuntimeSettingsClick = onPresetRuntimeSettingsClick,
                                onUsageStatsClick = onUsageStatsClick,
                                onVoiceSettingsClick = onVoiceSettingsClick,
                            uiPreferences = uiPreferences,
                            onOverlayOpacityChanged = onOverlayOpacityChanged,
                                onSessionToggle = onSessionToggle,
                                canToggle = canToggle,
                                onDownloaderClick = onDownloaderClick,
                                onDjClick = onDjClick,
                                onPresetClick = onPresetClick,
                                onPagerSwipeLockChanged = { pagerSwipeLocked = it },
                                sharedTransitionScope = null,
                                animatedVisibilityScope = null,
                            )
                        }
                    } else {
                        Box(modifier = Modifier.fillMaxSize()) {
                            SectionDetail(
                                selectedSection = section,
                                state = state,
                                apiKey = apiKey,
                                cerebrasApiKey = cerebrasApiKey,
                                groqApiKey = groqApiKey,
                                openRouterApiKey = openRouterApiKey,
                                ollamaUrl = ollamaUrl,
                                globalTtsSettings = globalTtsSettings,
                                presetRuntimeSettings = presetRuntimeSettings,
                                locale = locale,
                                wideLayout = false,
                                onApiKeyChanged = onApiKeyChanged,
                                onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                                onGroqApiKeyChanged = onGroqApiKeyChanged,
                                onOpenRouterApiKeyChanged = onOpenRouterApiKeyChanged,
                                onOllamaUrlChanged = onOllamaUrlChanged,
                                onPresetRuntimeSettingsClick = onPresetRuntimeSettingsClick,
                                onUsageStatsClick = onUsageStatsClick,
                                onVoiceSettingsClick = onVoiceSettingsClick,
                            uiPreferences = uiPreferences,
                            onOverlayOpacityChanged = onOverlayOpacityChanged,
                                onSessionToggle = onSessionToggle,
                                canToggle = canToggle,
                                onDownloaderClick = onDownloaderClick,
                                onDjClick = onDjClick,
                                onPresetClick = onPresetClick,
                                onPagerSwipeLockChanged = { pagerSwipeLocked = it },
                                sharedTransitionScope = null,
                                animatedVisibilityScope = null,
                            )
                        }
                    }
                }
            }
        }
    }
}
