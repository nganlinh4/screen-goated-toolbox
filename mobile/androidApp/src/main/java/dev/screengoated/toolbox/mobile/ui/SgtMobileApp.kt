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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.Alignment
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp

import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import dev.screengoated.toolbox.mobile.BuildConfig
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.translationgummy.TranslationGummyScreen
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.service.tts.EdgeVoiceCatalogState
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import android.widget.Toast
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors
import dev.screengoated.toolbox.mobile.updater.AppUpdateUiState

@Composable
internal fun SgtMobileApp(
    state: LiveSessionState,
    providerKeys: ProviderKeysState,
    globalTtsSettings: MobileGlobalTtsSettings,
    presetRuntimeSettings: PresetRuntimeSettings,
    customModels: List<CustomPresetModelDefinition>,
    uiPreferences: MobileUiPreferences,
    locale: MobileLocaleText,
    historyBundle: HistoryUiBundle,
    appUpdateState: AppUpdateUiState,
    shellSectionRequest: ShellSectionRequest? = null,
    edgeVoiceCatalogState: EdgeVoiceCatalogState,
    onPresetRuntimeSettingsChanged: (PresetRuntimeSettings) -> Unit,
    onCustomModelsChanged: (List<CustomPresetModelDefinition>) -> Unit,
    onUiLanguageSelected: (String) -> Unit,
    onThemeCycleRequested: () -> Unit,
    onGlobalTtsMethodChanged: (MobileTtsMethod) -> Unit,
    onGlobalTtsModelChanged: (String) -> Unit,
    onGlobalTtsSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
    onGlobalTtsVoiceChanged: (String) -> Unit,
    onGlobalTtsConditionsChanged: (List<MobileTtsLanguageCondition>) -> Unit,
    onGlobalEdgeTtsSettingsChanged: (MobileEdgeTtsSettings) -> Unit,
    onGlobalTtsSettingsChanged: (dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings) -> Unit,
    onVoiceSettingsShown: () -> Unit,
    onRetryEdgeVoiceCatalog: () -> Unit,
    onPreviewGeminiVoice: (String) -> Unit,
    onPreviewEdgeVoice: (String, String) -> Unit,
    onPreviewGoogleTranslate: () -> Unit,
    onSessionToggle: () -> Unit,
    onResetHistoryDefaults: () -> Unit,
    onCheckForAppUpdates: () -> Unit,
    onOverlayOpacityChanged: (Int) -> Unit = {},
) {
    val appContext = LocalContext.current
    val appContainer = remember(appContext) {
        (appContext.applicationContext as SgtMobileApplication).appContainer
    }
    val translationGummyState by appContainer.translationGummyRepository.state.collectAsState()
    var showTtsSettings by rememberSaveable { mutableStateOf(false) }
    var ttsGeminiOnly by rememberSaveable { mutableStateOf(false) }
    var showPresetRuntimeSettings by rememberSaveable { mutableStateOf(false) }
    var showCustomModels by rememberSaveable { mutableStateOf(false) }
    var showUsageStats by rememberSaveable { mutableStateOf(false) }
    var showDownloadedTools by rememberSaveable { mutableStateOf(false) }
    var showDownloader by rememberSaveable { mutableStateOf(false) }
    var showFeatureUnsupported by rememberSaveable { mutableStateOf(false) }
    var showDj by rememberSaveable { mutableStateOf(false) }
    var showTranslationGummy by rememberSaveable { mutableStateOf(false) }
    var activePresetId by rememberSaveable { mutableStateOf<String?>(null) }
    val presetRepository = appContainer.presetRepository
    val presetCatalog by presetRepository.catalogState.collectAsState()

    if (showTtsSettings) {
        val ttsSnapshotAtOpen = remember { globalTtsSettings }
        GlobalTtsSettingsDialog(
            settings = globalTtsSettings,
            locale = locale,
            edgeVoiceCatalogState = edgeVoiceCatalogState,
            onDismiss = {
                showTtsSettings = false
                // Restart translation gummy websocket if voice or model changed
                // (matches Windows translation_gummy::update_settings)
                if (showTranslationGummy &&
                    (globalTtsSettings.geminiModel != ttsSnapshotAtOpen.geminiModel ||
                        globalTtsSettings.voice != ttsSnapshotAtOpen.voice)
                ) {
                    dev.screengoated.toolbox.mobile.translationgummy.TranslationGummyService.start(
                        appContext,
                        restart = true,
                    )
                }
                ttsGeminiOnly = false
            },
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
            onSettingsChanged = onGlobalTtsSettingsChanged,
            geminiOnly = ttsGeminiOnly,
            translationGummyVolume = if (ttsGeminiOnly && showTranslationGummy) {
                translationGummyState.volume
            } else {
                null
            },
            onTranslationGummyVolumeChanged = appContainer.translationGummyRepository::updateVolumePercent,
            onTranslationGummyMuteToggle = appContainer.translationGummyRepository::toggleMuted,
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

    if (showCustomModels) {
        CustomModelsDialog(
            models = customModels,
            openRouterApiKey = providerKeys.openRouterApiKey,
            ollamaBaseUrl = providerKeys.ollamaUrl,
            locale = locale,
            uiLanguage = uiPreferences.uiLanguage,
            onDismiss = { showCustomModels = false },
            onSave = onCustomModelsChanged,
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

    if (showDownloadedTools) {
        DownloadedToolsDialog(
            locale = locale,
            onDismiss = { showDownloadedTools = false },
        )
    }

    val focusManager = LocalFocusManager.current
    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE

    Box(
        modifier = Modifier
            .fillMaxSize()
            .testTag("sgt-app-root")
            .semantics { testTagsAsResourceId = true }
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
                    providerKeys = providerKeys,
                    globalTtsSettings = globalTtsSettings,
                    presetRuntimeSettings = presetRuntimeSettings,
                    locale = locale,
                    historyBundle = historyBundle,
                    appUpdateState = appUpdateState,
                    shellSectionRequest = shellSectionRequest,
                    settingsActions = SettingsActions(
                        onPresetRuntimeSettingsClick = { showPresetRuntimeSettings = true },
                        onCustomModelsClick = { showCustomModels = true },
                        onUsageStatsClick = { showUsageStats = true },
                        onDownloadedToolsClick = { showDownloadedTools = true },
                        onResetDefaults = {
                            // Match Windows reset scope: reset everything except API keys and language
                            presetRepository.resetAllToDefaults()
                            onPresetRuntimeSettingsChanged(PresetRuntimeSettings())
                            onOverlayOpacityChanged(85)
                            onResetHistoryDefaults()
                            // Reset TTS to defaults
                            onGlobalTtsMethodChanged(dev.screengoated.toolbox.mobile.model.MobileTtsMethod.GEMINI_LIVE)
                            onGlobalTtsSpeedPresetChanged(dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset.FAST)
                            onGlobalTtsVoiceChanged(dev.screengoated.toolbox.mobile.model.TtsDefaults.DEFAULT_TTS_GEMINI_VOICE)
                            onGlobalEdgeTtsSettingsChanged(dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings())
                        },
                        onVoiceSettingsClick = {
                            onVoiceSettingsShown()
                            showTtsSettings = true
                        },
                        onOverlayOpacityChanged = onOverlayOpacityChanged,
                        onCheckForAppUpdates = onCheckForAppUpdates,
                    ),
                    navActions = ShellNavActions(
                        onSessionToggle = onSessionToggle,
                        onDownloaderClick = {
                            if (BuildConfig.DOWNLOADER_SUPPORTED) {
                                showDownloader = true
                            } else {
                                showFeatureUnsupported = true
                            }
                        },
                        onDjClick = { showDj = true },
                        onTranslationGummyClick = { showTranslationGummy = true },
                        onPresetClick = { presetId -> activePresetId = presetId },
                    ),
                    uiPreferences = uiPreferences,
                    showEmbeddedHeader = isLandscape,
                    appHeaderTitle = locale.appHeaderTitle,
                    uiLanguage = uiPreferences.uiLanguage,
                    languageOptions = locale.languageOptions,
                    onUiLanguageSelected = onUiLanguageSelected,
                    themeMode = uiPreferences.themeMode,
                    onThemeCycleRequested = onThemeCycleRequested,
                )
            }
        }

        if (showFeatureUnsupported) {
            AlertDialog(
                onDismissRequest = { showFeatureUnsupported = false },
                title = { Text(locale.appFeatureUnsupportedTitle) },
                text = { Text(locale.appFeatureUnsupportedMessage) },
                confirmButton = {
                    TextButton(onClick = { showFeatureUnsupported = false }) {
                        Text(locale.closeLabel)
                    }
                },
            )
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
                    .testTag("downloader-screen")
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
                    .testTag("dj-screen")
                    .background(MaterialTheme.colorScheme.surface),
            ) {
                DjScreen(
                    apiKey = providerKeys.apiKey,
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

        if (showTranslationGummy) {
            androidx.activity.compose.BackHandler { showTranslationGummy = false }
        }
        androidx.compose.animation.AnimatedVisibility(
            visible = showTranslationGummy,
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
                    .testTag("translation-gummy-screen")
                    .background(MaterialTheme.colorScheme.surface),
            ) {
                TranslationGummyScreen(
                    locale = locale,
                    onBack = { showTranslationGummy = false },
                    onNavigateToTtsSettings = {
                        ttsGeminiOnly = true
                        showTtsSettings = true
                        Toast.makeText(appContext, locale.ttsSettingsTitle, Toast.LENGTH_SHORT).show()
                    },
                )
            }
        }
    }
}
