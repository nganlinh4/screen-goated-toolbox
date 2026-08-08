package dev.screengoated.toolbox.mobile.phonecontrol.provider.grounding

import android.content.Context
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.preset.ApiKeys
import dev.screengoated.toolbox.mobile.preset.GeneratedPresetModelCatalogData
import dev.screengoated.toolbox.mobile.preset.VisionApiClient
import kotlinx.coroutines.CancellationException
import org.json.JSONException
import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener

internal data class GroundingCoordinate(
    val id: String?,
    val x: Int,
    val y: Int,
    val label: String,
    val modelId: String,
)

internal data class GroundingVerification(
    val confidence: Int,
    val what: String?,
    val modelId: String,
)

internal sealed interface GroundingClientResult<out T> {
    data class Success<T>(val value: T) : GroundingClientResult<T>

    data class Failure(
        val code: String,
        val message: String,
        val retryable: Boolean,
        val requiredUserStep: String? = null,
        val freshObservationRequired: Boolean = false,
    ) : GroundingClientResult<Nothing>
}

internal class VisualGroundingClient(context: Context) {
    private val container = (context.applicationContext as SgtMobileApplication).appContainer
    private val client = VisionApiClient(container.httpClient)

    suspend fun locate(
        description: String,
        context: String,
        imageBytes: ByteArray,
    ): GroundingClientResult<GroundingCoordinate> = run(
        prompt = pointPrompt(description, context),
        responseSchema = namedPointsSchema(setOf("target")),
        imageBytes = imageBytes,
        parse = { response, model ->
            parseNamedRecords(response, setOf("target"), model)
                ?.singleOrNull()
                ?.let { GroundingClientResult.Success(it) }
                ?: groundingFailure(response, "target", setOf("target"))
        },
    )

    suspend fun map(
        description: String,
        context: String,
        imageBytes: ByteArray,
    ): GroundingClientResult<List<GroundingCoordinate>> = run(
        prompt = marksPrompt(description, context),
        responseSchema = openPointsSchema(),
        imageBytes = imageBytes,
        parse = { response, model ->
            parseOpenRecords(response, model)
                ?.let { GroundingClientResult.Success(it) }
                ?: groundingFailure(response, "targets", emptySet())
        },
    )

    suspend fun drag(
        from: String,
        to: String,
        context: String,
        imageBytes: ByteArray,
    ): GroundingClientResult<Pair<GroundingCoordinate, GroundingCoordinate>> = run(
        prompt = dragPrompt(from, to, context),
        responseSchema = namedPointsSchema(setOf("from", "to")),
        imageBytes = imageBytes,
        parse = { response, model ->
            val points = parseNamedRecords(response, setOf("from", "to"), model)
            val fromPoint = points?.singleOrNull { it.id == "from" }
            val toPoint = points?.singleOrNull { it.id == "to" }
            if (fromPoint != null && toPoint != null &&
                distanceSquared(fromPoint, toPoint) >= MIN_ENDPOINT_DISTANCE_SQUARED
            ) {
                GroundingClientResult.Success(fromPoint to toPoint)
            } else {
                groundingFailure(response, "drag endpoints", setOf("from", "to"))
            }
        },
    )

    suspend fun verify(
        description: String,
        context: String,
        imageBytes: ByteArray,
    ): GroundingClientResult<GroundingVerification> = run(
        prompt = verificationPrompt(description, context),
        responseSchema = verificationSchema(),
        imageBytes = imageBytes,
        parse = { response, model -> parseVerification(response, model) },
    )

