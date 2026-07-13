package dev.screengoated.toolbox.mobile.shared.live

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import okhttp3.Request
import java.util.Base64

private val geminiLiveProtocolJson = Json { ignoreUnknownKeys = true }

internal const val GEMINI_LIVE_WEBSOCKET_ENDPOINT =
    "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"

internal fun geminiLiveWebSocketRequest(apiKey: String): Request {
    val endpoint = Request.Builder()
        .url(GEMINI_LIVE_WEBSOCKET_ENDPOINT)
        .build()
        .url
    val url = endpoint
        .newBuilder()
        .addQueryParameter("key", apiKey)
        .build()
    return Request.Builder().url(url).build()
}

internal data class GeminiLiveInlineData(
    val mimeType: String?,
    val data: String,
)

internal data class GeminiLiveContentPart(
    val text: String?,
    val thought: Boolean,
    val inlineData: GeminiLiveInlineData?,
)

internal data class GeminiLiveFunctionCall(
    val id: String,
    val name: String,
    val args: JsonElement = JsonNull,
)

internal data class GeminiLiveSessionResumption(
    val handle: String?,
    val resumable: Boolean,
)

internal data class GeminiLiveServerFrame(
    val setupComplete: Boolean = false,
    val serverContentPresent: Boolean = false,
    val error: String? = null,
    val errorRetryable: Boolean = false,
    val inputTranscript: String? = null,
    val outputTranscript: String? = null,
    val contentParts: List<GeminiLiveContentPart> = emptyList(),
    val turnComplete: Boolean = false,
    val generationComplete: Boolean = false,
    val interrupted: Boolean = false,
    val goAway: Boolean = false,
    val goAwayTimeLeft: String? = null,
    val goAwayTimeLeftMs: Long? = null,
    val toolCalls: List<GeminiLiveFunctionCall> = emptyList(),
    val toolCallPresent: Boolean = false,
    val toolCancellationIds: List<String>? = null,
    val sessionResumption: GeminiLiveSessionResumption? = null,
    val usageMetadata: JsonElement? = null,
    val recognized: Boolean = false,
) {
    val responseComplete: Boolean
        get() = turnComplete || generationComplete

    val audioParts: List<GeminiLiveInlineData>
        get() = contentParts.mapNotNull(GeminiLiveContentPart::inlineData)

    val visibleTextParts: List<String>
        get() = contentParts.mapNotNull { part ->
            part.text?.takeUnless { part.thought }
        }

    val toolCallIds: List<String>
        get() = toolCalls.map(GeminiLiveFunctionCall::id)

    val contentCount: Int
        get() = contentParts.count { it.text != null } +
            audioParts.size +
            listOfNotNull(inputTranscript, outputTranscript).size

    val hasPostSetupObservation: Boolean
        get() = serverContentPresent ||
            toolCallPresent ||
            toolCancellationIds != null ||
            goAway ||
            sessionResumption != null ||
            usageMetadata != null
}

internal fun parseGeminiLiveServerFrame(message: String): GeminiLiveServerFrame? {
    val candidate = message.trim()
    if (!candidate.looksLikeStrictJsonValue()) {
        return null
    }
    return runCatching {
        val root = geminiLiveProtocolJson.parseToJsonElement(candidate) as? JsonObject
            ?: return@runCatching GeminiLiveServerFrame()
        val serverContent = root.objectOrNull("serverContent")
        val parts = buildList {
            val jsonParts = serverContent
                ?.objectOrNull("modelTurn")
                ?.arrayOrNull("parts")
                ?: return@buildList
            for (element in jsonParts) {
                val part = element as? JsonObject ?: continue
                val inlineData = part.objectOrNull("inlineData")?.let { inline ->
                    inline.stringOrNull("data")
                        ?.takeIf(String::isNotBlank)
                        ?.takeIf(String::isValidBase64)
                        ?.let { data ->
                            GeminiLiveInlineData(
                                mimeType = inline.stringOrNull("mimeType")
                                    .orEmpty()
                                    .takeIf(String::isNotBlank),
                                data = data,
                            )
                        }
                }
                add(
                    GeminiLiveContentPart(
                        text = part.stringOrNull("text")
                            ?.takeIf(String::isNotBlank),
                        thought = part.booleanOrFalse("thought"),
                        inlineData = inlineData,
                    ),
                )
            }
        }
        val toolCalls = root
            .objectOrNull("toolCall")
            ?.arrayOrNull("functionCalls")
            .orEmpty()
            .map { element ->
                val call = element as? JsonObject
                GeminiLiveFunctionCall(
                    id = call?.stringOrNull("id").orEmpty(),
                    name = call?.stringOrNull("name").orEmpty(),
                    args = call?.get("args") ?: JsonNull,
                )
            }
        val toolCancellationIds = if (root.containsKey("toolCallCancellation")) {
            root.objectOrNull("toolCallCancellation")
                ?.arrayOrNull("ids")
                .orEmpty()
                .mapNotNull { id ->
                    (id as? JsonPrimitive)
                        ?.takeIf(JsonPrimitive::isString)
                        ?.content
                }
        } else {
            null
        }
        val goAway = root.containsKey("goAway")
        val goAwayTimeLeft = root
            .objectOrNull("goAway")
            ?.stringOrNull("timeLeft")
        val sessionResumption = if (root.containsKey("sessionResumptionUpdate")) {
            val update = root.objectOrNull("sessionResumptionUpdate")
            GeminiLiveSessionResumption(
                handle = update?.stringOrNull("newHandle"),
                resumable = update?.booleanOrFalse("resumable") == true,
            )
        } else {
            null
        }

        GeminiLiveServerFrame(
            setupComplete = root.containsKey("setupComplete"),
            serverContentPresent = root.containsKey("serverContent"),
            error = root["error"].geminiLiveErrorMessage(),
            errorRetryable = root["error"].geminiLiveErrorRetryable(),
            inputTranscript = serverContent
                ?.objectOrNull("inputTranscription")
                ?.stringOrNull("text")
                ?.takeIf(String::isNotBlank),
            outputTranscript = serverContent
                ?.objectOrNull("outputTranscription")
                ?.stringOrNull("text")
                ?.takeIf(String::isNotBlank),
            contentParts = parts,
            turnComplete = serverContent?.booleanOrFalse("turnComplete") == true,
            generationComplete = serverContent?.booleanOrFalse("generationComplete") == true,
            interrupted = serverContent?.booleanOrFalse("interrupted") == true,
            goAway = goAway,
            goAwayTimeLeft = goAwayTimeLeft,
            goAwayTimeLeftMs = protobufDurationMs(goAwayTimeLeft),
            toolCalls = toolCalls,
            toolCallPresent = root.containsKey("toolCall"),
            toolCancellationIds = toolCancellationIds,
            sessionResumption = sessionResumption,
            usageMetadata = root["usageMetadata"],
            recognized = RECOGNIZED_SERVER_FIELDS.any(root::containsKey),
        )
    }.getOrNull()
}

