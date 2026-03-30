package dev.screengoated.toolbox.mobile.bilingualrelay

import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject

internal const val BILINGUAL_RELAY_LIVE_WS_ENDPOINT =
    "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"

internal data class BilingualRelaySocketUpdate(
    val setupComplete: Boolean = false,
    val inputTranscript: String? = null,
    val outputTranscript: String? = null,
    val audioChunk: ByteArray? = null,
    val turnComplete: Boolean = false,
    val interrupted: Boolean = false,
    val error: String? = null,
    val goAway: Boolean = false,
)

internal fun buildBilingualRelaySetupPayload(
    model: String,
    instruction: String,
    voiceName: String,
): String {
    val generationConfig = JSONObject()
        .put("responseModalities", JSONArray().put("AUDIO"))
        .put("mediaResolution", "MEDIA_RESOLUTION_LOW")
        .put("thinkingConfig", JSONObject().put("thinkingLevel", "minimal"))
        .put(
            "speechConfig",
            JSONObject().put(
                "voiceConfig",
                JSONObject().put(
                    "prebuiltVoiceConfig",
                    JSONObject().put("voiceName", voiceName),
                ),
            ),
        )

    return JSONObject()
        .put(
            "setup",
            JSONObject()
                .put("model", "models/$model")
                .put("generationConfig", generationConfig)
                .put(
                    "systemInstruction",
                    JSONObject().put(
                        "parts",
                        JSONArray().put(JSONObject().put("text", instruction)),
                    ),
                )
                .put("contextWindowCompression", JSONObject().put("slidingWindow", JSONObject()))
                .put("inputAudioTranscription", JSONObject())
                .put("outputAudioTranscription", JSONObject()),
        )
        .toString()
}

internal fun buildBilingualRelayAudioPayload(pcmData: ShortArray): String {
    val bytes = ByteArray(pcmData.size * 2)
    pcmData.forEachIndexed { index, sample ->
        val byteIndex = index * 2
        bytes[byteIndex] = (sample.toInt() and 0xFF).toByte()
        bytes[byteIndex + 1] = ((sample.toInt() shr 8) and 0xFF).toByte()
    }
    return JSONObject()
        .put(
            "realtimeInput",
            JSONObject().put(
                "audio",
                JSONObject()
                    .put("mimeType", "audio/pcm;rate=16000")
                    .put("data", Base64.encodeToString(bytes, Base64.NO_WRAP)),
            ),
        )
        .toString()
}

internal fun buildBilingualRelayAudioStreamEndPayload(): String {
    return JSONObject()
        .put("realtimeInput", JSONObject().put("audioStreamEnd", true))
        .toString()
}

internal fun parseBilingualRelaySocketUpdate(message: String): BilingualRelaySocketUpdate {
    if (message.contains("setupComplete")) {
        return BilingualRelaySocketUpdate(setupComplete = true)
    }

    return runCatching {
        val root = JSONObject(message)

        // GoAway: server signals imminent termination
        if (root.has("goAway")) {
            return@runCatching BilingualRelaySocketUpdate(goAway = true)
        }

        val errorMessage = root.optJSONObject("error")
            ?.optString("message")
            ?.takeIf(String::isNotBlank)
        if (errorMessage != null) {
            return@runCatching BilingualRelaySocketUpdate(error = errorMessage)
        }

        val serverContent = root.optJSONObject("serverContent")
        val inputTranscript = serverContent
            ?.optJSONObject("inputTranscription")
            ?.optString("text")
            ?.takeIf(String::isNotBlank)
        val outputTranscript = serverContent
            ?.optJSONObject("outputTranscription")
            ?.optString("text")
            ?.takeIf(String::isNotBlank)
        val interrupted = serverContent?.optBoolean("interrupted") == true
        val turnComplete = serverContent?.optBoolean("turnComplete") == true ||
            serverContent?.optBoolean("generationComplete") == true

        var audioChunk: ByteArray? = null
        val parts = serverContent
            ?.optJSONObject("modelTurn")
            ?.optJSONArray("parts")
        if (parts != null) {
            for (index in 0 until parts.length()) {
                val part = parts.optJSONObject(index) ?: continue
                val base64 = part.optJSONObject("inlineData")
                    ?.optString("data")
                    ?.takeIf(String::isNotBlank)
                    ?: continue
                audioChunk = Base64.decode(base64, Base64.DEFAULT)
                break
            }
        }

        BilingualRelaySocketUpdate(
            inputTranscript = inputTranscript,
            outputTranscript = outputTranscript,
            audioChunk = audioChunk,
            turnComplete = turnComplete,
            interrupted = interrupted,
        )
    }.getOrDefault(BilingualRelaySocketUpdate())
}
