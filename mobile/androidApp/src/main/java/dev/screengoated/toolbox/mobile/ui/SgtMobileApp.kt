@file:OptIn(
    ExperimentalMaterial3Api::class,
    ExperimentalMaterial3ExpressiveApi::class,
    ExperimentalSharedTransitionApi::class,
)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.ExperimentalSharedTransitionApi
import androidx.compose.animation.SharedTransitionLayout
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.Alignment
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.bilingualrelay.BilingualRelayScreen
import dev.screengoated.toolbox.mobile.history.HistoryUiState
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.service.tts.EdgeVoiceCatalogState
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors
import dev.screengoated.toolbox.mobile.updater.AppUpdateUiState

@Composable
fun SgtMobileApp(
    state: LiveSessionState,
    apiKey: String,
    cerebrasApiKey: String,
    groqApiKey: String,
    openRouterApiKey: String,
    ollamaUrl: String,
    globalTtsSettings: MobileGlobalTtsSettings,
    presetRuntimeSettings: PresetRuntimeSettings,
    uiPreferences: MobileUiPreferences,
    locale: MobileLocaleText,
    historyState: HistoryUiState,
    historySearchQuery: String,
    appUpdateState: AppUpdateUiState,
    edgeVoiceCatalogState: EdgeVoiceCatalogState,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onGroqApiKeyChanged: (String) -> Unit,
    onOpenRouterApiKeyChanged: (String) -> Unit,
    onOllamaUrlChanged: (String) -> Unit,
    onPresetRuntimeSettingsChanged: (PresetRuntimeSettings) -> Unit,
    onUiLanguageSelected: (String) -> Unit,
    onThemeCycleRequested: () -> Unit,
    onGlobalTtsMethodChanged: (MobileTtsMethod) -> Unit,
    onGlobalTtsModelChanged: (String) -> Unit,
    onGlobalTtsSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
    onGlobalTtsVoiceChanged: (String) -> Unit,
    onGlobalTtsConditionsChanged: (List<MobileTtsLanguageCondition>) -> Unit,
    onGlobalEdgeTtsSettingsChanged: (MobileEdgeTtsSettings) -> Unit,
    onVoiceSettingsShown: () -> Unit,
    onRetryEdgeVoiceCatalog: () -> Unit,
    onPreviewGeminiVoice: (String) -> Unit,
    onPreviewEdgeVoice: (String, String) -> Unit,
    onPreviewGoogleTranslate: () -> Unit,
    onSessionToggle: () -> Unit,
    onHistorySearchQueryChanged: (String) -> Unit,
    onClearHistorySearchQuery: () -> Unit,
    onHistoryMaxItemsChanged: (Int) -> Unit,
    onResetHistoryDefaults: () -> Unit,
    onDeleteHistoryItem: (Long) -> Unit,
    onClearHistoryItems: () -> Unit,
    onCheckForAppUpdates: () -> Unit,
    onOverlayOpacityChanged: (Int) -> Unit = {},
) {
    var showTtsSettings by rememberSaveable { mutableStateOf(false) }
    var showPresetRuntimeSettings by rememberSaveable { mutableStateOf(false) }
    var showUsageStats by rememberSaveable { mutableStateOf(false) }
    var showDownloader by rememberSaveable { mutableStateOf(false) }
    var showDj by rememberSaveable { mutableStateOf(false) }
    var showBilingualRelay by rememberSaveable { mutableStateOf(false) }
    var activePresetId by rememberSaveable { mutableStateOf<String?>(null) }
    val presetRepository = (LocalContext.current.applicationContext as SgtMobileApplication)
        .appContainer
        .presetRepository
    val presetCatalog by presetRepository.catalogState.collectAsState()

    if (showTtsSettings) {
        GlobalTtsSettingsDialog(
            settings = globalTtsSettings,
            locale = locale,
            edgeVoiceCatalogState = edgeVoiceCatalogState,
            onDismiss = { showTtsSettings = false },
            onMethodChanged = onGlobalTtsMethodChanged,
            onGeminiModelChanged = onGlobalTtsModelChanged,
            onSpeedPresetChanged = onGlobalTtsSpeedPresetChanged,
            onVoiceChanged = onGlobalTtsVoiceChanged,
            onConditionsChanged = onGlobalTtsConditionsChanged,
            onEdgeSettingsChanged = onGlobalEdgeTtsSettingsChanged,
            onRetryEdgeVoiceCatalog = onRetryEdgeVoiceCatalog,
            onPreviewGeminiVoice = onPreviewGeminiVoice,
            onPreviewEdgeVoice = onPreviewEdgeVoice,
            onPreviewGoogleTranslate = onPreviewGoogleTranslate,
        )
    }

    if (showPresetRuntimeSettings) {
        PresetRuntimeSettingsDialog(
            settings = presetRuntimeSettings,
            locale = locale,
            uiLanguage = uiPreferences.uiLanguage,
            onDismiss = { showPresetRuntimeSettings = false },
            onSave = { onPresetRuntimeSettingsChanged(it) },
        )
    }

    if (showUsageStats) {
        UsageStatsDialog(
            locale = locale,
            providerSettings = presetRuntimeSettings.providerSettings,
            lang = uiPreferences.uiLanguage,
            onDismiss = { showUsageStats = false },
        )
    }

    val focusManager = LocalFocusManager.current
    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.surface)
            .pointerInput(Unit) { detectTapGestures(onTap = { focusManager.clearFocus() }) },
    ) {
        Scaffold(
            containerColor = Color.Transparent,
            topBar = {
                if (!isLandscape) {
                    CenterAlignedTopAppBar(
                        colors = TopAppBarDefaults.topAppBarColors(
                            containerColor = Color.Transparent,
                        ),
                        title = {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(10.dp),
                            ) {
                                SgtBrandBadge(
                                    size = 28.dp,
                                    showBackground = false,
                                )
                                AppHeaderTitleLink(
                                    title = locale.appHeaderTitle,
                                )
                            }
                        },
                        navigationIcon = {
                            LanguageMorphToggle(
                                uiLanguage = uiPreferences.uiLanguage,
                                languageOptions = locale.languageOptions,
                                onLanguageSelected = onUiLanguageSelected,
                            )
                        },
                        actions = {
                            ThemeMorphToggle(
                                themeMode = uiPreferences.themeMode,
                                onClick = onThemeCycleRequested,
                                contentDescription = "${locale.themeCycleLabel}: ${locale.themeModeLabels[uiPreferences.themeMode]}",
                            )
                        },
                    )
                }
            },
        ) { innerPadding ->
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(innerPadding),
            ) {
                MobileShellSurface(
                    state = state,
                    apiKey = apiKey,
                    cerebrasApiKey = cerebrasApiKey,
                    groqApiKey = groqApiKey,
                    openRouterApiKey = openRouterApiKey,
                    ollamaUrl = ollamaUrl,
                    globalTtsSettings = globalTtsSettings,
                    presetRuntimeSettings = presetRuntimeSettings,
                    locale = locale,
                    historyState = historyState,
                    historySearchQuery = historySearchQuery,
                    appUpdateState = appUpdateState,
                    onApiKeyChanged = onApiKeyChanged,
                    onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                    onGroqApiKeyChanged = onGroqApiKeyChanged,
                    onOpenRouterApiKeyChanged = onOpenRouterApiKeyChanged,
                    onOllamaUrlChanged = onOllamaUrlChanged,
                    onPresetRuntimeSettingsClick = { showPresetRuntimeSettings = true },
                    onUsageStatsClick = { showUsageStats = true },
                    onResetDefaults = {
                        // Match Windows reset scope: reset everything except API keys and language
                        presetRepository.resetAllToDefaults()
                        onPresetRuntimeSettingsChanged(PresetRuntimeSettings())
                        onOverlayOpacityChanged(85)
                        onResetHistoryDefaults()
                        // Reset TTS to defaults
                        onGlobalTtsMethodChanged(dev.screengoated.toolbox.mobile.model.MobileTtsMethod.GEMINI_LIVE)
                        onGlobalTtsSpeedPresetChanged(dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset.FAST)
                        onGlobalTtsVoiceChanged("Aoede")
                        onGlobalEdgeTtsSettingsChanged(dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings())
                    },
                    onVoiceSettingsClick = {
                        onVoiceSettingsShown()
                        showTtsSettings = true
                    },
                    uiPreferences = uiPreferences,
                    onOverlayOpacityChanged = onOverlayOpacityChanged,
                    showEmbeddedHeader = isLandscape,
                    appHeaderTitle = locale.appHeaderTitle,
                    uiLanguage = uiPreferences.uiLanguage,
                    languageOptions = locale.languageOptions,
                    onUiLanguageSelected = onUiLanguageSelected,
                    themeMode = uiPreferences.themeMode,
                    onThemeCycleRequested = onThemeCycleRequested,
                    onSessionToggle = onSessionToggle,
                    onDownloaderClick = { showDownloader = true },
                    onDjClick = { showDj = true },
                    onBilingualRelayClick = { showBilingualRelay = true },
                    onPresetClick = { presetId -> activePresetId = presetId },
                    onHistorySearchQueryChanged = onHistorySearchQueryChanged,
                    onClearHistorySearchQuery = onClearHistorySearchQuery,
                    onHistoryMaxItemsChanged = onHistoryMaxItemsChanged,
                    onDeleteHistoryItem = onDeleteHistoryItem,
                    onClearHistoryItems = onClearHistoryItems,
                    onCheckForAppUpdates = onCheckForAppUpdates,
                )
            }
        }

        // Downloader overlay with container-transform-style animation
        if (showDownloader) {
            androidx.activity.compose.BackHandler { showDownloader = false }
        }
        androidx.compose.animation.AnimatedVisibility(
            visible = showDownloader,
            enter = fadeIn(tween(200)) + androidx.compose.animation.scaleIn(
                initialScale = 0.8f,
                animationSpec = tween(350, easing = androidx.compose.animation.core.FastOutSlowInEasing),
            ),
            exit = fadeOut(tween(150)) + androidx.compose.animation.scaleOut(
                targetScale = 0.8f,
                animationSpec = tween(250, easing = androidx.compose.animation.core.FastOutSlowInEasing),
            ),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(MaterialTheme.colorScheme.surface),
            ) {
                DownloaderScreenWrapper(locale = locale, onBack = { showDownloader = false })
            }
        }

        // DJ overlay
        if (showDj) {
            androidx.activity.compose.BackHandler { showDj = false }
        }
        androidx.compose.animation.AnimatedVisibility(
            visible = showDj,
            enter = fadeIn(tween(200)) + androidx.compose.animation.scaleIn(
                initialScale = 0.8f,
                animationSpec = tween(350, easing = androidx.compose.animation.core.FastOutSlowInEasing),
            ),
            exit = fadeOut(tween(150)) + androidx.compose.animation.scaleOut(
                targetScale = 0.8f,
                animationSpec = tween(250, easing = androidx.compose.animation.core.FastOutSlowInEasing),
            ),
        ) {
            val isDjDark = when (uiPreferences.themeMode) {
                MobileThemeMode.SYSTEM -> androidx.compose.foundation.isSystemInDarkTheme()
                MobileThemeMode.DARK -> true
                MobileThemeMode.LIGHT -> false
            }
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(MaterialTheme.colorScheme.surface),
            ) {
                DjScreen(
                    apiKey = apiKey,
                    isDark = isDjDark,
                    lang = uiPreferences.uiLanguage,
                    onBack = { showDj = false },
                )
            }
        }

        // Preset editor — opens directly (no inspector intermediary)
        val activePreset = activePresetId?.let { id -> presetCatalog.findPreset(id) }
        if (activePreset != null) {
            val presetLang = uiPreferences.uiLanguage
            androidx.activity.compose.BackHandler { activePresetId = null }
            androidx.compose.animation.AnimatedVisibility(
                visible = true,
                enter = fadeIn(tween(200)) + androidx.compose.animation.scaleIn(
                    initialScale = 0.9f,
                    animationSpec = tween(
                        300,
                        easing = androidx.compose.animation.core.FastOutSlowInEasing,
                    ),
                ),
            ) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(MaterialTheme.colorScheme.surface),
                ) {
                    dev.screengoated.toolbox.mobile.preset.ui.PresetEditorScreen(
                        preset = activePreset.preset,
                        lang = presetLang,
                        onBack = { activePresetId = null },
                        onPresetChanged = { updated ->
                            presetRepository.updateBuiltInOverride(activePreset.preset.id) {
                                updated
                            }
                        },
                        onRestoreDefault = {
                            presetRepository.restoreBuiltInPreset(activePreset.preset.id)
                        },
                        providerSettings = presetRuntimeSettings.providerSettings,
                    )
                }
            }
        }

        if (showBilingualRelay) {
            androidx.activity.compose.BackHandler { showBilingualRelay = false }
        }
        androidx.compose.animation.AnimatedVisibility(
            visible = showBilingualRelay,
            enter = fadeIn(tween(200)) + androidx.compose.animation.scaleIn(
                initialScale = 0.8f,
                animationSpec = tween(
                    350,
                    easing = androidx.compose.animation.core.FastOutSlowInEasing,
                ),
            ),
            exit = fadeOut(tween(150)) + androidx.compose.animation.scaleOut(
                targetScale = 0.8f,
                animationSpec = tween(
                    250,
                    easing = androidx.compose.animation.core.FastOutSlowInEasing,
                ),
            ),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(MaterialTheme.colorScheme.surface),
            ) {
                BilingualRelayScreen(
                    locale = locale,
                    onBack = { showBilingualRelay = false },
                    onNavigateToTtsSettings = {
                        showBilingualRelay = false
                        showTtsSettings = true
                    },
                )
            }
        }
    }
}