private fun protobufDurationMs(value: String?): Long? {
    val match = value?.let(PROTOBUF_DURATION::matchEntire) ?: return null
    val seconds = match.groupValues[1].toLongOrNull() ?: return null
    val nanos = match.groupValues[2].padEnd(9, '0').toIntOrNull() ?: return null
    val roundedFractionMs = (nanos + 500_000L) / 1_000_000L
    if (seconds > (Long.MAX_VALUE - roundedFractionMs) / 1_000L) {
        return Long.MAX_VALUE
    }
    return seconds * 1_000L + roundedFractionMs
}

private fun JsonObject.objectOrNull(key: String): JsonObject? =
    get(key) as? JsonObject

private fun JsonObject.arrayOrNull(key: String): JsonArray? =
    get(key) as? JsonArray

private fun JsonObject.stringOrNull(key: String): String? =
    (get(key) as? JsonPrimitive)
        ?.takeIf(JsonPrimitive::isString)
        ?.content

private fun JsonObject.booleanOrFalse(key: String): Boolean =
    (get(key) as? JsonPrimitive)?.booleanOrNull == true

private fun JsonElement?.geminiLiveErrorMessage(): String? {
    return when (this) {
        null,
        JsonNull,
        -> null
        is JsonObject -> stringOrNull("message")
            ?.takeIf(String::isNotBlank)
            ?: toString().takeIf(String::isNotBlank)
        is JsonPrimitive -> content.takeIf(String::isNotBlank)
        else -> toString().takeIf(String::isNotBlank)
    }
}

private fun JsonElement?.geminiLiveErrorRetryable(): Boolean {
    val error = this as? JsonObject ?: return false
    val code = (error["code"] as? JsonPrimitive)
        ?.takeUnless(JsonPrimitive::isString)
        ?.content
        ?.toLongOrNull()
    val status = error.stringOrNull("status")
    return (code != null && code in RETRYABLE_ERROR_CODES) ||
        (status != null && status in RETRYABLE_ERROR_STATUSES)
}

private fun String.looksLikeStrictJsonValue(): Boolean {
    return startsWith('{') ||
        startsWith('[') ||
        startsWith('"') ||
        this == "true" ||
        this == "false" ||
        this == "null" ||
        JSON_NUMBER.matches(this)
}

private fun String.isValidBase64(): Boolean {
    if (length % 4 != 0) {
        return false
    }
    return runCatching {
        val decoded = Base64.getDecoder().decode(this)
        Base64.getEncoder().encodeToString(decoded) == this
    }.getOrDefault(false)
}

private val JSON_NUMBER = Regex("""^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?$""")

private val PROTOBUF_DURATION = Regex("""^(\d+)(?:\.(\d{1,9}))?s$""")

private val RETRYABLE_ERROR_CODES = setOf(408L, 429L, 500L, 502L, 503L, 504L)

private val RETRYABLE_ERROR_STATUSES = setOf(
    "ABORTED",
    "DEADLINE_EXCEEDED",
    "INTERNAL",
    "RESOURCE_EXHAUSTED",
    "UNAVAILABLE",
)

private val RECOGNIZED_SERVER_FIELDS = setOf(
    "setupComplete",
    "serverContent",
    "error",
    "toolCall",
    "toolCallCancellation",
    "goAway",
    "sessionResumptionUpdate",
    "usageMetadata",
)
