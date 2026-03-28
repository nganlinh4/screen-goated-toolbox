package dev.screengoated.toolbox.mobile.model

import dev.screengoated.toolbox.mobile.shared.live.ProviderDescriptor

data class RealtimePaneFontSizes(
    val transcriptionSp: Int = 16,
    val translationSp: Int = 16,
)

data class RealtimeTtsSettings(
    val enabled: Boolean = false,
    val speedPercent: Int = 100,
    val autoSpeed: Boolean = true,
    val volumePercent: Int = 100,
)

object RealtimeModelIds {
    const val TRANSCRIPTION_GEMINI_2_5 = "gemini-live-audio"
    const val TRANSCRIPTION_GEMINI_3_1 = "gemini-live-audio-3.1"
    const val TRANSCRIPTION_PARAKEET = "parakeet"
    const val GEMINI_LIVE_API_MODEL_2_5 = "gemini-2.5-flash-native-audio-preview-12-2025"
    const val GEMINI_LIVE_API_MODEL_3_1 = "gemini-3.1-flash-live-preview"

    const val TRANSLATION_TAALAS = "taalas-rt"
    const val TRANSLATION_GEMMA = "google-gemma"
    const val TRANSLATION_GTX = "google-gtx"

    fun defaultTranscriptionProvider(modelId: String = TRANSCRIPTION_GEMINI_2_5): ProviderDescriptor {
        return when (modelId) {
            TRANSCRIPTION_PARAKEET -> ProviderDescriptor(
                id = TRANSCRIPTION_PARAKEET,
                model = "realtime_eou_120m-v1-onnx",
            )

            else -> ProviderDescriptor(
                id = TRANSCRIPTION_GEMINI_2_5,
                model = GEMINI_LIVE_API_MODEL_2_5,
            )
        }
    }

    fun normalizeTranscriptionModelId(modelId: String): String {
        return when (modelId) {
            "",
            "gemini",
            TRANSCRIPTION_GEMINI_2_5,
            GEMINI_LIVE_API_MODEL_2_5,
            "gemini-live-audio-2.5" -> TRANSCRIPTION_GEMINI_2_5

            TRANSCRIPTION_GEMINI_3_1,
            GEMINI_LIVE_API_MODEL_3_1 -> TRANSCRIPTION_GEMINI_2_5

            TRANSCRIPTION_PARAKEET -> TRANSCRIPTION_PARAKEET
            else -> TRANSCRIPTION_GEMINI_2_5
        }
    }
}
