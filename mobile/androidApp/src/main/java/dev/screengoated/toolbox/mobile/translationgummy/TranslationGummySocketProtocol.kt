package dev.screengoated.toolbox.mobile.translationgummy

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveMediaResolution
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSetupSpec
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveTranscriptionMode
import dev.screengoated.toolbox.mobile.shared.live.buildGeminiLiveSetup
import dev.screengoated.toolbox.mobile.shared.live.parseGeminiLiveServerFrame
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.util.Base64

internal data class TranslationGummySocketUpdate(
    val setupComplete: Boolean = false,
    val inputTranscript: String? = null,
    val outputTranscript: String? = null,
    val audioChunks: List<ByteArray> = emptyList(),
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
    return buildGeminiLiveSetup(
        GeminiLiveSetupSpec(
            apiModel = model,
            mediaResolution = GeminiLiveMediaResolution.LOW,
            voiceName = voiceName,
            systemInstruction = instruction,
            transcriptionMode = GeminiLiveTranscriptionMode.BOTH,
            contextWindowCompression = true,
            setupExtensions = buildJsonObject {
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
            },
        ),
    ).toString()
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
                        put("data", Base64.getEncoder().encodeToString(bytes))
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
    val frame = parseGeminiLiveServerFrame(message) ?: return TranslationGummySocketUpdate()
    return parseTranslationGummySocketUpdate(frame)
}

internal fun parseTranslationGummySocketUpdate(frame: GeminiLiveServerFrame): TranslationGummySocketUpdate {
    return TranslationGummySocketUpdate(
        setupComplete = frame.setupComplete,
        inputTranscript = frame.inputTranscript,
        outputTranscript = frame.outputTranscript,
        audioChunks = frame.audioParts.mapNotNull { part ->
            runCatching { Base64.getDecoder().decode(part.data) }.getOrNull()
        },
        turnComplete = frame.responseComplete,
        interrupted = frame.interrupted,
        error = frame.error,
        goAway = frame.goAway,
    )
}
