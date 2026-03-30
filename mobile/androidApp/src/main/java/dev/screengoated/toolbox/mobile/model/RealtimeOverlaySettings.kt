package dev.screengoated.toolbox.mobile.model

import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog
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
    const val TRANSCRIPTION_GEMINI_2_5 = GeneratedLiveModelCatalog.TRANSCRIPTION_GEMINI_2_5
    const val TRANSCRIPTION_GEMINI_3_1 = GeneratedLiveModelCatalog.TRANSCRIPTION_GEMINI_3_1
    const val TRANSCRIPTION_PARAKEET = GeneratedLiveModelCatalog.TRANSCRIPTION_PARAKEET
    const val GEMINI_LIVE_API_MODEL_2_5 = GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_2_5
    const val GEMINI_LIVE_API_MODEL_3_1 = GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_3_1

    const val TRANSLATION_CEREBRAS = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_CEREBRAS
    const val TRANSLATION_GEMMA = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_GEMMA
    const val TRANSLATION_GTX = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_GTX

    fun defaultTranscriptionProvider(modelId: String = TRANSCRIPTION_GEMINI_2_5): ProviderDescriptor {
        return GeneratedLiveModelCatalog.defaultTranscriptionProvider(modelId)
    }

    fun normalizeTranscriptionModelId(modelId: String): String {
        return GeneratedLiveModelCatalog.normalizeTranscriptionModelId(modelId)
    }

    fun normalizeTtsGeminiModel(apiModel: String): String {
        return GeneratedLiveModelCatalog.normalizeTtsGeminiModel(apiModel)
    }

    fun normalizeTranslationModelId(modelId: String): String {
        return when (modelId) {
            "taalas-rt" -> TRANSLATION_CEREBRAS
            TRANSLATION_CEREBRAS, TRANSLATION_GEMMA, TRANSLATION_GTX -> modelId
            else -> GeneratedLiveModelCatalog.DEFAULT_TRANSLATION_PROVIDER_ID
        }
    }

    fun translationProviderDescriptor(id: String = TRANSLATION_CEREBRAS): ProviderDescriptor {
        return GeneratedLiveModelCatalog.translationProviderDescriptor(normalizeTranslationModelId(id))
    }
}
