package dev.screengoated.toolbox.mobile.model

import android.content.Context
import android.content.Intent
import dev.screengoated.toolbox.mobile.history.HistoryRepository
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
import dev.screengoated.toolbox.mobile.shared.live.LiveTextState
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
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
    private val historyRepository: HistoryRepository,
) {
    private var persistedConfig = normalizeConfig(settingsStore.loadConfig())
    private var transientSessionConfigActive = false
    private val mutableApiKey = MutableStateFlow(settingsStore.loadApiKey())
    private val mutableCerebrasApiKey = MutableStateFlow(settingsStore.loadCerebrasApiKey())
    private val mutableGroqApiKey = MutableStateFlow(settingsStore.loadGroqApiKey())
    private val mutableOpenRouterApiKey = MutableStateFlow(settingsStore.loadOpenRouterApiKey())
    private val mutableOllamaUrl = MutableStateFlow(settingsStore.loadOllamaUrl())
    private val mutablePaneFontSizes = MutableStateFlow(settingsStore.loadPaneFontSizes())
    private val mutableRealtimeTtsSettings = MutableStateFlow(settingsStore.loadRealtimeTtsSettings())
    private val mutableGlobalTtsSettings = MutableStateFlow(
        normalizeGlobalTtsSettings(settingsStore.loadGlobalTtsSettings()),
    )
    private val mutableUiPreferences = MutableStateFlow(settingsStore.loadUiPreferences())
    private val mutablePresetRuntimeSettings = MutableStateFlow(settingsStore.loadPresetRuntimeSettings())

    val apiKey: StateFlow<String> = mutableApiKey.asStateFlow()
    val cerebrasApiKey: StateFlow<String> = mutableCerebrasApiKey.asStateFlow()
    val groqApiKey: StateFlow<String> = mutableGroqApiKey.asStateFlow()
    val openRouterApiKey: StateFlow<String> = mutableOpenRouterApiKey.asStateFlow()
    val ollamaUrl: StateFlow<String> = mutableOllamaUrl.asStateFlow()
    val state: StateFlow<LiveSessionState> = store.state
    val projectionStore: ProjectionConsentStore = projectionConsentStore
    val paneFontSizes: StateFlow<RealtimePaneFontSizes> = mutablePaneFontSizes.asStateFlow()
    val realtimeTtsSettings: StateFlow<RealtimeTtsSettings> = mutableRealtimeTtsSettings.asStateFlow()
    val globalTtsSettings: StateFlow<MobileGlobalTtsSettings> = mutableGlobalTtsSettings.asStateFlow()
    val uiPreferences: StateFlow<MobileUiPreferences> = mutableUiPreferences.asStateFlow()
    val presetRuntimeSettings: StateFlow<PresetRuntimeSettings> = mutablePresetRuntimeSettings.asStateFlow()

    val supportedLanguages: List<String> = LanguageCatalog.names

    init {
        val permissions = permissionEvaluator.evaluate(context, persistedConfig, overlaySupported)
        store.hydrate(persistedConfig, permissions)
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
        if (!transientSessionConfigActive) {
            persistedConfig = state.value.config
            settingsStore.saveConfig(persistedConfig)
        }
        refreshPermissions()
    }

    fun applyTransientSessionConfig(config: LiveSessionConfig) {
        transientSessionConfigActive = true
        store.hydrate(config, permissionEvaluator.evaluate(context, config, overlaySupported))
    }

    fun clearTransientSessionConfig() {
        if (!transientSessionConfigActive) {
            return
        }
        transientSessionConfigActive = false
        val permissions = permissionEvaluator.evaluate(context, persistedConfig, overlaySupported)
        store.hydrate(persistedConfig, permissions)
    }

    fun isTransientSessionConfigActive(): Boolean = transientSessionConfigActive

    fun updateApiKey(apiKey: String) {
        mutableApiKey.value = apiKey
        settingsStore.saveApiKey(apiKey.trim())
    }

    fun updateCerebrasApiKey(apiKey: String) {
        mutableCerebrasApiKey.value = apiKey
        settingsStore.saveCerebrasApiKey(apiKey.trim())
    }

    fun updateGroqApiKey(apiKey: String) {
        mutableGroqApiKey.value = apiKey
        settingsStore.saveGroqApiKey(apiKey.trim())
    }

    fun updateOpenRouterApiKey(apiKey: String) {
        mutableOpenRouterApiKey.value = apiKey
        settingsStore.saveOpenRouterApiKey(apiKey.trim())
    }

    fun updateOllamaUrl(url: String) {
        mutableOllamaUrl.value = url
        settingsStore.saveOllamaUrl(url.trim())
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

    fun updatePresetRuntimeSettings(settings: PresetRuntimeSettings) {
        mutablePresetRuntimeSettings.value = settings
        settingsStore.savePresetRuntimeSettings(settings)
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
        val previousLiveText = state.value.liveText
        store.finalizeTranslation(bytesToCommit)
        persistCommittedSegment(previousLiveText, state.value.liveText)
    }

    fun forceCommitIfDue(nowMs: Long): Boolean {
        val previousLiveText = state.value.liveText
        val committed = store.forceCommitIfDue(nowMs)
        if (committed) {
            persistCommittedSegment(previousLiveText, state.value.liveText)
        }
        return committed
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

    fun commitPendingLiveHistory() {
        val previousLiveText = state.value.liveText
        store.forceCommitAll()
        persistCommittedSegment(previousLiveText, state.value.liveText)
    }

    fun currentApiKey(): String = apiKey.value.trim()

    fun currentCerebrasApiKey(): String = cerebrasApiKey.value.trim()

    fun currentGroqApiKey(): String = groqApiKey.value.trim()

    fun currentOpenRouterApiKey(): String = openRouterApiKey.value.trim()

    fun currentOllamaUrl(): String = ollamaUrl.value.trim()

    fun currentConfig(): LiveSessionConfig = state.value.config

    fun currentPaneFontSizes(): RealtimePaneFontSizes = paneFontSizes.value

    fun currentRealtimeTtsSettings(): RealtimeTtsSettings = realtimeTtsSettings.value

    fun currentGlobalTtsSettings(): MobileGlobalTtsSettings = globalTtsSettings.value

    fun currentUiPreferences(): MobileUiPreferences = uiPreferences.value

    fun currentPresetRuntimeSettings(): PresetRuntimeSettings = presetRuntimeSettings.value

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
        return RealtimeModelIds.translationProviderDescriptor(modelId)
    }

    private fun transcriptionProviderFor(modelId: String): ProviderDescriptor {
        return RealtimeModelIds.defaultTranscriptionProvider(
            RealtimeModelIds.normalizeTranscriptionModelId(modelId),
        )
    }

    private fun persistCommittedSegment(
        previous: LiveTextState,
        current: LiveTextState,
    ) {
        if (current.lastCommittedPos <= previous.lastCommittedPos) {
            return
        }
        val sourceSegment = current.fullTranscript
            .substring(previous.lastCommittedPos, current.lastCommittedPos)
            .trim()
        val translationSegment = committedTranslationDelta(
            previous = previous.committedTranslation,
            current = current.committedTranslation,
        )
        if (sourceSegment.isBlank() || translationSegment.isBlank()) {
            return
        }
        historyRepository.saveText(
            resultText = translationSegment,
            inputText = sourceSegment,
        )
    }

    private fun committedTranslationDelta(
        previous: String,
        current: String,
    ): String {
        if (previous.isBlank()) {
            return current.trim()
        }
        val prefixed = "$previous "
        return when {
            current == previous -> ""
            current.startsWith(prefixed) -> current.removePrefix(prefixed).trim()
            else -> current.removePrefix(previous).trim()
        }
    }

    private fun normalizeConfig(config: LiveSessionConfig): LiveSessionConfig {
        val normalizedId =
            RealtimeModelIds.normalizeTranscriptionModelId(config.transcriptionProvider.id)
        return config.copy(
            transcriptionProvider = RealtimeModelIds.defaultTranscriptionProvider(normalizedId),
        )
    }

    private fun normalizeGlobalTtsSettings(settings: MobileGlobalTtsSettings): MobileGlobalTtsSettings {
        return settings.copy(
            geminiModel = RealtimeModelIds.normalizeTtsGeminiModel(settings.geminiModel),
        )
    }
}
