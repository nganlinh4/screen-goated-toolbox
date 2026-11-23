package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import java.util.Locale
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class VoicePreviewSamples private constructor(
    private val samples: Map<String, String>,
    private val fallback: String,
) {
    fun forLanguage(language: String): String = samples[language.lowercase(Locale.ROOT)] ?: fallback
    fun usesFallback(language: String): Boolean = !samples.containsKey(language.lowercase(Locale.ROOT))

    companion object {
        fun parse(source: String): VoicePreviewSamples {
            val root = Json.parseToJsonElement(source).jsonObject
            val samples = root.getValue("samples").jsonObject.mapValues { it.value.jsonPrimitive.content }
            return VoicePreviewSamples(samples, root.getValue("fallback").jsonPrimitive.content)
        }
    }
}

internal fun edgeVoicePreviewRequest(
    settings: MobileGlobalTtsSettings,
    language: String,
    voice: String,
    samples: VoicePreviewSamples,
): TtsRequest = TtsRequest(
    text = samples.forLanguage(language),
    consumer = TtsConsumer.SETTINGS_PREVIEW,
    priority = TtsPriority.PREVIEW,
    requestMode = TtsRequestMode.NORMAL,
    settingsSnapshot = settings.copy(method = MobileTtsMethod.EDGE_TTS).toRuntimeSnapshot(),
    ownerToken = "settings-preview",
    edgeVoiceOverride = voice,
)
