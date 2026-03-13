package dev.screengoated.toolbox.mobile.model

import android.content.Context
import android.content.Intent
import dev.screengoated.toolbox.mobile.shared.live.DisplayMode
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionMetrics
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionPatch
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionStore
import dev.screengoated.toolbox.mobile.shared.live.ProviderDescriptor
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.shared.live.TranscriptionMethod
import dev.screengoated.toolbox.mobile.shared.live.TranslationRequest
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore
import dev.screengoated.toolbox.mobile.storage.SecureSettingsStore
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

class AndroidLiveSessionRepository(
    private val context: Context,
    private val store: LiveSessionStore,
    private val settingsStore: SecureSettingsStore,
    private val permissionEvaluator: PermissionSnapshotEvaluator,
    private val projectionConsentStore: ProjectionConsentStore,
    private val overlaySupported: Boolean,
) {
    private val mutableApiKey = MutableStateFlow(settingsStore.loadApiKey())
    private val mutableCerebrasApiKey = MutableStateFlow(settingsStore.loadCerebrasApiKey())
    private val mutablePaneFontSizes = MutableStateFlow(settingsStore.loadPaneFontSizes())
    private val mutableRealtimeTtsSettings = MutableStateFlow(settingsStore.loadRealtimeTtsSettings())
    private val mutableGlobalTtsSettings = MutableStateFlow(settingsStore.loadGlobalTtsSettings())
    private val mutableUiPreferences = MutableStateFlow(settingsStore.loadUiPreferences())

    val apiKey: StateFlow<String> = mutableApiKey.asStateFlow()
    val cerebrasApiKey: StateFlow<String> = mutableCerebrasApiKey.asStateFlow()
    val state: StateFlow<LiveSessionState> = store.state
    val projectionStore: ProjectionConsentStore = projectionConsentStore
    val paneFontSizes: StateFlow<RealtimePaneFontSizes> = mutablePaneFontSizes.asStateFlow()
    val realtimeTtsSettings: StateFlow<RealtimeTtsSettings> = mutableRealtimeTtsSettings.asStateFlow()
    val globalTtsSettings: StateFlow<MobileGlobalTtsSettings> = mutableGlobalTtsSettings.asStateFlow()
    val uiPreferences: StateFlow<MobileUiPreferences> = mutableUiPreferences.asStateFlow()

    val supportedLanguages: List<String> = LanguageCatalog.names

    init {
        val config = settingsStore.loadConfig()
        val permissions = permissionEvaluator.evaluate(context, config, overlaySupported)
        store.hydrate(config, permissions)
    }

    fun refreshPermissions() {
        store.updatePermissions(permissionEvaluator.evaluate(context, state.value.config, overlaySupported))
    }

    fun updateConfig(patch: LiveSessionPatch) {
        val previousLanguage = state.value.config.targetLanguage
        store.updateConfig(patch)
        if (patch.targetLanguage != null && patch.targetLanguage != previousLanguage) {
            store.clearTranslationHistory()
        }
        settingsStore.saveConfig(state.value.config)
        refreshPermissions()
    }

    fun updateApiKey(apiKey: String) {
        mutableApiKey.value = apiKey
        settingsStore.saveApiKey(apiKey.trim())
    }

    fun updateCerebrasApiKey(apiKey: String) {
        mutableCerebrasApiKey.value = apiKey
        settingsStore.saveCerebrasApiKey(apiKey.trim())
    }

    fun updatePaneFontSizes(fontSizes: RealtimePaneFontSizes) {
        mutablePaneFontSizes.value = fontSizes
        settingsStore.savePaneFontSizes(fontSizes)
    }

    fun updateRealtimeTtsSettings(settings: RealtimeTtsSettings) {
        mutableRealtimeTtsSettings.value = settings.copy(
            speedPercent = settings.speedPercent.coerceIn(50, 200),
            volumePercent = settings.volumePercent.coerceIn(0, 100),
        )
        settingsStore.saveRealtimeTtsSettings(mutableRealtimeTtsSettings.value)
    }

    fun updateGlobalTtsSettings(settings: MobileGlobalTtsSettings) {
        mutableGlobalTtsSettings.value = settings
        settingsStore.saveGlobalTtsSettings(settings)
    }

    fun updateUiPreferences(preferences: MobileUiPreferences) {
        val normalizedLanguage = when (preferences.uiLanguage) {
            "vi", "ko" -> preferences.uiLanguage
            else -> "en"
        }
        mutableUiPreferences.value = preferences.copy(uiLanguage = normalizedLanguage)
        settingsStore.saveUiPreferences(mutableUiPreferences.value)
    }

    fun runtimePermissions(): Array<String> = permissionEvaluator.runtimePermissions()

    fun rememberProjectionConsent(resultCode: Int, data: Intent?) {
        projectionConsentStore.update(resultCode, data)
        refreshPermissions()
    }

    fun clearProjectionConsent() {
        projectionConsentStore.clear()
        refreshPermissions()
    }

    fun markAwaitingPermissions() {
        store.markAwaitingPermissions(state.value.permissions)
    }

    fun markStarting() {
        store.markStarting()
    }

    fun markListening() {
        store.markListening()
    }

    fun markTranslating() {
        store.markTranslating()
    }

    fun setTranscriptionMethod(method: TranscriptionMethod) {
        store.setTranscriptionMethod(method)
    }

    fun appendTranscript(
        text: String,
        nowMs: Long,
    ) {
        store.appendTranscript(text, nowMs)
    }

    fun claimTranslationRequest(): TranslationRequest? {
        return store.claimTranslationRequest()
    }

    fun appendTranslationDelta(
        text: String,
        nowMs: Long,
    ) {
        store.appendTranslationDelta(text, nowMs)
    }

    fun finalizeTranslation(bytesToCommit: Int) {
        store.finalizeTranslation(bytesToCommit)
    }

    fun forceCommitIfDue(nowMs: Long): Boolean {
        return store.forceCommitIfDue(nowMs)
    }

    fun updateMetrics(metrics: LiveSessionMetrics) {
        store.updateMetrics(metrics)
    }

    fun setOverlayVisible(visible: Boolean) {
        store.setOverlayVisible(visible)
    }

    fun fail(message: String) {
        store.fail(message)
    }

    fun stop() {
        store.stop()
    }

    fun currentApiKey(): String = apiKey.value.trim()

    fun currentCerebrasApiKey(): String = cerebrasApiKey.value.trim()

    fun currentConfig(): LiveSessionConfig = state.value.config

    fun currentPaneFontSizes(): RealtimePaneFontSizes = paneFontSizes.value

    fun currentRealtimeTtsSettings(): RealtimeTtsSettings = realtimeTtsSettings.value

    fun currentGlobalTtsSettings(): MobileGlobalTtsSettings = globalTtsSettings.value

    fun currentUiPreferences(): MobileUiPreferences = uiPreferences.value

    fun translationModelId(): String = state.value.config.translationProvider.id

    fun transcriptionModelId(): String = state.value.config.transcriptionProvider.id

    fun updateTranslationModel(modelId: String) {
        updateConfig(
            LiveSessionPatch(
                translationProvider = translationProviderFor(modelId),
            ),
        )
    }

    fun updateTranscriptionModel(modelId: String) {
        updateConfig(
            LiveSessionPatch(
                transcriptionProvider = transcriptionProviderFor(modelId),
            ),
        )
    }

    fun canStartSession(): Boolean = state.value.permissions.readyFor(state.value.config, overlaySupported)

    fun syncStoppedState() {
        if (state.value.phase !in listOf(SessionPhase.IDLE, SessionPhase.STOPPED)) {
            store.stop()
        }
    }

    fun ensureSafePlayDefaults() {
        if (!overlaySupported && state.value.config.displayMode == DisplayMode.OVERLAY) {
            updateConfig(LiveSessionPatch(displayMode = DisplayMode.IN_APP_MIRROR))
        }
    }

    fun ensureSourceStillValid() {
        if (state.value.config.sourceMode == SourceMode.DEVICE && !state.value.permissions.mediaProjectionGranted) {
            store.markAwaitingPermissions(state.value.permissions)
        }
    }

    private fun translationProviderFor(modelId: String): ProviderDescriptor {
        return when (modelId) {
            RealtimeModelIds.TRANSLATION_CEREBRAS -> ProviderDescriptor(
                id = RealtimeModelIds.TRANSLATION_CEREBRAS,
                model = "gpt-oss-120b",
            )

            RealtimeModelIds.TRANSLATION_GTX -> ProviderDescriptor(
                id = RealtimeModelIds.TRANSLATION_GTX,
                model = "google-translate-gtx",
            )

            else -> ProviderDescriptor(
                id = RealtimeModelIds.TRANSLATION_GEMMA,
                model = "gemma-3-27b-it",
            )
        }
    }

    private fun transcriptionProviderFor(modelId: String): ProviderDescriptor {
        return when (modelId) {
            RealtimeModelIds.TRANSCRIPTION_PARAKEET -> ProviderDescriptor(
                id = RealtimeModelIds.TRANSCRIPTION_PARAKEET,
                model = "realtime_eou_120m-v1-onnx",
            )

            else -> ProviderDescriptor(
                id = RealtimeModelIds.TRANSCRIPTION_GEMINI,
                model = "gemini-2.5-flash-native-audio-preview-12-2025",
            )
        }
    }
}
