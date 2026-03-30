package dev.screengoated.toolbox.mobile.shared.live

import kotlinx.serialization.Serializable

@Serializable
enum class SourceMode {
    MIC,
    DEVICE,
}

@Serializable
enum class DisplayMode {
    OVERLAY,
    IN_APP_MIRROR,
}

@Serializable
enum class AuthMode {
    BYOK,
    PAIRED_DESKTOP,
    EPHEMERAL,
}

@Serializable
enum class EngineKind {
    CLOUD,
    PAIRED_DESKTOP,
    ON_DEVICE,
}

@Serializable
enum class SessionPhase {
    IDLE,
    AWAITING_PERMISSIONS,
    STARTING,
    LISTENING,
    TRANSLATING,
    ERROR,
    STOPPED,
}

@Serializable
enum class TranscriptionMethod {
    GEMINI_LIVE,
    PARAKEET,
}

@Serializable
data class ProviderDescriptor(
    val id: String,
    val model: String,
)

@Serializable
data class LiveSessionConfig(
    val sourceMode: SourceMode = SourceMode.DEVICE,
    val displayMode: DisplayMode = DisplayMode.OVERLAY,
    val targetLanguage: String = "Vietnamese",
    val transcriptionProvider: ProviderDescriptor = GeneratedLiveModelCatalog.defaultTranscriptionProvider(),
    val translationProvider: ProviderDescriptor = GeneratedLiveModelCatalog.translationProviderDescriptor(),
    val authMode: AuthMode = AuthMode.BYOK,
    val engineKind: EngineKind = EngineKind.CLOUD,
    val keepOverlayOnTop: Boolean = true,
    val notificationPersistent: Boolean = true,
)

@Serializable
data class LiveSessionPatch(
    val sourceMode: SourceMode? = null,
    val displayMode: DisplayMode? = null,
    val targetLanguage: String? = null,
    val transcriptionProvider: ProviderDescriptor? = null,
    val translationProvider: ProviderDescriptor? = null,
    val authMode: AuthMode? = null,
    val engineKind: EngineKind? = null,
    val keepOverlayOnTop: Boolean? = null,
    val notificationPersistent: Boolean? = null,
)

@Serializable
data class PermissionSnapshot(
    val recordAudioGranted: Boolean = false,
    val notificationsGranted: Boolean = false,
    val overlayGranted: Boolean = false,
    val mediaProjectionGranted: Boolean = false,
) {
    fun readyFor(config: LiveSessionConfig, overlaySupported: Boolean): Boolean {
        val notificationsReady = notificationsGranted
        val overlayReady = when {
            config.displayMode != DisplayMode.OVERLAY -> true
            !overlaySupported -> true
            else -> overlayGranted
        }
        val playbackReady = when (config.sourceMode) {
            SourceMode.MIC -> true
            SourceMode.DEVICE -> mediaProjectionGranted
        }
        return recordAudioGranted && notificationsReady && overlayReady && playbackReady
    }
}

@Serializable
data class LiveSessionMetrics(
    val transcriptLatencyMs: Long? = null,
    val translationLatencyMs: Long? = null,
    val lastUpdatedEpochMs: Long = 0L,
)

@Serializable
data class TranslationHistoryEntry(
    val source: String,
    val translation: String,
)

@Serializable
data class LiveTextState(
    val fullTranscript: String = "",
    val displayTranscript: String = "",
    val lastCommittedPos: Int = 0,
    val lastProcessedLen: Int = 0,
    val committedTranslation: String = "",
    val uncommittedTranslation: String = "",
    val uncommittedSourceStart: Int = 0,
    val uncommittedSourceEnd: Int = 0,
    val displayTranslation: String = "",
    val translationHistory: List<TranslationHistoryEntry> = emptyList(),
    val lastTranscriptAppendAtMs: Long = 0L,
    val lastTranslationUpdateAtMs: Long = 0L,
    val transcriptionMethod: TranscriptionMethod = TranscriptionMethod.GEMINI_LIVE,
) {
    val transcript: String
        get() = displayTranscript

    val translation: String
        get() = displayTranslation
}

@Serializable
data class LiveSessionState(
    val phase: SessionPhase = SessionPhase.IDLE,
    val config: LiveSessionConfig = LiveSessionConfig(),
    val permissions: PermissionSnapshot = PermissionSnapshot(),
    val liveText: LiveTextState = LiveTextState(),
    val lastError: String? = null,
    val errorSerial: Int = 0,
    val overlayVisible: Boolean = false,
    val metrics: LiveSessionMetrics = LiveSessionMetrics(),
) {
    val transcript: String
        get() = liveText.transcript

    val translation: String
        get() = liveText.translation
}

@Serializable
data class TranslationRequest(
    val sourceStart: Int,
    val sourceEnd: Int,
    val finalizedSourceEnd: Int,
    val pendingSource: String,
    val finalizedSource: String,
    val draftSource: String,
    val previousDraftTranslation: String = "",
    val history: List<TranslationHistoryEntry> = emptyList(),
) {
    val hasFinishedDelimiter: Boolean
        get() = finalizedSourceEnd > sourceStart

    val bytesToCommit: Int
        get() = (finalizedSourceEnd - sourceStart).coerceAtLeast(0)

    val draftSourceStart: Int
        get() = finalizedSourceEnd

    fun requiresDraftTranslation(): Boolean {
        val trimmed = draftSource.trim()
        return trimmed.isNotEmpty() && trimmed.any { it.isLetterOrDigit() }
    }

    fun fallbackDraftTranslation(): String {
        return if (requiresDraftTranslation()) "" else draftSource.trim()
    }
}

@Serializable
data class TranslationPatch(
    val sourceStart: Int,
    val sourceEnd: Int,
    val state: String,
    val translation: String,
)

@Serializable
data class TranslationResponse(
    val patches: List<TranslationPatch>,
)

@Serializable
sealed interface LiveSessionEvent {
    @Serializable
    data class StateChanged(val state: LiveSessionState) : LiveSessionEvent

    @Serializable
    data class TranscriptDelta(val text: String) : LiveSessionEvent

    @Serializable
    data class TranslationDelta(val text: String) : LiveSessionEvent

    @Serializable
    data class MetricsUpdated(val metrics: LiveSessionMetrics) : LiveSessionEvent

    @Serializable
    data class PermissionRevoked(val permissions: PermissionSnapshot) : LiveSessionEvent

    @Serializable
    data class FatalError(val message: String) : LiveSessionEvent
}
