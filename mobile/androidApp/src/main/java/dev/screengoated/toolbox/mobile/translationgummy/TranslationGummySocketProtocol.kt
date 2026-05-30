package dev.screengoated.toolbox.mobile.translationgummy

import android.util.Base64
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

internal const val TRANSLATION_GUMMY_LIVE_WS_ENDPOINT =
    "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"

private val translationGummyJson = Json { ignoreUnknownKeys = true }

internal data class TranslationGummySocketUpdate(
    val setupComplete: Boolean = false,
    val inputTranscript: String? = null,
    val outputTranscript: String? = null,
    val audioChunk: ByteArray? = null,
    val turnComplete: Boolean = false,
    val interrupted: Boolean = false,
    val error: String? = null,
    val goAway: Boolean = false,
)

internal fun buildTranslationGummySetupPayload(
    model: String,
    instruction: String,
    voiceName: String,
): String {
    return buildJsonObject {
        put(
            "setup",
            buildJsonObject {
                put("model", "models/$model")
                put(
                    "generationConfig",
                    buildJsonObject {
                        put("responseModalities", buildJsonArray { add(JsonPrimitive("AUDIO")) })
                        put("mediaResolution", "MEDIA_RESOLUTION_LOW")
                        put("thinkingConfig", buildJsonObject { put("thinkingBudget", 0) })
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
                    },
                )
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
                put(
                    "realtimeInputConfig",
                    buildJsonObject {
                        put(
                            "automaticActivityDetection",
                            buildJsonObject {
                                put("startOfSpeechSensitivity", "START_SENSITIVITY_HIGH")
                                put("endOfSpeechSensitivity", "END_SENSITIVITY_HIGH")
                                put("prefixPaddingMs", 80)
                                put("silenceDurationMs", 320)
                            },
                        )
                        put("activityHandling", "START_OF_ACTIVITY_INTERRUPTS")
                        put("turnCoverage", "TURN_INCLUDES_ONLY_ACTIVITY")
                    },
                )
                put(
                    "contextWindowCompression",
                    buildJsonObject { put("slidingWindow", buildJsonObject {}) },
                )
                put("inputAudioTranscription", buildJsonObject {})
                put("outputAudioTranscription", buildJsonObject {})
            },
        )
    }.toString()
}

internal fun buildTranslationGummyAudioPayload(pcmData: ShortArray): String {
    val bytes = ByteArray(pcmData.size * 2)
    pcmData.forEachIndexed { index, sample ->
        val byteIndex = index * 2
        bytes[byteIndex] = (sample.toInt() and 0xFF).toByte()
        bytes[byteIndex + 1] = ((sample.toInt() shr 8) and 0xFF).toByte()
    }
    return buildJsonObject {
        put(
            "realtimeInput",
            buildJsonObject {
                put(
                    "audio",
                    buildJsonObject {
                        put("mimeType", "audio/pcm;rate=16000")
                        put("data", Base64.encodeToString(bytes, Base64.NO_WRAP))
                    },
                )
            },
        )
    }.toString()
}

internal fun buildTranslationGummyAudioStreamEndPayload(): String {
    return buildJsonObject {
        put("realtimeInput", buildJsonObject { put("audioStreamEnd", true) })
    }.toString()
}

internal fun parseTranslationGummySocketUpdate(message: String): TranslationGummySocketUpdate {
    return runCatching {
        val root = translationGummyJson.parseToJsonElement(message).jsonObject
        if (root.containsKey("setupComplete")) {
            return@runCatching TranslationGummySocketUpdate(setupComplete = true)
        }

        // GoAway: server signals imminent termination
        if (root.containsKey("goAway")) {
            return@runCatching TranslationGummySocketUpdate(goAway = true)
        }

        val errorMessage = root.objectOrNull("error")
            ?.stringOrNull("message")
            ?.takeIf(String::isNotBlank)
        if (errorMessage != null) {
            return@runCatching TranslationGummySocketUpdate(error = errorMessage)
        }

        val serverContent = root.objectOrNull("serverContent")
        val inputTranscript = serverContent
            ?.objectOrNull("inputTranscription")
            ?.stringOrNull("text")
            ?.takeIf(String::isNotBlank)
        val outputTranscript = serverContent
            ?.objectOrNull("outputTranscription")
            ?.stringOrNull("text")
            ?.takeIf(String::isNotBlank)
        val interrupted = serverContent?.booleanOrFalse("interrupted") == true
        val turnComplete = serverContent?.booleanOrFalse("turnComplete") == true ||
            serverContent?.booleanOrFalse("generationComplete") == true

        var audioChunk: ByteArray? = null
        val parts = serverContent
            ?.objectOrNull("modelTurn")
            ?.arrayOrNull("parts")
        if (parts != null) {
            for (partElement in parts) {
                val part = partElement as? JsonObject ?: continue
                val base64 = part.objectOrNull("inlineData")
                    ?.stringOrNull("data")
                    ?.takeIf(String::isNotBlank)
                    ?: continue
                audioChunk = Base64.decode(base64, Base64.DEFAULT)
                break
            }
        }

        TranslationGummySocketUpdate(
            inputTranscript = inputTranscript,
            outputTranscript = outputTranscript,
            audioChunk = audioChunk,
            turnComplete = turnComplete,
            interrupted = interrupted,
        )
    }.getOrDefault(TranslationGummySocketUpdate())
}

private fun JsonObject.objectOrNull(key: String): JsonObject? =
    get(key) as? JsonObject

private fun JsonObject.arrayOrNull(key: String): JsonArray? =
    get(key) as? JsonArray

private fun JsonObject.stringOrNull(key: String): String? =
    get(key)?.jsonPrimitive?.contentOrNull

private fun JsonObject.booleanOrFalse(key: String): Boolean =
    get(key)?.jsonPrimitive?.booleanOrNull == true
