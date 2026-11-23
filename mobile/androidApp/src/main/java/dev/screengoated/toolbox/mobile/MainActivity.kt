package dev.screengoated.toolbox.mobile

import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.core.net.toUri
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.platform.LocalContext
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.flow.collect
import dev.screengoated.toolbox.mobile.helpassistant.HelpAssistantPendingLaunchStore
import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.shared.live.DisplayMode
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.ProviderDescriptor
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.service.helpassistant.HelpAssistantOverlayService
import dev.screengoated.toolbox.mobile.ui.HistoryUiBundle
import dev.screengoated.toolbox.mobile.ui.ProviderKeysState
import dev.screengoated.toolbox.mobile.ui.MobileShellSection
import dev.screengoated.toolbox.mobile.ui.ShellSectionRequest
import dev.screengoated.toolbox.mobile.ui.SgtMobileApp
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText
import dev.screengoated.toolbox.mobile.ui.theme.SgtMobileTheme

class MainActivity : ComponentActivity() {
    private val viewModel: MainViewModel by viewModels {
        MainViewModel.factory(application as SgtMobileApplication)
    }

    private var pendingStart = false
    private var shellSectionRequest by mutableStateOf<ShellSectionRequest?>(null)
    private var shellSectionRequestSerial = 0L

    private val permissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        if ((application as SgtMobileApplication).appContainerOrNull == null) {
            pendingStart = false
            return@registerForActivityResult
        }
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
        if ((application as SgtMobileApplication).appContainerOrNull == null) {
            pendingStart = false
            return@registerForActivityResult
        }
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
        installSplashScreen()
        super.onCreate(savedInstanceState)
        if ((application as SgtMobileApplication).appContainerOrNull == null) {
            setContent { SecureStoreRecoveryScreen(this) }
            return
        }
        handleIntent(intent)
        enableEdgeToEdge()