    private suspend fun <T> run(
        prompt: String,
        responseSchema: JSONObject,
        imageBytes: ByteArray,
        parse: (String, String) -> GroundingClientResult<T>,
    ): GroundingClientResult<T> {
        val apiKey = container.repository.currentApiKey()
        if (apiKey.isBlank()) {
            return GroundingClientResult.Failure(
                code = "capability_unavailable",
                message = "Vision grounding requires a configured Gemini API key.",
                retryable = true,
                requiredUserStep = "configure_gemini_api_key",
            )
        }
        var lastInvalid: GroundingClientResult.Failure? = null
        for (modelId in GROUNDING_MODEL_IDS) {
            val attempt = try {
                client.executeStreaming(
                    modelId = modelId,
                    prompt = prompt,
                    imageBytes = imageBytes,
                    apiKeys = ApiKeys(geminiKey = apiKey),
                    uiLanguage = "en",
                    onChunk = {},
                    streamingEnabled = false,
                    responseSchema = responseSchema,
                )
            } catch (cancelled: CancellationException) {
                throw cancelled
            }
            val failure = attempt.exceptionOrNull()
            if (failure != null) {
                if (failure is CancellationException) throw failure
                continue
            }
            val response = attempt.getOrThrow()
            if (response.isBlank()) continue
            when (val parsed = parse(response, modelId)) {
                is GroundingClientResult.Success -> return parsed
                is GroundingClientResult.Failure -> {
                    if (parsed.code == "target_not_found" ||
                        parsed.code == "vision_verification_rejected"
                    ) {
                        return parsed
                    }
                    lastInvalid = parsed
                }
            }
        }
        return lastInvalid ?: GroundingClientResult.Failure(
            code = "vision_grounding_failed",
            message = "The vision grounding chain did not return a usable result.",
            retryable = true,
        )
    }
}

internal fun parseNamedRecords(
    response: String,
    expectedIds: Set<String>,
    modelId: String,
): List<GroundingCoordinate>? {
    if (expectedIds.isEmpty() || reportsNotVisible(response, expectedIds)) return null
    val root = parseJsonObject(response, NAMED_ROOT_FIELDS) ?: return null
    val missing = parseStringSet(root.optJSONArray("missing")) ?: return null
    if (missing.isNotEmpty() || !expectedIds.containsAll(missing)) return null
    val records = parsePoints(root.optJSONArray("points"), modelId, named = true) ?: return null
    return records.takeIf { it.mapNotNull(GroundingCoordinate::id).toSet() == expectedIds }
}

internal fun parseOpenRecords(
    response: String,
    modelId: String,
): List<GroundingCoordinate>? {
    val root = parseJsonObject(response, OPEN_ROOT_FIELDS) ?: return null
    val records = parsePoints(root.optJSONArray("points"), modelId, named = false) ?: return null
    if (records.size > MAX_MARKS) return null
    return records.takeIf { points ->
        points.indices.none { left ->
            (left + 1 until points.size).any { right ->
                distanceSquared(points[left], points[right]) < MIN_ENDPOINT_DISTANCE_SQUARED
            }
        }
    }
}

internal fun parseVerification(
    response: String,
    modelId: String,
): GroundingClientResult<GroundingVerification> {
    val value = parseJsonObject(response, VERIFICATION_FIELDS) ?: return invalidVerification()
    val matches = value.opt("matches") as? Boolean ?: return invalidVerification()
    val confidence = value.strictInt("confidence") ?: return invalidVerification()
    val what = (value.opt("what") as? String)
        ?.trim()
        ?.takeIf { it.isNotEmpty() && it.codePointLength() <= MAX_LABEL_CHARS }
        ?: return invalidVerification()
    if (confidence !in 0..100) return invalidVerification()
    if (!matches || confidence < MIN_VERIFICATION_CONFIDENCE) {
        return GroundingClientResult.Failure(
            code = "vision_verification_rejected",
            message = "The proposed point is not confidently inside the requested target.",
            retryable = true,
            freshObservationRequired = true,
        )
    }
    return GroundingClientResult.Success(
        GroundingVerification(
            confidence,
            what,
            modelId,
        ),
    )
}

private fun pointPrompt(description: String, context: String): String =
    """
    ${contextPrefix(context)}Find this visible target: ${JSONObject.quote(description)}
    Output only JSON matching the supplied schema. Put the target in points, or put "target" in missing.
    x and y are integer center coordinates on a 0-1000 grid.
    """.trimIndent()

private fun marksPrompt(description: String, context: String): String =
    """
    ${contextPrefix(context)}Map every distinct visible actionable target relevant to: ${JSONObject.quote(description)}
    Output only JSON matching the supplied schema, in reading order, with at most $MAX_MARKS points.
    x and y are integer center coordinates on a 0-1000 grid. Use an empty points array when none are visible.
    """.trimIndent()

