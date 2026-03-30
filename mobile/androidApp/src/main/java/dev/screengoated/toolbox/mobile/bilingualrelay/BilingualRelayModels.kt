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
    val first: BilingualRelayLanguageProfile = BilingualRelayLanguageProfile(),
    val second: BilingualRelayLanguageProfile = BilingualRelayLanguageProfile(),
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

        return """
            I have 2 languages: ${describe(normalized.first)} and ${describe(normalized.second)}.
            When I speak whatever sentence, detect the language I speak and repeat the sentence but in the other language.
            DO NOT return any other follow up text or introductory text.
        """.trimIndent()
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
