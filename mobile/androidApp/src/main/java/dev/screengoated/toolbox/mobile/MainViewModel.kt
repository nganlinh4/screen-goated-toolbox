package dev.screengoated.toolbox.mobile

import android.content.Context
import android.content.Intent
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import dev.screengoated.toolbox.mobile.history.HistoryRepository
import dev.screengoated.toolbox.mobile.updater.AppUpdateNotice
import dev.screengoated.toolbox.mobile.history.HistoryUiState
import dev.screengoated.toolbox.mobile.model.AndroidLiveSessionRepository
import dev.screengoated.toolbox.mobile.model.CustomModelStatus
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.model.next
import dev.screengoated.toolbox.mobile.model.normalizedResultOverlayOpacityPercent
import dev.screengoated.toolbox.mobile.model.restoredDefaults
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.model.withMethod
import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.service.LiveTranslateService
import dev.screengoated.toolbox.mobile.service.tts.EdgeVoiceCatalogState
import dev.screengoated.toolbox.mobile.service.tts.TtsConsumer
import dev.screengoated.toolbox.mobile.service.tts.TtsPriority
import dev.screengoated.toolbox.mobile.service.tts.TtsRequest
import dev.screengoated.toolbox.mobile.service.tts.TtsRequestMode
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.service.tts.toRuntimeSnapshot
import dev.screengoated.toolbox.mobile.service.tts.VoicePreviewSamples
import dev.screengoated.toolbox.mobile.service.tts.edgeVoicePreviewRequest
import dev.screengoated.toolbox.mobile.shared.live.DisplayMode
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionPatch
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.updater.AppUpdateController
import dev.screengoated.toolbox.mobile.updater.AppUpdateUiState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