private fun dragPrompt(from: String, to: String, context: String): String =
    """
    ${contextPrefix(context)}Locate both drag endpoints in this same image.
    Start: ${JSONObject.quote(from)}
    Destination: ${JSONObject.quote(to)}
    Output only JSON matching the supplied schema. Put visible endpoints in points and absent IDs in missing.
    Coordinates are integer centers on a 0-1000 grid.
    """.trimIndent()

private fun verificationPrompt(description: String, context: String): String =
    """
    ${contextPrefix(context)}The red crosshair marks a proposed click.
    Requested target: ${JSONObject.quote(description)}
    Return only JSON: {"matches":<boolean>,"confidence":<0-100 integer>,"what":"<short item>"}.
    matches is true only if the crosshair center is visibly inside the target.
    """.trimIndent()

private fun contextPrefix(context: String): String =
    context.trim().takeIf(String::isNotEmpty)?.let {
        "Context (for disambiguation only): " +
            "${JSONObject.quote(it.takeCodePoints(MAX_CONTEXT_CHARS))}\n"
    } ?: ""

private fun parseLabel(value: String): String? =
    value.trim().takeIf { it.isNotEmpty() && it.codePointLength() <= MAX_LABEL_CHARS }

internal fun reportsNotVisible(response: String, expectedIds: Set<String>): Boolean {
    if (expectedIds.isEmpty()) return false
    val root = parseJsonObject(response, NAMED_ROOT_FIELDS) ?: return false
    val missing = parseStringSet(root.optJSONArray("missing")) ?: return false
    val points = parsePoints(root.optJSONArray("points"), "", named = true) ?: return false
    val present = points.mapNotNull(GroundingCoordinate::id).toSet()
    return missing.isNotEmpty() && missing.all { it in expectedIds } &&
        present.intersect(missing).isEmpty() && present + missing == expectedIds
}

private fun parsePoints(array: JSONArray?, modelId: String, named: Boolean): List<GroundingCoordinate>? {
    array ?: return null
    val records = ArrayList<GroundingCoordinate>(array.length())
    repeat(array.length()) { index ->
        val value = array.optJSONObject(index) ?: return null
        val fields = if (named) NAMED_POINT_FIELDS else OPEN_POINT_FIELDS
        if (value.keys().asSequence().toSet() != fields) return null
        val id = if (named) (value.opt("id") as? String)?.takeIf(String::isNotBlank) else null
        if (named && id == null) return null
        val x = value.strictInt("x")?.takeIf { it in 0..1000 } ?: return null
        val y = value.strictInt("y")?.takeIf { it in 0..1000 } ?: return null
        val label = (value.opt("label") as? String)?.let(::parseLabel) ?: return null
        records += GroundingCoordinate(id, x, y, label, modelId)
    }
    return records.takeIf { points ->
        points.mapNotNull(GroundingCoordinate::id).toSet().size == points.size || !named
    }
}

private fun parseStringSet(array: JSONArray?): Set<String>? {
    array ?: return null
    val values = linkedSetOf<String>()
    repeat(array.length()) { index ->
        val value = array.opt(index) as? String ?: return null
        if (value.isBlank() || !values.add(value)) return null
    }
    return values
}

private fun parseJsonObject(response: String, fields: Set<String>): JSONObject? {
    val normalized = normalizeOuterFence(response) ?: return null
    return try {
        val parser = JSONTokener(normalized)
        val value = parser.nextValue() as? JSONObject ?: return null
        if (parser.nextClean() != '\u0000' || value.keys().asSequence().toSet() != fields) null else value
    } catch (_: JSONException) {
        null
    }
}

private fun normalizeOuterFence(response: String): String? {
    val trimmed = response.trim()
    if (!trimmed.startsWith("```")) return trimmed.takeIf { it.isNotEmpty() && "```" !in it }
    val lines = trimmed.lines()
    if (lines.size < 3 || lines.first() !in setOf("```", "```json") || lines.last() != "```") return null
    return lines.subList(1, lines.lastIndex).joinToString("\n").trim().takeIf { "```" !in it }
}

