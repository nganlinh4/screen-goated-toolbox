package dev.screengoated.toolbox.mobile

import android.content.Context
import android.content.Intent
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import dev.screengoated.toolbox.mobile.model.AndroidLiveSessionRepository
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.model.next
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.model.withMethod
import dev.screengoated.toolbox.mobile.service.LiveTranslateService
import dev.screengoated.toolbox.mobile.service.tts.EdgeVoiceCatalogState
import dev.screengoated.toolbox.mobile.service.tts.TtsConsumer
import dev.screengoated.toolbox.mobile.service.tts.TtsPriority
import dev.screengoated.toolbox.mobile.service.tts.TtsRequest
import dev.screengoated.toolbox.mobile.service.tts.TtsRequestMode
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.service.tts.toRuntimeSnapshot
import dev.screengoated.toolbox.mobile.shared.live.DisplayMode
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionPatch
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.flow.StateFlow

class MainViewModel(
    private val repository: AndroidLiveSessionRepository,
    private val ttsRuntimeService: TtsRuntimeService,
) : ViewModel() {
    private var lastPreviewIndex = -1

    val sessionState: StateFlow<dev.screengoated.toolbox.mobile.shared.live.LiveSessionState> =
        repository.state
    val apiKey: StateFlow<String> = repository.apiKey
    val cerebrasApiKey: StateFlow<String> = repository.cerebrasApiKey
    val groqApiKey: StateFlow<String> = repository.groqApiKey
    val openRouterApiKey: StateFlow<String> = repository.openRouterApiKey
    val ollamaUrl: StateFlow<String> = repository.ollamaUrl
    val realtimeTtsSettings: StateFlow<RealtimeTtsSettings> = repository.realtimeTtsSettings
    val globalTtsSettings: StateFlow<MobileGlobalTtsSettings> = repository.globalTtsSettings
    val uiPreferences: StateFlow<MobileUiPreferences> = repository.uiPreferences
    val edgeVoiceCatalogState: StateFlow<EdgeVoiceCatalogState> = ttsRuntimeService.edgeVoiceCatalogState

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
        repository.ensureSafePlayDefaults()
        repository.refreshPermissions()
    }

    fun runtimePermissions(): Array<String> = repository.runtimePermissions()

    fun refreshPermissions() {
        repository.refreshPermissions()
        repository.ensureSourceStillValid()
    }

    fun onSourceModeSelected(sourceMode: SourceMode) {
        repository.updateConfig(LiveSessionPatch(sourceMode = sourceMode))
    }

    fun onDisplayModeSelected(displayMode: DisplayMode) {
        repository.updateConfig(LiveSessionPatch(displayMode = displayMode))
    }

    fun onTargetLanguageSelected(targetLanguage: String) {
        repository.updateConfig(LiveSessionPatch(targetLanguage = targetLanguage))
    }

    fun onApiKeyChanged(apiKey: String) {
        repository.updateApiKey(apiKey)
    }

    fun onCerebrasApiKeyChanged(apiKey: String) {
        repository.updateCerebrasApiKey(apiKey)
    }

    fun onGroqApiKeyChanged(apiKey: String) {
        repository.updateGroqApiKey(apiKey)
    }

    fun onOpenRouterApiKeyChanged(apiKey: String) {
        repository.updateOpenRouterApiKey(apiKey)
    }

    fun onOllamaUrlChanged(url: String) {
        repository.updateOllamaUrl(url)
    }

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

    fun onThemeSelected(themeMode: MobileThemeMode) {
        repository.updateUiPreferences(
            repository.currentUiPreferences().copy(themeMode = themeMode),
        )
    }

    fun onRealtimeTtsEnabledChanged(enabled: Boolean) {
        repository.updateRealtimeTtsSettings(
            repository.currentRealtimeTtsSettings().copy(enabled = enabled),
        )
    }

    fun onRealtimeTtsAutoSpeedChanged(enabled: Boolean) {
        repository.updateRealtimeTtsSettings(
            repository.currentRealtimeTtsSettings().copy(autoSpeed = enabled),
        )
    }

    fun onRealtimeTtsSpeedChanged(speedPercent: Int) {
        repository.updateRealtimeTtsSettings(
            repository.currentRealtimeTtsSettings().copy(speedPercent = speedPercent),
        )
    }

    fun onRealtimeTtsVolumeChanged(volumePercent: Int) {
        repository.updateRealtimeTtsSettings(
            repository.currentRealtimeTtsSettings().copy(volumePercent = volumePercent),
        )
    }

    fun onGlobalTtsMethodChanged(method: MobileTtsMethod) {
        repository.updateGlobalTtsSettings(
            repository.currentGlobalTtsSettings().withMethod(method),
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

    fun onVoiceSettingsShown() {
        ttsRuntimeService.ensureEdgeVoiceCatalog()
    }

    fun retryEdgeVoiceCatalog() {
        ttsRuntimeService.ensureEdgeVoiceCatalog(force = true)
    }

    fun previewGeminiVoice(voiceName: String) {
        val snapshot = repository.currentGlobalTtsSettings().copy(voice = voiceName)
        val preview = nextPreviewText(voiceName)
        ttsRuntimeService.interruptAndSpeak(
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
        val settings = repository.currentGlobalTtsSettings()
        val nextConfigs = settings.edgeSettings.voiceConfigs.toMutableList()
        val existingIndex = nextConfigs.indexOfFirst { it.languageCode.equals(languageCode, ignoreCase = true) }
        if (existingIndex >= 0) {
            nextConfigs[existingIndex] = nextConfigs[existingIndex].copy(voiceName = voiceName)
        }
        val snapshot = settings.copy(
            edgeSettings = settings.edgeSettings.copy(voiceConfigs = nextConfigs),
        )
        val preview = nextPreviewText(voiceName)
        ttsRuntimeService.interruptAndSpeak(
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
            val ttsRuntimeService = application.appContainer.ttsRuntimeService
            return object : ViewModelProvider.Factory {
                @Suppress("UNCHECKED_CAST")
                override fun <T : ViewModel> create(modelClass: Class<T>): T {
                    return MainViewModel(repository, ttsRuntimeService) as T
                }
            }
        }
    }
}
