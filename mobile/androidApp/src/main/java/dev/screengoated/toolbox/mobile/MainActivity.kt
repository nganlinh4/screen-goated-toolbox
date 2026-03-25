package dev.screengoated.toolbox.mobile

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.runtime.rememberUpdatedState
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.preset.AudioPresetLaunchKind
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.shared.live.DisplayMode
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.ProviderDescriptor
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.service.BubbleService
import dev.screengoated.toolbox.mobile.ui.SgtMobileApp
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.theme.SgtMobileTheme

class MainActivity : ComponentActivity() {
    private val viewModel: MainViewModel by viewModels {
        MainViewModel.factory(application as SgtMobileApplication)
    }

    private var pendingStart = false
    private var autoStartOnResume = false
    private var resumePendingAudioPreset = false

    private val permissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        viewModel.refreshPermissions()
        if (pendingStart) {
            if (viewModel.sessionState.value.permissions.recordAudioGranted &&
                viewModel.sessionState.value.permissions.notificationsGranted
            ) {
                continueStartFlow()
            } else {
                pendingStart = false
                viewModel.fail("Live translate needs the requested runtime permissions.")
            }
        }
    }

    private val overlayLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) {
        viewModel.refreshPermissions()
        if (pendingStart) {
            if (viewModel.sessionState.value.permissions.overlayGranted) {
                continueStartFlow()
            } else {
                pendingStart = false
                viewModel.fail("Overlay permission is required for the floating live window.")
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        handleIntent(intent)
        enableEdgeToEdge()

        setContent {
            val state by viewModel.sessionState.collectAsStateWithLifecycle()
            val apiKey by viewModel.apiKey.collectAsStateWithLifecycle()
            val cerebrasApiKey by viewModel.cerebrasApiKey.collectAsStateWithLifecycle()
            val groqApiKey by viewModel.groqApiKey.collectAsStateWithLifecycle()
            val openRouterApiKey by viewModel.openRouterApiKey.collectAsStateWithLifecycle()
            val ollamaUrl by viewModel.ollamaUrl.collectAsStateWithLifecycle()
            val globalTtsSettings by viewModel.globalTtsSettings.collectAsStateWithLifecycle()
            val uiPreferences by viewModel.uiPreferences.collectAsStateWithLifecycle()
            val presetRuntimeSettings by viewModel.presetRuntimeSettings.collectAsStateWithLifecycle()
            val edgeVoiceCatalogState by viewModel.edgeVoiceCatalogState.collectAsStateWithLifecycle()
            val historyState by viewModel.historyState.collectAsStateWithLifecycle()
            val historySearchQuery by viewModel.historySearchQuery.collectAsStateWithLifecycle()
            val context = LocalContext.current
            val locale = MobileLocaleText.forLanguage(uiPreferences.uiLanguage)
            val onSessionToggle by rememberUpdatedState {
                if (state.phase == dev.screengoated.toolbox.mobile.shared.live.SessionPhase.LISTENING ||
                    state.phase == dev.screengoated.toolbox.mobile.shared.live.SessionPhase.TRANSLATING ||
                    state.phase == dev.screengoated.toolbox.mobile.shared.live.SessionPhase.STARTING
                ) {
                    pendingStart = false
                    viewModel.stopSession(this@MainActivity)
                } else {
                    pendingStart = true
                    continueStartFlow()
                }
            }

            LaunchedEffect(state.errorSerial) {
                state.lastError?.takeIf { it.isNotBlank() }?.let { message ->
                    Toast.makeText(context, message, Toast.LENGTH_SHORT).show()
                }
            }

            SgtMobileTheme(themeMode = uiPreferences.themeMode) {
                SgtMobileApp(
                    state = state,
                    apiKey = apiKey,
                    cerebrasApiKey = cerebrasApiKey,
                    groqApiKey = groqApiKey,
                    openRouterApiKey = openRouterApiKey,
                    ollamaUrl = ollamaUrl,
                    globalTtsSettings = globalTtsSettings,
                    presetRuntimeSettings = presetRuntimeSettings,
                    uiPreferences = uiPreferences,
                    locale = locale,
                    historyState = historyState,
                    historySearchQuery = historySearchQuery,
                    onApiKeyChanged = viewModel::onApiKeyChanged,
                    onCerebrasApiKeyChanged = viewModel::onCerebrasApiKeyChanged,
                    onGroqApiKeyChanged = viewModel::onGroqApiKeyChanged,
                    onOpenRouterApiKeyChanged = viewModel::onOpenRouterApiKeyChanged,
                    onOllamaUrlChanged = viewModel::onOllamaUrlChanged,
                    onPresetRuntimeSettingsChanged = viewModel::onPresetRuntimeSettingsChanged,
                    onUiLanguageSelected = viewModel::onUiLanguageSelected,
                    onThemeCycleRequested = viewModel::onThemeCycleRequested,
                    edgeVoiceCatalogState = edgeVoiceCatalogState,
                    onGlobalTtsMethodChanged = viewModel::onGlobalTtsMethodChanged,
                    onGlobalTtsSpeedPresetChanged = viewModel::onGlobalTtsSpeedPresetChanged,
                    onGlobalTtsVoiceChanged = viewModel::onGlobalTtsVoiceChanged,
                    onGlobalTtsConditionsChanged = viewModel::onGlobalTtsConditionsChanged,
                    onGlobalEdgeTtsSettingsChanged = viewModel::onGlobalEdgeTtsSettingsChanged,
                    onVoiceSettingsShown = viewModel::onVoiceSettingsShown,
                    onRetryEdgeVoiceCatalog = viewModel::retryEdgeVoiceCatalog,
                    onPreviewGeminiVoice = viewModel::previewGeminiVoice,
                    onPreviewEdgeVoice = viewModel::previewEdgeVoice,
                    onPreviewGoogleTranslate = viewModel::previewGoogleTranslate,
                    onSessionToggle = onSessionToggle,
                    onOverlayOpacityChanged = viewModel::onOverlayOpacityChanged,
                    onHistorySearchQueryChanged = viewModel::onHistorySearchQueryChanged,
                    onClearHistorySearchQuery = viewModel::clearHistorySearchQuery,
                    onHistoryMaxItemsChanged = viewModel::onHistoryMaxItemsChanged,
                    onResetHistoryDefaults = viewModel::resetHistoryDefaults,
                    onDeleteHistoryItem = viewModel::deleteHistoryItem,
                    onClearHistoryItems = viewModel::clearHistoryItems,
                )
            }
        }
    }

    override fun onResume() {
        super.onResume()
        viewModel.refreshPermissions()
        maybeRunDeferredStartFlow(source = "onResume")
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleIntent(intent)
        window.decorView.post {
            maybeRunDeferredStartFlow(source = "onNewIntent")
        }
    }

    private fun continueStartFlow() {
        val appContainer = (application as SgtMobileApplication).appContainer
        val pendingAudioPreset = appContainer.audioPresetLaunchStore.peek()
        val pendingResolvedPreset = pendingAudioPreset?.let { appContainer.presetRepository.getResolvedPreset(it.presetId) }
        viewModel.refreshPermissions()
        val state = viewModel.sessionState.value
        val requiresGeminiApiKey = pendingAudioPreset?.kind == AudioPresetLaunchKind.REALTIME || pendingAudioPreset == null
        if (requiresGeminiApiKey && !viewModel.hasApiKey()) {
            pendingStart = false
            viewModel.fail("Enter your Gemini BYOK key before starting live translate.")
            return
        }

        val missingRuntimePermissions = buildList {
            if (!state.permissions.recordAudioGranted) {
                add(android.Manifest.permission.RECORD_AUDIO)
            }
            if (pendingAudioPreset?.kind != AudioPresetLaunchKind.CAPTURE &&
                android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU &&
                !state.permissions.notificationsGranted
            ) {
                add(android.Manifest.permission.POST_NOTIFICATIONS)
            }
        }
        val effectiveSourceMode = when {
            pendingResolvedPreset?.preset?.audioSource == "device" -> SourceMode.DEVICE
            pendingResolvedPreset != null -> SourceMode.MIC
            else -> state.config.sourceMode
        }
        val hasProjectionConsent = when {
            pendingAudioPreset?.kind == AudioPresetLaunchKind.CAPTURE && effectiveSourceMode == SourceMode.DEVICE ->
                appContainer.projectionConsentStore.hasConsent()
            else -> state.permissions.mediaProjectionGranted
        }

        when {
            missingRuntimePermissions.isNotEmpty() -> {
                permissionLauncher.launch(missingRuntimePermissions.toTypedArray())
            }

            pendingAudioPreset?.kind != AudioPresetLaunchKind.CAPTURE &&
                state.config.displayMode == DisplayMode.OVERLAY &&
                BuildConfig.OVERLAY_SUPPORTED &&
                !state.permissions.overlayGranted -> {
                overlayLauncher.launch(
                    Intent(
                        Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                        Uri.parse("package:$packageName"),
                    ),
                )
            }

            effectiveSourceMode == SourceMode.DEVICE &&
                !hasProjectionConsent -> {
                pendingStart = false
                val intent = when (pendingAudioPreset?.kind) {
                    AudioPresetLaunchKind.CAPTURE -> ProjectionConsentProxyActivity.resumeCapturePresetIntent(this)
                    AudioPresetLaunchKind.REALTIME -> ProjectionConsentProxyActivity.resumeRealtimePresetIntent(this)
                    else -> ProjectionConsentProxyActivity.startSessionIntent(this)
                }
                startActivity(intent)
            }

            else -> {
                pendingStart = false
                if (resumePendingAudioPreset && pendingAudioPreset != null) {
                    when (pendingAudioPreset.kind) {
                        AudioPresetLaunchKind.CAPTURE -> {
                            BubbleService.resumePendingAudioPreset(this)
                            resumePendingAudioPreset = false
                            return
                        }
                        AudioPresetLaunchKind.REALTIME -> {
                            val resolved = appContainer.presetRepository.getResolvedPreset(pendingAudioPreset.presetId)
                            if (resolved == null) {
                                appContainer.audioPresetLaunchStore.clear()
                                viewModel.fail("The requested realtime audio preset is unavailable.")
                                return
                            }
                            appContainer.repository.applyTransientSessionConfig(
                                resolved.preset.toRealtimeSessionConfig(
                                    fallback = appContainer.repository.currentConfig(),
                                ),
                            )
                            appContainer.audioPresetLaunchStore.setActiveRealtimePresetId(resolved.preset.id)
                            viewModel.startSession(this)
                            appContainer.audioPresetLaunchStore.clear()
                        }
                    }
                    resumePendingAudioPreset = false
                } else {
                    viewModel.startSession(this)
                }
            }
        }
    }

    private fun handleIntent(intent: Intent?) {
        if (intent?.getBooleanExtra(EXTRA_AUTO_START, false) == true) {
            autoStartOnResume = true
            intent.removeExtra(EXTRA_AUTO_START)
        }
        if (intent?.getBooleanExtra(EXTRA_RESUME_PENDING_AUDIO_PRESET, false) == true) {
            autoStartOnResume = true
            resumePendingAudioPreset = true
            Toast.makeText(
                this,
                "Reacquiring device-audio capture permission...",
                Toast.LENGTH_SHORT,
            ).show()
            intent.removeExtra(EXTRA_RESUME_PENDING_AUDIO_PRESET)
        }
    }

    private fun maybeRunDeferredStartFlow(source: String) {
        if (!autoStartOnResume) {
            return
        }
        autoStartOnResume = false
        pendingStart = true
        continueStartFlow()
    }

    companion object {
        const val EXTRA_AUTO_START = "dev.screengoated.toolbox.mobile.extra.AUTO_START"
        const val EXTRA_RESUME_PENDING_AUDIO_PRESET =
            "dev.screengoated.toolbox.mobile.extra.RESUME_PENDING_AUDIO_PRESET"
    }
}

internal fun dev.screengoated.toolbox.mobile.shared.preset.Preset.toRealtimeSessionConfig(
    fallback: LiveSessionConfig,
): LiveSessionConfig {
    val transcriptionBlock = blocks.firstOrNull { it.blockType == BlockType.AUDIO }
    val translationBlock = blocks.firstOrNull { it.blockType == BlockType.TEXT }
    val sourceMode = if (audioSource == "device") SourceMode.DEVICE else SourceMode.MIC
    val targetLanguage = transcriptionBlock?.languageVars?.get("language1")
        ?: translationBlock?.languageVars?.get("language1")
        ?: fallback.targetLanguage
    val transcriptionProvider = when (
        PresetModelCatalog.getById(transcriptionBlock?.model.orEmpty())?.provider
    ) {
        PresetModelProvider.PARAKEET -> ProviderDescriptor(
            id = RealtimeModelIds.TRANSCRIPTION_PARAKEET,
            model = "realtime_eou_120m-v1-onnx",
        )
        else -> ProviderDescriptor(
            id = RealtimeModelIds.TRANSCRIPTION_GEMINI,
            model = "gemini-2.5-flash-native-audio-preview-12-2025",
        )
    }
    val translationProvider = translationBlock?.let {
        when (it.model) {
            "google-gemma" -> ProviderDescriptor(
                id = RealtimeModelIds.TRANSLATION_GEMMA,
                model = "google-gemma",
            )
            "google-gtx" -> ProviderDescriptor(
                id = RealtimeModelIds.TRANSLATION_GTX,
                model = "google-gtx",
            )
            else -> ProviderDescriptor(
                id = RealtimeModelIds.TRANSLATION_CEREBRAS,
                model = "cerebras-oss",
            )
        }
    } ?: fallback.translationProvider

    return fallback.copy(
        sourceMode = sourceMode,
        targetLanguage = targetLanguage,
        transcriptionProvider = transcriptionProvider,
        translationProvider = translationProvider,
    )
}
