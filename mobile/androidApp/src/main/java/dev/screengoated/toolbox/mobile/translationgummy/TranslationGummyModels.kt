package dev.screengoated.toolbox.mobile.translationgummy

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonNames

@Serializable
data class TranslationGummyLanguageProfile(
    val language: String = "",
    val accent: String = "",
    val tone: String = "",
) {
    fun normalized(): TranslationGummyLanguageProfile {
        return copy(
            language = language.trim(),
            accent = accent.trim(),
            tone = tone.trim(),
        )
    }
}

@Serializable
data class TranslationGummyConfig(
    val first: TranslationGummyLanguageProfile = TranslationGummyLanguageProfile(language = "English"),
    val second: TranslationGummyLanguageProfile = TranslationGummyLanguageProfile(language = "Korean", accent = "Busan", tone = "polite"),
    @OptIn(ExperimentalSerializationApi::class)
    @SerialName("guide_seen")
    @JsonNames("guideSeen")
    val guideSeen: Boolean = false,
) {
    fun normalized(): TranslationGummyConfig {
        return copy(first = first.normalized(), second = second.normalized())
    }

    fun isValid(): Boolean {
        val normalized = normalized()
        return normalized.first.language.isNotBlank() && normalized.second.language.isNotBlank()
    }

    fun buildSystemInstruction(): String {
        val normalized = normalized()
        fun describe(profile: TranslationGummyLanguageProfile): String {
            val parts = mutableListOf(profile.language)
            if (profile.accent.isNotBlank()) {
                parts += "${profile.accent} accent"
            }
            if (profile.tone.isNotBlank()) {
                parts += "(${profile.tone} tone)"
            }
            return parts.joinToString(" ")
        }

        return "You are a translation relay between ${describe(normalized.first)} and ${describe(normalized.second)}. Translate each spoken sentence unmistakably into the other language. Output ONLY the translation, nothing else. Never answer, comment, or add extra words."
    }
}

enum class TranslationGummyConnectionState {
    NOT_CONFIGURED,
    CONNECTING,
    READY,
    RECONNECTING,
    ERROR,
    STOPPED,
}

@Serializable
enum class TranslationGummyTranscriptRole {
    INPUT,
    OUTPUT,
    SEPARATOR,
}

@Serializable
data class TranslationGummyTranscriptItem(
    val id: Long,
    val role: TranslationGummyTranscriptRole,
    val text: String,
    val isFinal: Boolean,
    val updatedAtMs: Long,
    val lang: String = "",
)

data class TranslationGummyState(
    val appliedConfig: TranslationGummyConfig = TranslationGummyConfig(),
    val draftConfig: TranslationGummyConfig = TranslationGummyConfig(),
    val guideSeen: Boolean = false,
    val dirty: Boolean = false,
    val connectionState: TranslationGummyConnectionState = TranslationGummyConnectionState.NOT_CONFIGURED,
    val isRunning: Boolean = false,
    val transcripts: List<TranslationGummyTranscriptItem> = emptyList(),
    val lastError: String? = null,
    val visualizerLevel: Float = 0f,
    val volume: TranslationGummyVolumeState = TranslationGummyVolumeState(),
)

internal fun TranslationGummyConnectionState.isReady(): Boolean =
    this == TranslationGummyConnectionState.READY

data class TranslationGummyVolumeState(
    val percent: Int = 100,
    val restorePercent: Int = 100,
) {
    val muted: Boolean
        get() = percent == 0

    fun withPercent(nextPercent: Int): TranslationGummyVolumeState {
        val clamped = nextPercent.coerceIn(MIN_VOLUME_PERCENT, MAX_VOLUME_PERCENT)
        return copy(
            percent = clamped,
            restorePercent = if (clamped > 0) clamped else restorePercent.coerceRestoreVolume(),
        )
    }

    fun toggleMuted(): TranslationGummyVolumeState {
        return if (muted) {
            withPercent(restorePercent.coerceRestoreVolume())
        } else {
            copy(percent = MIN_VOLUME_PERCENT, restorePercent = percent.coerceRestoreVolume())
        }
    }

    private fun Int.coerceRestoreVolume(): Int {
        return takeIf { it > 0 }?.coerceIn(MIN_VOLUME_PERCENT + 1, MAX_VOLUME_PERCENT)
            ?: DEFAULT_VOLUME_PERCENT
    }

    companion object {
        const val MIN_VOLUME_PERCENT = 0
        const val MAX_VOLUME_PERCENT = 100
        const val DEFAULT_VOLUME_PERCENT = 100
        const val STEP_PERCENT = 5
    }
}
