package dev.screengoated.toolbox.mobile.bilingualrelay

import kotlinx.serialization.Serializable

@Serializable
data class BilingualRelayLanguageProfile(
    val language: String = "",
    val accent: String = "",
    val tone: String = "",
) {
    fun normalized(): BilingualRelayLanguageProfile {
        return copy(
            language = language.trim(),
            accent = accent.trim(),
            tone = tone.trim(),
        )
    }
}

@Serializable
data class BilingualRelayConfig(
    val first: BilingualRelayLanguageProfile = BilingualRelayLanguageProfile(language = "English"),
    val second: BilingualRelayLanguageProfile = BilingualRelayLanguageProfile(language = "Korean", accent = "Busan", tone = "polite"),
) {
    fun normalized(): BilingualRelayConfig {
        return copy(first = first.normalized(), second = second.normalized())
    }

    fun isValid(): Boolean {
        val normalized = normalized()
        return normalized.first.language.isNotBlank() && normalized.second.language.isNotBlank()
    }

    fun buildSystemInstruction(): String {
        val normalized = normalized()
        fun describe(profile: BilingualRelayLanguageProfile): String {
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

enum class BilingualRelayConnectionState {
    NOT_CONFIGURED,
    CONNECTING,
    READY,
    RECONNECTING,
    ERROR,
    STOPPED,
}

enum class BilingualRelayTranscriptRole {
    INPUT,
    OUTPUT,
    SEPARATOR,
}

data class BilingualRelayTranscriptItem(
    val id: Long,
    val role: BilingualRelayTranscriptRole,
    val text: String,
    val isFinal: Boolean,
    val updatedAtMs: Long,
    val lang: String = "",
)

data class BilingualRelayState(
    val appliedConfig: BilingualRelayConfig = BilingualRelayConfig(),
    val draftConfig: BilingualRelayConfig = BilingualRelayConfig(),
    val dirty: Boolean = false,
    val connectionState: BilingualRelayConnectionState = BilingualRelayConnectionState.NOT_CONFIGURED,
    val isRunning: Boolean = false,
    val transcripts: List<BilingualRelayTranscriptItem> = emptyList(),
    val lastError: String? = null,
    val visualizerLevel: Float = 0f,
)

internal fun BilingualRelayConnectionState.isReady(): Boolean =
    this == BilingualRelayConnectionState.READY
