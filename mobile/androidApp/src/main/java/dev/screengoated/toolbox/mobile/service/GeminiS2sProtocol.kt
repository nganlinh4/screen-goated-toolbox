package dev.screengoated.toolbox.mobile.service

import android.util.Base64
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import org.json.JSONArray
import org.json.JSONObject
import java.util.Locale

data class GeminiS2sRuntimeSettings(
    val targetLanguage: String,
    val customInstruction: String,
    val globalTts: MobileGlobalTtsSettings,
    val realtime: RealtimeTtsSettings,
)

data class S2sDisplaySnapshot(
    val sourceCommitted: String,
    val sourceDraft: String,
    val targetCommitted: String,
    val targetDraft: String,
)

internal data class GeminiS2sParsedUpdate(
    val inputText: String = "",
    val outputText: String = "",
    val audioChunks: List<ByteArray> = emptyList(),
    val turnComplete: Boolean = false,
    val generationComplete: Boolean = false,
    val error: String? = null,
)

internal data class S2sSegment(
    val id: Long,
    val generation: Long,
    val samples: ShortArray,
    val speechFrames: Int,
    val peakRms: Float,
    val meanRms: Float,
    val energeticFrames: Int,
    val speechLikeFrames: Int,
    val activeMs: Long,
) {
    val audioMs: Int get() = samples.size * 1000 / 16_000
}

internal sealed interface S2sRaceEvent {
    val segmentId: Long
    val generation: Long
    val attempt: Int

    data class SourceText(
        override val segmentId: Long,
        override val generation: Long,
        override val attempt: Int,
        val text: String,
    ) : S2sRaceEvent

    data class TargetText(
        override val segmentId: Long,
        override val generation: Long,
        override val attempt: Int,
        val text: String,
    ) : S2sRaceEvent

    data class Audio(
        override val segmentId: Long,
        override val generation: Long,
        override val attempt: Int,
        val bytes: ByteArray,
    ) : S2sRaceEvent

    data class Done(
        override val segmentId: Long,
        override val generation: Long,
        override val attempt: Int,
    ) : S2sRaceEvent

    data class Retry(
        override val segmentId: Long,
        override val generation: Long,
        override val attempt: Int,
    ) : S2sRaceEvent
}

internal sealed interface S2sEvent {
    val segmentId: Long
    val generation: Long

    data class SourceText(
        override val segmentId: Long,
        override val generation: Long,
        val text: String,
    ) : S2sEvent

    data class TargetText(
        override val segmentId: Long,
        override val generation: Long,
        val text: String,
    ) : S2sEvent

    data class Audio(
        override val segmentId: Long,
        override val generation: Long,
        val bytes: ByteArray,
    ) : S2sEvent

    data class Done(
        override val segmentId: Long,
        override val generation: Long,
        val empty: Boolean,
    ) : S2sEvent

    data class Queued(
        override val segmentId: Long,
        override val generation: Long,
        val audioMs: Int,
        val queuedAtMs: Long,
    ) : S2sEvent
}

internal data class SegmentPlayback(
    var audioMs: Int,
    var queuedAtMs: Long = 0L,
    val audioChunks: ArrayDeque<ByteArray> = ArrayDeque(),
    var sourceText: String = "",
    var targetText: String = "",
    var done: Boolean = false,
    var empty: Boolean = false,
)

internal enum class SegmentResult {
    OK,
    RETRY_FRESH,
    EMPTY_FINAL,
}

internal enum class AdaptiveS2sVadOutcome {
    HEALTHY,
    EMPTY_NO_INPUT,
    RETRY_FRESH,
}

internal class S2sContextMemory {
    private val lines = ArrayDeque<String>()

    @Synchronized
    fun push(text: String) {
        val line = text.trim().takeIf { it.isNotBlank() } ?: return
        lines.addLast(line.take(CONTEXT_LINE_CHAR_LIMIT))
        while (lines.size > CONTEXT_SEGMENT_LIMIT) {
            lines.removeFirst()
        }
    }

