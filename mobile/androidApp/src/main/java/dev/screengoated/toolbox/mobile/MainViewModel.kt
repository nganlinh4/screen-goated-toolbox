package dev.screengoated.toolbox.mobile

import android.content.Context
import android.content.Intent
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import dev.screengoated.toolbox.mobile.model.AndroidLiveSessionRepository
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.service.LiveTranslateService
import dev.screengoated.toolbox.mobile.shared.live.DisplayMode
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionPatch
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import kotlinx.coroutines.flow.StateFlow

class MainViewModel(
    private val repository: AndroidLiveSessionRepository,
) : ViewModel() {
    val sessionState: StateFlow<dev.screengoated.toolbox.mobile.shared.live.LiveSessionState> =
        repository.state
    val apiKey: StateFlow<String> = repository.apiKey
    val cerebrasApiKey: StateFlow<String> = repository.cerebrasApiKey
    val realtimeTtsSettings: StateFlow<RealtimeTtsSettings> = repository.realtimeTtsSettings
    val globalTtsSettings: StateFlow<MobileGlobalTtsSettings> = repository.globalTtsSettings

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
            repository.currentGlobalTtsSettings().copy(method = method),
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

    companion object {
        fun factory(application: SgtMobileApplication): ViewModelProvider.Factory {
            val repository = application.appContainer.repository
            return object : ViewModelProvider.Factory {
                @Suppress("UNCHECKED_CAST")
                override fun <T : ViewModel> create(modelClass: Class<T>): T {
                    return MainViewModel(repository) as T
                }
            }
        }
    }
}
