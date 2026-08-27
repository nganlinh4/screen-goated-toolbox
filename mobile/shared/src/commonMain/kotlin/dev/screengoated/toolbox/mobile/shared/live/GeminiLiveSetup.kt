package dev.screengoated.toolbox.mobile.shared.live

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

enum class GeminiLiveMediaResolution(val apiValue: String) {
    LOW("MEDIA_RESOLUTION_LOW"),
    HIGH("MEDIA_RESOLUTION_HIGH"),
}

enum class GeminiLiveTranscriptionMode {
    NONE,
    INPUT,
    OUTPUT,
    BOTH,
}

data class GeminiLiveSetupSpec(
    val apiModel: String,
    val responseModalities: List<String> = listOf("AUDIO"),
    val mediaResolution: GeminiLiveMediaResolution? = null,
    val voiceName: String? = null,
    val systemInstruction: String? = null,
    val transcriptionMode: GeminiLiveTranscriptionMode = GeminiLiveTranscriptionMode.NONE,
    val inputAudioTranscriptionConfig: JsonObject = JsonObject(emptyMap()),
    val contextWindowCompression: Boolean = false,
    val reasoningOverride: GeminiLiveReasoningOverride? = null,
    val generationOverrides: JsonObject = JsonObject(emptyMap()),
    val setupExtensions: JsonObject = JsonObject(emptyMap()),
)

data class GeminiLiveReasoningOverride(
    val thinkingLevel: String,
    val includeThoughts: Boolean = false,
)

/** Builds a complete Live setup envelope and always applies endpoint-owned policy. */
fun buildGeminiLiveSetup(spec: GeminiLiveSetupSpec): JsonObject = buildJsonObject {
    put(
        "setup",
        buildJsonObject {
            spec.setupExtensions.forEach { (name, value) -> put(name, value) }
            put("model", "models/${spec.apiModel}")
            put("generationConfig", buildGenerationConfig(spec))
            spec.systemInstruction?.let { instruction ->
                put(
                    "systemInstruction",
                    buildJsonObject {
                        put(
                            "parts",
                            buildJsonArray {
                                add(buildJsonObject { put("text", instruction) })
                            },
                        )
                    },
                )
            }
            if (
                spec.transcriptionMode == GeminiLiveTranscriptionMode.INPUT ||
                spec.transcriptionMode == GeminiLiveTranscriptionMode.BOTH
            ) {
                put("inputAudioTranscription", spec.inputAudioTranscriptionConfig)
            }
            if (
                spec.transcriptionMode == GeminiLiveTranscriptionMode.OUTPUT ||
                spec.transcriptionMode == GeminiLiveTranscriptionMode.BOTH
            ) {
                put("outputAudioTranscription", buildJsonObject {})
            }
            if (spec.contextWindowCompression) {
                put(
                    "contextWindowCompression",
                    buildJsonObject { put("slidingWindow", buildJsonObject {}) },
                )
            }
        },
    )
}

private fun buildGenerationConfig(spec: GeminiLiveSetupSpec): JsonObject = buildJsonObject {
    spec.generationOverrides.forEach { (name, value) -> put(name, value) }
    put(
        "responseModalities",
        buildJsonArray {
            spec.responseModalities.forEach { modality -> add(JsonPrimitive(modality)) }
        },
    )
    val endpoint = GeneratedLiveModelCatalog.endpointProfile(spec.apiModel)
    endpoint?.maxOutputTokens?.let {
        put("maxOutputTokens", it)
    }
    if (spec.reasoningOverride != null) {
        put(
            "thinkingConfig",
            buildJsonObject {
                put("thinkingLevel", spec.reasoningOverride.thinkingLevel)
                if (spec.reasoningOverride.includeThoughts) {
                    put("includeThoughts", true)
                }
            },
        )
    } else {
        when (val thinking = endpoint?.thinking) {
            is GeneratedLiveThinkingConfig.Budget -> put(
                "thinkingConfig",
                buildJsonObject { put("thinkingBudget", thinking.value) },
            )
            is GeneratedLiveThinkingConfig.Level -> put(
                "thinkingConfig",
                buildJsonObject { put("thinkingLevel", thinking.value) },
            )
            null -> Unit
        }
    }
    spec.mediaResolution?.let { put("mediaResolution", it.apiValue) }
    spec.voiceName?.let { voiceName ->
        put(
            "speechConfig",
            buildJsonObject {
                put(
                    "voiceConfig",
                    buildJsonObject {
                        put(
                            "prebuiltVoiceConfig",
                            buildJsonObject { put("voiceName", voiceName) },
                        )
                    },
                )
            },
        )
    }
}
