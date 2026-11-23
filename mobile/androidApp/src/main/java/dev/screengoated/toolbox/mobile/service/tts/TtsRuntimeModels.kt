package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import kotlinx.serialization.Serializable

enum class TtsConsumer {
    REALTIME,
    SETTINGS_PREVIEW,
    RESULT_OVERLAY,
    AUTO_SPEAK,
}

enum class TtsPriority(
    val level: Int,
) {
    REALTIME(0),
    USER(10),
    PREVIEW(20),
}

enum class TtsRequestMode {
    NORMAL,
    REALTIME,
    INTERRUPT,
}

enum class TtsCompletionStatus {
    COMPLETED,
    INTERRUPTED,
    FAILED,
    PARTIAL,
}

data class TtsRequestSettingsSnapshot(
    val method: MobileTtsMethod,
    val geminiModel: String,
    val geminiVoice: String,
    val speedPreset: MobileTtsSpeedPreset,
    val languageConditions: List<MobileTtsLanguageCondition>,
    val edgeSettings: MobileEdgeTtsSettings,
    val targetLanguage: String? = null,
    val realtimeSpeedPercent: Int = 100,
    val realtimeAutoSpeed: Boolean = true,
    val realtimeVolumePercent: Int = 100,
)

data class TtsRequest(
    val text: String,
    val consumer: TtsConsumer,
    val priority: TtsPriority,
    val requestMode: TtsRequestMode,
    val settingsSnapshot: TtsRequestSettingsSnapshot,
    val ownerToken: String,
    val isCurrent: (() -> Boolean)? = null,
    val runIfCurrent: (((() -> Unit) -> Boolean))? = null,
    val resumeAfterInterruption: TtsInterruption? = null,
    val edgeVoiceOverride: String? = null,
)

internal fun TtsRequest.canDispatch(): Boolean = isCurrent?.invoke() != false
internal fun TtsRequest.dispatchIfCurrent(action: () -> Unit): Boolean =
    runIfCurrent?.invoke(action) ?: if (canDispatch()) { action(); true } else false

data class TtsRuntimeState(
    val isPlaying: Boolean = false,
    val activeRequestId: Long? = null,
    val activeConsumer: TtsConsumer? = null,
    val pendingWorkCount: Int = 0,
    val pendingPlaybackCount: Int = 0,
    val currentRealtimeEffectiveSpeed: Int = 100,
)

data class TtsPlaybackEvent(
    val requestId: Long,
    val consumer: TtsConsumer,
    val ownerToken: String,
    val completionStatus: TtsCompletionStatus,
    val interruptedBy: TtsInterruption? = null,
)

@Serializable
data class EdgeVoice(
    val shortName: String,
    val gender: String,
    val locale: String,
    val friendlyName: String,
)

@Serializable
data class CachedEdgeVoiceCatalog(
    val voices: List<EdgeVoice> = emptyList(),
)

data class EdgeVoiceCatalogState(
    val voices: List<EdgeVoice> = emptyList(),
    val byLanguage: Map<String, List<EdgeVoice>> = emptyMap(),
    val loaded: Boolean = false,
    val loading: Boolean = false,
    val errorMessage: String? = null,
)

fun MobileGlobalTtsSettings.toRuntimeSnapshot(
    targetLanguage: String? = null,
    realtimeSettings: RealtimeTtsSettings? = null,
): TtsRequestSettingsSnapshot {
    return TtsRequestSettingsSnapshot(
        method = method,
        geminiModel = geminiModel,
        geminiVoice = voice,
        speedPreset = speedPreset,
        languageConditions = languageConditions,
        edgeSettings = edgeSettings,
        targetLanguage = targetLanguage,
        realtimeSpeedPercent = realtimeSettings?.speedPercent ?: 100,
        realtimeAutoSpeed = realtimeSettings?.autoSpeed ?: true,
        realtimeVolumePercent = realtimeSettings?.volumePercent ?: 100,
    )
}