        setContent {
            val state by viewModel.sessionState.collectAsStateWithLifecycle()
            val apiKey by viewModel.apiKey.collectAsStateWithLifecycle()
            val groqApiKey by viewModel.groqApiKey.collectAsStateWithLifecycle()
            val openRouterApiKey by viewModel.openRouterApiKey.collectAsStateWithLifecycle()
            val nvidiaApiKey by viewModel.nvidiaApiKey.collectAsStateWithLifecycle()
            val ollamaUrl by viewModel.ollamaUrl.collectAsStateWithLifecycle()
            val globalTtsSettings by viewModel.globalTtsSettings.collectAsStateWithLifecycle()
            val uiPreferences by viewModel.uiPreferences.collectAsStateWithLifecycle()
            val presetRuntimeSettings by viewModel.presetRuntimeSettings.collectAsStateWithLifecycle()
            val customModelEditor by viewModel.customModelEditor.collectAsStateWithLifecycle()
            val edgeVoiceCatalogState by viewModel.edgeVoiceCatalogState.collectAsStateWithLifecycle()
            val historyState by viewModel.historyState.collectAsStateWithLifecycle()
            val historySearchQuery by viewModel.historySearchQuery.collectAsStateWithLifecycle()
            val appUpdateState by viewModel.appUpdateState.collectAsStateWithLifecycle()
            val context = LocalContext.current
            val toastBus = (application as SgtMobileApplication).appContainer.toastBus
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

            LaunchedEffect(toastBus) {
                toastBus.messages.collect { message ->
                    Toast.makeText(context, message, Toast.LENGTH_SHORT).show()
                }
            }

            LaunchedEffect(appUpdateState.notification) {
                val updateVersion=viewModel.consumeUpdateNotification(appUpdateState.notification)
                if (updateVersion!=null) {
                    Toast.makeText(
                        context,
                        "${locale.updateAvailableNotification} v$updateVersion",
                        Toast.LENGTH_SHORT,
                    ).show()
                }
            }

            SgtMobileTheme(themeMode = uiPreferences.themeMode) {
                SgtMobileApp(
                    state = state,
                    providerKeys = ProviderKeysState(
                        apiKey = apiKey,
                        groqApiKey = groqApiKey,
                        openRouterApiKey = openRouterApiKey,
                        nvidiaApiKey = nvidiaApiKey,
                        ollamaUrl = ollamaUrl,
                        onApiKeyChanged = viewModel::onApiKeyChanged,
                        onGroqApiKeyChanged = viewModel::onGroqApiKeyChanged,
                        onOpenRouterApiKeyChanged = viewModel::onOpenRouterApiKeyChanged,
                        onNvidiaApiKeyChanged = viewModel::onNvidiaApiKeyChanged,
                        onOllamaUrlChanged = viewModel::onOllamaUrlChanged,
                    ),
                    globalTtsSettings = globalTtsSettings,
                    presetRuntimeSettings = presetRuntimeSettings,
                    customModelEditor = customModelEditor,
                    uiPreferences = uiPreferences,
                    locale = locale,
                    historyBundle = HistoryUiBundle(
                        state = historyState,
                        searchQuery = historySearchQuery,
                        onSearchQueryChanged = viewModel::onHistorySearchQueryChanged,
                        onClearSearchQuery = viewModel::clearHistorySearchQuery,
                        onMaxItemsChanged = viewModel::onHistoryMaxItemsChanged,
                        onDeleteItem = viewModel::deleteHistoryItem,
                        mediaProofForOpen = viewModel::historyMediaProofForOpen,
                        onClearItems = viewModel::clearHistoryItems,
                    ),
                    appUpdateState = appUpdateState,
                    shellSectionRequest = shellSectionRequest,
                    onPresetRuntimeSettingsChanged = viewModel::onPresetRuntimeSettingsChanged,
                    onCustomModelsChanged = viewModel::onCustomModelsChanged,
                    onCustomModelEdited = viewModel::onCustomModelEdited,
                    onCustomModelDeleted = viewModel::onCustomModelDeleted,
                    onRetryCustomModels = viewModel::retryCustomModels,
                    onUiLanguageSelected = viewModel::onUiLanguageSelected,
                    onThemeCycleRequested = viewModel::onThemeCycleRequested,
                    edgeVoiceCatalogState = edgeVoiceCatalogState,
                    onGlobalTtsMethodChanged = viewModel::onGlobalTtsMethodChanged,
                    onGlobalTtsModelChanged = viewModel::onGlobalTtsModelChanged,
                    onGlobalTtsSpeedPresetChanged = viewModel::onGlobalTtsSpeedPresetChanged,
                    onGlobalTtsVoiceChanged = viewModel::onGlobalTtsVoiceChanged,
                    onGlobalTtsConditionsChanged = viewModel::onGlobalTtsConditionsChanged,
                    onGlobalEdgeTtsSettingsChanged = viewModel::onGlobalEdgeTtsSettingsChanged,
                    onGlobalTtsSettingsChanged = viewModel::onGlobalTtsSettingsChanged,
                    onVoiceSettingsShown = viewModel::onVoiceSettingsShown,
                    onRetryEdgeVoiceCatalog = viewModel::retryEdgeVoiceCatalog,
                    onPreviewGeminiVoice = viewModel::previewGeminiVoice,
                    onPreviewEdgeVoice = viewModel::previewEdgeVoice,
                    onPreviewGoogleTranslate = viewModel::previewGoogleTranslate,
                    onSessionToggle = onSessionToggle,
                    onOverlayOpacityChanged = viewModel::onOverlayOpacityChanged,
                    onResetHistoryDefaults = viewModel::resetHistoryDefaults,
                    onRestoreAppSettingsDefaults = viewModel::restoreAppSettingsDefaults,
                    onCheckForAppUpdates = viewModel::checkForAppUpdates,
                )
            }
        }
    }

    override fun onResume() {
        super.onResume()
        if ((application as SgtMobileApplication).appContainerOrNull == null) return
        viewModel.refreshPermissions()
        maybeRunPendingHelpAssistant()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if ((application as SgtMobileApplication).appContainerOrNull == null) return
        handleIntent(intent)
    }

    private fun continueStartFlow() {
        val appContainer = (application as SgtMobileApplication).appContainer
        viewModel.refreshPermissions()
        val state = viewModel.sessionState.value
        // Only Gemini-backed transcription needs a key: the on-device engines never call
        // out, and translation always falls back to the keyless GTX provider.
        val transcriptionModelId = state.config.transcriptionProvider.id
        val requiresGeminiApiKey = RealtimeModelIds.requiresGeminiApiKey(transcriptionModelId)
        if (requiresGeminiApiKey && !viewModel.hasApiKey()) {
            pendingStart = false
            apiKeyErrorToastText("NO_API_KEY:google", appContainer.repository.currentUiPreferences().uiLanguage)
                ?.let(appContainer.toastBus::show)
            viewModel.fail("Enter your Gemini BYOK key before starting live translate.")
            return
        }

        val missingRuntimePermissions = buildList {
            if (!state.permissions.recordAudioGranted) {
                add(android.Manifest.permission.RECORD_AUDIO)
            }
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU &&
                !state.permissions.notificationsGranted
            ) {
                add(android.Manifest.permission.POST_NOTIFICATIONS)
            }
        }
        when {
            missingRuntimePermissions.isNotEmpty() -> {
                permissionLauncher.launch(missingRuntimePermissions.toTypedArray())
            }

            state.config.displayMode == DisplayMode.OVERLAY &&
                BuildConfig.OVERLAY_SUPPORTED &&
                !state.permissions.overlayGranted -> {
                overlayLauncher.launch(
                    Intent(
                        Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                        "package:$packageName".toUri(),
                    ),
                )
            }

            state.config.sourceMode == SourceMode.DEVICE &&
                !state.permissions.mediaProjectionGranted -> {
                pendingStart = false
                startActivity(ProjectionConsentProxyActivity.startSessionIntent(this))
            }

            else -> {
                pendingStart = false
                viewModel.startSession(this)
            }
        }
    }

    private fun maybeRunPendingHelpAssistant() {
        if (!Settings.canDrawOverlays(this)) {
            return
        }
        val pending = HelpAssistantPendingLaunchStore.take() ?: return
        HelpAssistantOverlayService.start(
            context = this,
            question = pending.question,
            uiLanguage = pending.uiLanguage,
        )
    }

    private fun handleIntent(intent: Intent?) {
        intent?.getStringExtra(EXTRA_SHELL_SECTION)?.let { rawSection ->
            val section = runCatching { MobileShellSection.valueOf(rawSection) }.getOrNull()
            if (section != null) {
                shellSectionRequest = ShellSectionRequest(
                    section = section,
                    serial = ++shellSectionRequestSerial,
                )
            }
            intent.removeExtra(EXTRA_SHELL_SECTION)
        }
    }

    companion object {
        private const val EXTRA_SHELL_SECTION =
            "dev.screengoated.toolbox.mobile.extra.SHELL_SECTION"

        internal fun settingsIntent(context: android.content.Context): Intent = Intent(
            context,
            MainActivity::class.java,
        ).putExtra(
            EXTRA_SHELL_SECTION,
            MobileShellSection.SETTINGS.name,
        ).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
    }
}
