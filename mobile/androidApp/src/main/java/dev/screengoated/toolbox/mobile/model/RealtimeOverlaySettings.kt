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

internal fun RealtimeTtsSettings.withDirectSpeech(directSpeech: Boolean): RealtimeTtsSettings =
    copy(enabled = enabled || directSpeech)

object RealtimeModelIds {
    const val TRANSCRIPTION_GEMINI_2_5 = GeneratedLiveModelCatalog.TRANSCRIPTION_GEMINI_2_5
    const val TRANSCRIPTION_GEMINI_3_1 = GeneratedLiveModelCatalog.TRANSCRIPTION_GEMINI_3_1
    const val TRANSCRIPTION_GEMINI_S2S = GeneratedLiveModelCatalog.TRANSCRIPTION_GEMINI_S2S
    const val TRANSCRIPTION_GEMINI_TRANSLATE = GeneratedLiveModelCatalog.TRANSCRIPTION_GEMINI_TRANSLATE
    const val TRANSCRIPTION_GEMINI_TRANSCRIBE = GeneratedLiveModelCatalog.TRANSCRIPTION_GEMINI_TRANSCRIBE
    const val GEMINI_LIVE_API_MODEL_2_5 = GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_2_5
    const val GEMINI_LIVE_API_MODEL_3_1 = GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_3_1
    const val GEMINI_LIVE_TRANSLATE_API_MODEL = GeneratedLiveModelCatalog.GEMINI_LIVE_TRANSLATE_API_MODEL
    const val GEMINI_TRANSCRIBE_API_MODEL = GeneratedLiveModelCatalog.GEMINI_TRANSCRIBE_API_MODEL

    const val TRANSLATION_LLM = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_LLM
    const val TRANSLATION_GTX = GeneratedLiveModelCatalog.TRANSLATION_PROVIDER_GTX

    fun defaultTranscriptionProvider(
        modelId: String = GeneratedLiveModelCatalog.DEFAULT_TRANSCRIPTION_PROVIDER_ID,
    ): ProviderDescriptor {
        return GeneratedLiveModelCatalog.defaultTranscriptionProvider(modelId)
    }

    fun normalizeTranscriptionModelId(modelId: String): String {
        return GeneratedLiveModelCatalog.normalizeTranscriptionModelId(modelId)
    }

    fun isGeminiS2sModelId(modelId: String): Boolean {
        val normalized = normalizeTranscriptionModelId(modelId)
        return normalized in GeneratedLiveModelCatalog.realtimeS2sModelIds
    }

    private const val ANDROID_PLATFORM = "android"

    private fun engineFacts(modelId: String) =
        GeneratedLiveModelCatalog.realtimeTranscriptionEngine(normalizeTranscriptionModelId(modelId))

    /**
     * True when Android cannot run the selector: the catalog lists it for another platform,
     * or it names an on-device engine and Android ships no on-device engine runtime
     * (docs/OFFLINE-ENGINES.md). Such selectors fail as unavailable, never as a missing key.
     */
    fun isUnavailableOnAndroid(modelId: String): Boolean =
        engineFacts(modelId)?.let { ANDROID_PLATFORM !in it.platforms || it.onDeviceEngine != null }
            ?: false

    /** Only cloud transcription that Android can run needs a Gemini key. */
    fun requiresGeminiApiKey(modelId: String): Boolean =
        engineFacts(modelId)?.let { it.onDeviceEngine == null && ANDROID_PLATFORM in it.platforms } ?: true

    fun normalizeTtsGeminiModel(apiModel: String): String {
        return GeneratedLiveModelCatalog.normalizeTtsGeminiModel(apiModel)
    }

    fun normalizeTranslationModelId(modelId: String): String {
        return when (modelId) {
            TRANSLATION_LLM, TRANSLATION_GTX -> modelId
            else -> GeneratedLiveModelCatalog.DEFAULT_TRANSLATION_PROVIDER_ID
        }
    }

    fun translationProviderDescriptor(id: String = TRANSLATION_LLM): ProviderDescriptor {
        return GeneratedLiveModelCatalog.translationProviderDescriptor(normalizeTranslationModelId(id))
    }
}