    @Synchronized
    fun snapshot(): String {
        val selected = mutableListOf<String>()
        var total = 0
        for (line in lines.asReversed()) {
            if (total + line.length > CONTEXT_TOTAL_CHAR_LIMIT) {
                break
            }
            selected.add(line)
            total += line.length
        }
        return selected.asReversed().joinToString("\n")
    }

    private companion object {
        private const val CONTEXT_SEGMENT_LIMIT = 5
        private const val CONTEXT_LINE_CHAR_LIMIT = 240
        private const val CONTEXT_TOTAL_CHAR_LIMIT = 1_500
    }
}

internal fun buildGeminiS2sSetupPayload(
    model: String,
    settings: GeminiS2sRuntimeSettings,
    contextText: String = "",
): String {
    if (isGeminiTranslateApiModel(model)) {
        return JSONObject()
            .put(
                "setup",
                JSONObject()
                    .put("model", "models/$model")
                    .put(
                        "generationConfig",
                        JSONObject()
                            .put("responseModalities", JSONArray().put("AUDIO"))
                            .put(
                                "translationConfig",
                                JSONObject()
                                    .put("targetLanguageCode", targetLanguageCode(settings.targetLanguage))
                                    .put("echoTargetLanguage", true),
                            ),
                    )
                    .put("inputAudioTranscription", JSONObject())
                    .put("outputAudioTranscription", JSONObject()),
            )
            .toString()
    }

    val generationConfig = JSONObject()
        .put("responseModalities", JSONArray().put("AUDIO"))
        .put("mediaResolution", "MEDIA_RESOLUTION_LOW")
        .put("thinkingConfig", JSONObject().put("thinkingBudget", 0))
        .put(
            "speechConfig",
            JSONObject().put(
                "voiceConfig",
                JSONObject().put(
                    "prebuiltVoiceConfig",
                    JSONObject().put("voiceName", settings.globalTts.voice.ifBlank { "Aoede" }),
                ),
            ),
        )

    val instruction = buildString {
        append("Translate the user's speech directly into ")
        append(settings.targetLanguage)
        append(". Output only natural ")
        append(settings.targetLanguage)
        append(" speech. Do not explain, preface, or repeat the source language.")
        append(" Speak at ")
        append(settings.globalTts.speedPreset.toGeminiS2sSpeedLabel())
        append(" speed.")
        val custom = settings.customInstruction.trim()
        if (custom.isNotBlank()) {
            append('\n')
            append(custom)
        }
        if (contextText.isNotBlank()) {
            append("\nRecent translated context for continuity:\n")
            append(contextText)
        }
    }

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

private fun isGeminiTranslateApiModel(model: String): Boolean {
    return model.contains("live-translate") || model == RealtimeModelIds.GEMINI_LIVE_TRANSLATE_API_MODEL
}

internal fun targetLanguageCode(language: String): String {
    val trimmed = language.trim()
    if (trimmed.isEmpty()) {
        return "en"
    }

    return when (trimmed.lowercase(Locale.US)) {
        "chinese",
        "chinese (simplified)",
        "simplified chinese",
        "zh",
        "zh-cn",
        "zh-hans",
        "zh_hans" -> "zh-Hans"
        "chinese (traditional)",
        "traditional chinese",
        "zh-tw",
        "zh-hant",
        "zh_hant" -> "zh-Hant"
        "portuguese (brazil)",
        "brazilian portuguese",
        "pt-br",
        "pt_br" -> "pt-BR"
        "portuguese (portugal)",
        "european portuguese",
        "pt-pt",
        "pt_pt" -> "pt-PT"
        "filipino",
        "tagalog" -> "fil"
        "norwegian" -> "no"
        else -> {
            if (isBcp47Like(trimmed)) {
                normalizeBcp47Code(trimmed)
            } else {
                dev.screengoated.toolbox.mobile.model.LanguageCatalog.codeForName(trimmed)
                    .lowercase(Locale.US)
                    .ifBlank { "en" }
            }
        }
    }
}

private fun isBcp47Like(value: String): Boolean {
    val parts = value.split('-')
    val language = parts.firstOrNull() ?: return false
    return language.length in 2..3 &&
        language.all { it.isLetter() } &&
        parts.drop(1).all { part ->
            part.isNotEmpty() && part.length <= 8 && part.all { it.isLetterOrDigit() }
        }
}

private fun normalizeBcp47Code(code: String): String {
    return when (code.lowercase(Locale.US)) {
        "zh-hans" -> "zh-Hans"
        "zh-hant" -> "zh-Hant"
        "pt-br" -> "pt-BR"
        "pt-pt" -> "pt-PT"
        else -> code.lowercase(Locale.US)
    }
}

internal fun buildGeminiS2sAudioPayload(samples: ShortArray): String {
    val bytes = ByteArray(samples.size * 2)
    samples.forEachIndexed { index, sample ->
        val byteIndex = index * 2
        bytes[byteIndex] = (sample.toInt() and 0xFF).toByte()
        bytes[byteIndex + 1] = ((sample.toInt() shr 8) and 0xFF).toByte()
    }
    val encoded = Base64.encodeToString(bytes, Base64.NO_WRAP)
    return JSONObject()
        .put(
            "realtimeInput",
            JSONObject().put(
                "audio",
                JSONObject()
                    .put("data", encoded)
                    .put("mimeType", "audio/pcm;rate=16000"),
            ),
        )
        .toString()
}

internal fun buildGeminiS2sAudioStreamEndPayload(): String {
    return JSONObject()
        .put("realtimeInput", JSONObject().put("audioStreamEnd", true))
        .toString()
}

internal fun parseGeminiS2sUpdate(message: String): GeminiS2sParsedUpdate {
    return runCatching {
        val root = JSONObject(message)
        val error = root.optJSONObject("error")
            ?.optString("message")
            ?.takeIf(String::isNotBlank)
        val serverContent = root.optJSONObject("serverContent")
        val modelTurn = serverContent?.optJSONObject("modelTurn")
        val audio = mutableListOf<ByteArray>()
        val parts = modelTurn?.optJSONArray("parts")
        if (parts != null) {
            for (index in 0 until parts.length()) {
                val inlineData = parts.optJSONObject(index)?.optJSONObject("inlineData")
                val data = inlineData?.optString("data")?.takeIf(String::isNotBlank)
                if (data != null) {
                    audio.add(Base64.decode(data, Base64.DEFAULT))
                }
            }
        }
        GeminiS2sParsedUpdate(
            inputText = serverContent
                ?.optJSONObject("inputTranscription")
                ?.optString("text")
                ?.takeIf(String::isNotBlank)
                .orEmpty(),
            outputText = serverContent
                ?.optJSONObject("outputTranscription")
                ?.optString("text")
                ?.takeIf(String::isNotBlank)
                .orEmpty(),
            audioChunks = audio,
            turnComplete = serverContent?.optBoolean("turnComplete", false) == true,
            generationComplete = serverContent?.optBoolean("generationComplete", false) == true,
            error = error,
        )
    }.getOrElse { GeminiS2sParsedUpdate() }
}

internal fun mergeGeminiS2sSegmentText(
    existing: String,
    incoming: String,
): String {
    val left = existing.trimEnd()
    val right = incoming.trim()
    if (left.isEmpty()) {
        return right
    }
    if (right.isEmpty()) {
        return left
    }
    val lowerLeft = left.lowercase(Locale.ROOT)
    val lowerRight = right.lowercase(Locale.ROOT)
    val maxOverlap = minOf(left.length, right.length, 80)
    for (size in maxOverlap downTo 3) {
        if (lowerLeft.takeLast(size) == lowerRight.take(size)) {
            val suffix = right.substring(size).trimStart()
            return if (suffix.isBlank()) left else "$left $suffix"
        }
    }
    return "$left $right"
}

private fun MobileTtsSpeedPreset.toGeminiS2sSpeedLabel(): String {
    return when (this) {
        MobileTtsSpeedPreset.SLOW -> "Slow"
        MobileTtsSpeedPreset.NORMAL -> "Normal"
        MobileTtsSpeedPreset.FAST -> "Fast"
    }
}