private fun <T> groundingFailure(
    response: String,
    subject: String,
    expectedIds: Set<String>,
): GroundingClientResult<T> =
    if (reportsNotVisible(response, expectedIds)) {
        GroundingClientResult.Failure(
            code = "target_not_found",
            message = "The requested $subject are not visible in the current frame.",
            retryable = false,
        )
    } else {
        GroundingClientResult.Failure(
            code = "vision_grounding_invalid",
            message = "The vision model returned malformed grounding records.",
            retryable = true,
        )
    }

private fun invalidVerification() = GroundingClientResult.Failure(
    code = "vision_verification_invalid",
    message = "The vision model returned malformed target verification.",
    retryable = true,
    freshObservationRequired = true,
)

private fun JSONObject.strictInt(name: String): Int? {
    val number = (opt(name) as? Number)?.toDouble() ?: return null
    return number.takeIf {
        it.isFinite() && it % 1.0 == 0.0 && it in Int.MIN_VALUE.toDouble()..Int.MAX_VALUE.toDouble()
    }?.toInt()
}

private fun String.codePointLength(): Int = codePointCount(0, length)

private fun String.takeCodePoints(maximum: Int): String =
    if (codePointLength() <= maximum) this else substring(0, offsetByCodePoints(0, maximum))

private fun distanceSquared(left: GroundingCoordinate, right: GroundingCoordinate): Int {
    val dx = left.x - right.x
    val dy = left.y - right.y
    return dx * dx + dy * dy
}

private fun namedPointsSchema(ids: Set<String>): JSONObject {
    val idValues = JSONArray().also { values -> ids.forEach(values::put) }
    return objectSchema(
        JSONObject()
            .put("points", arraySchema(pointSchema(idValues)))
            .put("missing", arraySchema(JSONObject().put("type", "string").put("enum", idValues))),
        JSONArray().put("points").put("missing"),
    )
}

private fun openPointsSchema(): JSONObject = objectSchema(
    JSONObject().put("points", arraySchema(pointSchema(null)).put("maxItems", MAX_MARKS)),
    JSONArray().put("points"),
)

private fun pointSchema(ids: JSONArray?): JSONObject {
    val properties = JSONObject()
    if (ids != null) properties.put("id", JSONObject().put("type", "string").put("enum", ids))
    properties
        .put("x", boundedIntegerSchema())
        .put("y", boundedIntegerSchema())
        .put("label", JSONObject().put("type", "string"))
    val required = JSONArray()
    if (ids != null) required.put("id")
    required.put("x").put("y").put("label")
    return objectSchema(properties, required)
}

private fun verificationSchema(): JSONObject = objectSchema(
    JSONObject()
        .put("matches", JSONObject().put("type", "boolean"))
        .put("confidence", integerSchema(100))
        .put("what", JSONObject().put("type", "string")),
    JSONArray().put("matches").put("confidence").put("what"),
)

private fun boundedIntegerSchema(): JSONObject = integerSchema(1000)

private fun integerSchema(maximum: Int): JSONObject = JSONObject()
    .put("type", "integer")
    .put("minimum", 0)
    .put("maximum", maximum)

private fun arraySchema(items: JSONObject): JSONObject = JSONObject()
    .put("type", "array")
    .put("items", items)

private fun objectSchema(properties: JSONObject, required: JSONArray): JSONObject = JSONObject()
    .put("type", "object")
    .put("properties", properties)
    .put("required", required)
    .put("additionalProperties", false)

internal val GROUNDING_MODEL_IDS: List<String> =
    GeneratedPresetModelCatalogData.computerControlGroundingModelChain
private const val MAX_MARKS = 30
private const val MAX_LABEL_CHARS = 160
private const val MAX_CONTEXT_CHARS = 600
private const val MIN_ENDPOINT_DISTANCE_SQUARED = 100
private const val MIN_VERIFICATION_CONFIDENCE = 70
private val VERIFICATION_FIELDS = setOf("matches", "confidence", "what")
private val NAMED_ROOT_FIELDS = setOf("points", "missing")
private val OPEN_ROOT_FIELDS = setOf("points")
private val NAMED_POINT_FIELDS = setOf("id", "x", "y", "label")
private val OPEN_POINT_FIELDS = setOf("x", "y", "label")
