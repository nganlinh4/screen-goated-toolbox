package dev.screengoated.toolbox.mobile.model

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
    const val TRANSCRIPTION_GEMINI = "gemini"
    const val TRANSCRIPTION_PARAKEET = "parakeet"

    const val TRANSLATION_TAALAS = "taalas-rt"
    const val TRANSLATION_GEMMA = "google-gemma"
    const val TRANSLATION_GTX = "google-gtx"
}
