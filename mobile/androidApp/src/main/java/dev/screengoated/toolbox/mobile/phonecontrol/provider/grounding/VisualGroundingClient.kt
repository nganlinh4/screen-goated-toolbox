package dev.screengoated.toolbox.mobile.phonecontrol.provider.grounding

import android.content.Context
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.preset.ApiKeys
import dev.screengoated.toolbox.mobile.preset.GeneratedPresetModelCatalogData
import dev.screengoated.toolbox.mobile.preset.VisionApiClient
import kotlinx.coroutines.CancellationException
import org.json.JSONException
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
        imageBytes = imageBytes,
        parse = { response, model -> parseVerification(response, model) },
    )

    private suspend fun <T> run(
        prompt: String,
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
    val records = strictLines(response)?.map { line ->
        val fields = recordFields(line)
        if (fields.size != 5 || fields[0] != "M" || fields[1] !in expectedIds) return null
        GroundingCoordinate(
            id = fields[1],
            x = parseGrid(fields[2]) ?: return null,
            y = parseGrid(fields[3]) ?: return null,
            label = parseLabel(fields[4]) ?: return null,
            modelId = modelId,
        )
    } ?: return null
    return records.takeIf {
        it.size == expectedIds.size &&
            it.mapNotNull(GroundingCoordinate::id).toSet() == expectedIds
    }
}

internal fun parseOpenRecords(
    response: String,
    modelId: String,
): List<GroundingCoordinate>? {
    if (response.trim() == "N") return emptyList()
    val lines = strictLines(response) ?: return null
    if (lines.size > MAX_MARKS) return null
    val records = lines.map { line ->
        val fields = recordFields(line)
        if (fields.size != 4 || fields[0] != "M") return null
        GroundingCoordinate(
            id = null,
            label = parseLabel(fields[1]) ?: return null,
            x = parseGrid(fields[2]) ?: return null,
            y = parseGrid(fields[3]) ?: return null,
            modelId = modelId,
        )
    }
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
    val trimmed = response.trim()
    val value = try {
        val parser = JSONTokener(trimmed)
        val parsed = parser.nextValue() as? JSONObject ?: return invalidVerification()
        if (parser.nextClean() != '\u0000') return invalidVerification()
        parsed
    } catch (_: JSONException) {
        return invalidVerification()
    }
    if (value.keys().asSequence().toSet() != VERIFICATION_FIELDS) {
        return invalidVerification()
    }
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
    Output exactly one line: M|target|x|y|short visible label
    x and y are integer center coordinates on a 0-1000 grid. If not visible, output N|target.
    Output no markdown or other text.
    """.trimIndent()

private fun marksPrompt(description: String, context: String): String =
    """
    ${contextPrefix(context)}Map every distinct visible actionable target relevant to: ${JSONObject.quote(description)}
    Output only records in reading order, at most $MAX_MARKS: M|short visible label|x|y
    x and y are integer center coordinates on a 0-1000 grid. If none are visible, output N.
    Output no markdown, prose, or duplicate points.
    """.trimIndent()

private fun dragPrompt(from: String, to: String, context: String): String =
    """
    ${contextPrefix(context)}Locate both drag endpoints in this same image.
    Start: ${JSONObject.quote(from)}
    Destination: ${JSONObject.quote(to)}
    Output exactly two lines: M|from|x|y|short visible label and M|to|x|y|short visible label.
    Coordinates are integer centers on a 0-1000 grid. For a missing endpoint output N|from or N|to.
    Output no markdown or other text.
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

private fun strictLines(response: String): List<String>? {
    val trimmed = response.trim()
    if (trimmed.isEmpty() || "```" in trimmed) return null
    return trimmed.lines().map(String::trim).takeIf { it.all(String::isNotEmpty) }
}

private fun recordFields(line: String): List<String> =
    line.split('|').map(String::trim).let { if (it.lastOrNull().isNullOrEmpty()) it.dropLast(1) else it }

private fun parseGrid(value: String): Int? = value.toIntOrNull()?.takeIf { it in 0..1000 }

private fun parseLabel(value: String): String? =
    value.trim().takeIf { it.isNotEmpty() && it.codePointLength() <= MAX_LABEL_CHARS }

internal fun reportsNotVisible(response: String, expectedIds: Set<String>): Boolean {
    val lines = strictLines(response) ?: return false
    if (lines.size == 1) {
        val fields = recordFields(lines.single())
        if (fields == listOf("N")) return true
    }
    if (expectedIds.isEmpty() || lines.size != expectedIds.size) return false
    val missingIds = linkedSetOf<String>()
    val seenIds = linkedSetOf<String>()
    lines.forEach { line ->
        val fields = recordFields(line)
        val id = when {
            fields.size == 2 && fields[0] == "N" && fields[1] in expectedIds ->
                fields[1].also { missingIds += it }
            fields.size == 5 &&
                fields[0] == "M" &&
                fields[1] in expectedIds &&
                parseGrid(fields[2]) != null &&
                parseGrid(fields[3]) != null &&
                parseLabel(fields[4]) != null -> fields[1]
            else -> return false
        }
        if (!seenIds.add(id)) return false
    }
    return missingIds.isNotEmpty() && seenIds == expectedIds
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

internal val GROUNDING_MODEL_IDS: List<String> =
    GeneratedPresetModelCatalogData.computerControlGroundingModelChain
private const val MAX_MARKS = 30
private const val MAX_LABEL_CHARS = 160
private const val MAX_CONTEXT_CHARS = 600
private const val MIN_ENDPOINT_DISTANCE_SQUARED = 100
private const val MIN_VERIFICATION_CONFIDENCE = 70
private val VERIFICATION_FIELDS = setOf("matches", "confidence", "what")