class MainViewModel(
    private val repository: AndroidLiveSessionRepository,
    private val historyRepository: HistoryRepository,
    private val ttsRuntimeService: TtsRuntimeService,
    private val appUpdateController: AppUpdateController,
    private val voicePreviewSamples: VoicePreviewSamples,
) : ViewModel() {
    private var lastPreviewIndex = -1
    private val mutableHistorySearchQuery = MutableStateFlow("")

    val sessionState: StateFlow<dev.screengoated.toolbox.mobile.shared.live.LiveSessionState> =
        repository.state
    val apiKey: StateFlow<String> = repository.apiKey
    val groqApiKey: StateFlow<String> = repository.groqApiKey
    val openRouterApiKey: StateFlow<String> = repository.openRouterApiKey
    val nvidiaApiKey: StateFlow<String> = repository.nvidiaApiKey
    val ollamaUrl: StateFlow<String> = repository.ollamaUrl
    val realtimeTtsSettings: StateFlow<RealtimeTtsSettings> = repository.realtimeTtsSettings
    val globalTtsSettings: StateFlow<MobileGlobalTtsSettings> = repository.globalTtsSettings
    val uiPreferences: StateFlow<MobileUiPreferences> = repository.uiPreferences
    val presetRuntimeSettings: StateFlow<PresetRuntimeSettings> = repository.presetRuntimeSettings
    val customModels: StateFlow<List<CustomPresetModelDefinition>> = repository.customModels
    val customModelsStatus: StateFlow<CustomModelStatus> = repository.customModelsStatus
    val customModelEditor = repository.customModelEditor
    val edgeVoiceCatalogState: StateFlow<EdgeVoiceCatalogState> = ttsRuntimeService.edgeVoiceCatalogState
    val historyState: StateFlow<HistoryUiState> = historyRepository.state
    val historySearchQuery: StateFlow<String> = mutableHistorySearchQuery.asStateFlow()
    val appUpdateState: StateFlow<AppUpdateUiState> = appUpdateController.state

    init {
        repository.updateConfig(
            LiveSessionPatch(
                displayMode = if (BuildConfig.OVERLAY_SUPPORTED) {
                    DisplayMode.OVERLAY
                } else {
                    DisplayMode.IN_APP_MIRROR
                },
            ),
        )
        repository.refreshPermissions()
        appUpdateController.autoCheckForUpdates()
    }

    fun runtimePermissions(): Array<String> = repository.runtimePermissions()

    fun refreshPermissions() {
        repository.refreshPermissions()
        repository.ensureSourceStillValid()
    }

    fun onApiKeyChanged(apiKey: String) {
        repository.updateApiKey(apiKey)
    }

    fun onGroqApiKeyChanged(apiKey: String) {
        repository.updateGroqApiKey(apiKey)
    }

    fun onOpenRouterApiKeyChanged(apiKey: String) {
        repository.updateOpenRouterApiKey(apiKey)
    }

    fun onNvidiaApiKeyChanged(apiKey: String) {
        repository.updateNvidiaApiKey(apiKey)
    }

    fun onOllamaUrlChanged(url: String) {
        repository.updateOllamaUrl(url)
    }

    fun onCustomModelsChanged(expected: List<CustomPresetModelDefinition>, models: List<CustomPresetModelDefinition>) =
        repository.submitCustomModels(expected, models)

    fun onCustomModelEdited(identity: dev.screengoated.toolbox.mobile.model.CustomModelRowIdentity,
        transform: (CustomPresetModelDefinition) -> CustomPresetModelDefinition) = repository.editCustomModel(identity, transform)

    fun onCustomModelDeleted(identity: dev.screengoated.toolbox.mobile.model.CustomModelRowIdentity) =
        repository.deleteCustomModel(identity)

    fun retryCustomModels(): Boolean = repository.retryCustomModels()

    fun onUiLanguageSelected(languageCode: String) {
        repository.updateUiPreferences(
            repository.currentUiPreferences().copy(uiLanguage = languageCode),
        )
    }

    fun onThemeCycleRequested() {
        repository.updateUiPreferences(
            repository.currentUiPreferences().copy(
                themeMode = repository.currentUiPreferences().themeMode.next(),
            ),
        )
    }

    fun onOverlayOpacityChanged(opacityPercent: Int) {
        repository.updateUiPreferences(
            repository.currentUiPreferences().copy(
                overlayOpacityPercent = opacityPercent.normalizedResultOverlayOpacityPercent(),
            ),
        )
    }

    /**
     * Settings half of Reset defaults, as Windows restore_defaults.rs resets app
     * and audio settings: theme, overlay opacity, preset runtime and every global
     * TTS field (method, Gemini model, voice, speed, language conditions, Edge).
     */
    fun restoreAppSettingsDefaults() {
        repository.updateUiPreferences(repository.currentUiPreferences().restoredDefaults())
        repository.updatePresetRuntimeSettings(PresetRuntimeSettings())
        repository.updateGlobalTtsSettings(MobileGlobalTtsSettings())
        repository.restoreLiveTranslateDefaults()
    }

    fun onPresetRuntimeSettingsChanged(settings: PresetRuntimeSettings) {
        repository.updatePresetRuntimeSettings(settings)
    }

    fun onGlobalTtsMethodChanged(method: MobileTtsMethod) {
        repository.updateGlobalTtsSettings(
            repository.currentGlobalTtsSettings().withMethod(method),
        )
    }

    fun onGlobalTtsModelChanged(model: String) {
        repository.updateGlobalTtsSettings(
            repository.currentGlobalTtsSettings().copy(geminiModel = model),
        )
    }

    fun onGlobalTtsSpeedPresetChanged(speedPreset: MobileTtsSpeedPreset) {
        repository.updateGlobalTtsSettings(
            repository.currentGlobalTtsSettings().copy(speedPreset = speedPreset),
        )
    }

    fun onGlobalTtsVoiceChanged(voice: String) {
        repository.updateGlobalTtsSettings(
            repository.currentGlobalTtsSettings().copy(voice = voice),
        )
    }

    fun onGlobalTtsConditionsChanged(conditions: List<MobileTtsLanguageCondition>) {
        repository.updateGlobalTtsSettings(
            repository.currentGlobalTtsSettings().copy(languageConditions = conditions),
        )
    }

    fun onGlobalEdgeTtsSettingsChanged(edgeSettings: MobileEdgeTtsSettings) {
        repository.updateGlobalTtsSettings(
            repository.currentGlobalTtsSettings().copy(edgeSettings = edgeSettings),
        )
    }

    fun onGlobalTtsSettingsChanged(settings: dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings) {
        repository.updateGlobalTtsSettings(settings)
    }

    fun onVoiceSettingsShown() {
        ttsRuntimeService.ensureEdgeVoiceCatalog()
    }

    fun retryEdgeVoiceCatalog() {
        ttsRuntimeService.ensureEdgeVoiceCatalog(force = true)
    }

    fun previewGeminiVoice(voiceName: String) {
        val snapshot = repository.currentGlobalTtsSettings().copy(voice = voiceName)
        val preview = nextPreviewText(voiceName)
        ttsRuntimeService.enqueueSettingsPreview(
            TtsRequest(
                text = preview,
                consumer = TtsConsumer.SETTINGS_PREVIEW,
                priority = TtsPriority.PREVIEW,
                requestMode = TtsRequestMode.INTERRUPT,
                settingsSnapshot = snapshot.toRuntimeSnapshot(),
                ownerToken = "settings-preview",
            ),
        )
    }

    fun previewEdgeVoice(
        languageCode: String,
        voiceName: String,
    ) {
        ttsRuntimeService.enqueueSettingsPreview(
            edgeVoicePreviewRequest(repository.currentGlobalTtsSettings(), languageCode, voiceName, voicePreviewSamples),
        )
    }

    fun previewGoogleTranslate() {
        val snapshot = repository.currentGlobalTtsSettings().withMethod(MobileTtsMethod.GOOGLE_TRANSLATE)
        val preview = nextPreviewText("Google Translate")
        ttsRuntimeService.enqueueSettingsPreview(
            TtsRequest(
                text = preview,
                consumer = TtsConsumer.SETTINGS_PREVIEW,
                priority = TtsPriority.PREVIEW,
                requestMode = TtsRequestMode.INTERRUPT,
                settingsSnapshot = snapshot.toRuntimeSnapshot(),
                ownerToken = "settings-preview",
            ),
        )
    }

    fun rememberProjectionConsent(resultCode: Int, data: Intent?) {
        repository.rememberProjectionConsent(resultCode, data)
    }

    fun startSession(context: Context) {
        repository.refreshPermissions()
        LiveTranslateService.start(context)
    }

    fun stopSession(context: Context) {
        LiveTranslateService.stop(context)
        repository.syncStoppedState()
    }

    fun onHistorySearchQueryChanged(query: String) {
        mutableHistorySearchQuery.value = query
    }

    fun clearHistorySearchQuery() {
        mutableHistorySearchQuery.value = ""
    }

    fun onHistoryMaxItemsChanged(value: Int) {
        historyRepository.updateMaxItems(value)
    }

    fun resetHistoryDefaults() {
        historyRepository.resetSettingsToDefaults()
    }

    fun deleteHistoryItem(id: Long) {
        historyRepository.delete(id)
    }

    fun historyMediaProofForOpen(item: dev.screengoated.toolbox.mobile.history.HistoryItem): dev.screengoated.toolbox.mobile.history.HistoryMediaProof? =
        historyRepository.mediaProofForOpen(item)

    fun clearHistoryItems() {
        historyRepository.clearAll()
    }

    fun checkForAppUpdates() {
        appUpdateController.checkForUpdates()
    }

    fun consumeUpdateNotification(notice: AppUpdateNotice?): String? = appUpdateController.consumeNotification(notice)

    fun hasApiKey(): Boolean = repository.currentApiKey().isNotBlank()

    fun fail(message: String) {
        repository.fail(message)
    }

    private fun nextPreviewText(voiceName: String): String {
        val locale = MobileLocaleText.forLanguage(repository.currentUiPreferences().uiLanguage)
        val selection = locale.nextPreviewText(
            voiceName = voiceName,
            previousIndex = lastPreviewIndex,
        )
        lastPreviewIndex = selection.index
        return selection.text
    }

    companion object {
        fun factory(application: SgtMobileApplication): ViewModelProvider.Factory {
            val repository = application.appContainer.repository
            val historyRepository = application.appContainer.historyRepository
            val ttsRuntimeService = application.appContainer.ttsRuntimeService
            val appUpdateController = application.appContainer.appUpdateController
            return object : ViewModelProvider.Factory {
                @Suppress("UNCHECKED_CAST")
                override fun <T : ViewModel> create(modelClass: Class<T>): T {
                    return MainViewModel(
                        repository,
                        historyRepository,
                        ttsRuntimeService,
                        appUpdateController,
                        VoicePreviewSamples.parse(application.assets.open("voice-preview.json").bufferedReader().use { it.readText() }),
                    ) as T
                }
            }
        }
    }
}
