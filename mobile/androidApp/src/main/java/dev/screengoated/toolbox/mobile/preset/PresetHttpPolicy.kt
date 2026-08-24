package dev.screengoated.toolbox.mobile.preset

import okhttp3.Call
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import org.json.JSONException
import org.json.JSONObject
import okio.Buffer
import java.util.concurrent.TimeUnit

internal fun OkHttpClient.newPresetCall(
    request: Request,
    model: PresetModelDescriptor,
    streamingEnabled: Boolean,
): Call {
    val encodedRequestBytes = runCatching { request.body?.contentLength() ?: 0L }
        .getOrDefault(0L)
        .coerceAtLeast(0L)
    val policy = presetRequestDeadlinePolicy(model, streamingEnabled, encodedRequestBytes)
    val call = newBuilder()
        .readTimeout(policy.readIdleTimeoutMillis, TimeUnit.MILLISECONDS)
        .build()
        .newCall(request)
    policy.wholeCallTimeoutMillis?.let { timeout ->
        call.timeout().timeout(timeout, TimeUnit.MILLISECONDS)
    }
    return call
}

internal data class PresetRequestDeadlinePolicy(
    val readIdleTimeoutMillis: Long,
    val wholeCallTimeoutMillis: Long?,
)

internal fun presetRequestDeadlinePolicy(
    model: PresetModelDescriptor,
    streamingEnabled: Boolean,
    encodedRequestBytes: Long,
): PresetRequestDeadlinePolicy {
    if (streamingEnabled) {
        return PresetRequestDeadlinePolicy(
            readIdleTimeoutMillis = STREAM_PROGRESS_IDLE_TIMEOUT_MILLIS,
            wholeCallTimeoutMillis = null,
        )
    }
    val outputTokens = when (model.modelType) {
        PresetModelType.VISION -> model.visionMaxOutputTokens
            ?.toLong()
            ?: DEFAULT_VISION_OUTPUT_TOKENS
        PresetModelType.TEXT, PresetModelType.AUDIO -> DEFAULT_TEXT_OUTPUT_TOKENS
    }
    val hardTimeout = workloadDerivedTimeoutMillis(encodedRequestBytes, outputTokens)
    return PresetRequestDeadlinePolicy(
        readIdleTimeoutMillis = hardTimeout,
        wholeCallTimeoutMillis = hardTimeout,
    )
}

internal fun workloadDerivedTimeoutMillis(
    encodedRequestBytes: Long,
    outputTokens: Long,
): Long {
    val requestSeconds = encodedRequestBytes
        .coerceAtLeast(0L)
        .saturatingAdd(REQUEST_BYTES_PER_ALLOWANCE_SECOND - 1L) / REQUEST_BYTES_PER_ALLOWANCE_SECOND
    val requestAllowance = requestSeconds
        .saturatingMultiply(1_000L)
        .coerceAtMost(MAXIMUM_REQUEST_ALLOWANCE_MILLIS)
    val outputAllowance = outputTokens
        .coerceAtLeast(0L)
        .saturatingMultiply(1_000L)
        .saturatingAdd(MINIMUM_OUTPUT_TOKENS_PER_SECOND - 1L) / MINIMUM_OUTPUT_TOKENS_PER_SECOND
    return STARTUP_ALLOWANCE_MILLIS
        .saturatingAdd(requestAllowance)
        .saturatingAdd(outputAllowance)
        .coerceIn(MINIMUM_INTERACTIVE_TIMEOUT_MILLIS, MAXIMUM_INTERACTIVE_TIMEOUT_MILLIS)
}

private fun Long.saturatingAdd(other: Long): Long =
    if (this > Long.MAX_VALUE - other) Long.MAX_VALUE else this + other

private fun Long.saturatingMultiply(other: Long): Long =
    if (this == 0L || other == 0L) 0L
    else if (this > Long.MAX_VALUE / other) Long.MAX_VALUE
    else this * other

internal fun Response.providerFailureMessage(subject: String): String {
    val source = body.source()
    val buffer = Buffer()
    while (buffer.size <= MAXIMUM_ERROR_BODY_BYTES) {
        val remaining = MAXIMUM_ERROR_BODY_BYTES + 1L - buffer.size
        if (source.read(buffer, minOf(8_192L, remaining)) == -1L) break
    }
    val bytes = buffer.readByteArray()
    val bodyText = bytes.copyOf(minOf(bytes.size, MAXIMUM_ERROR_BODY_BYTES)).toString(Charsets.UTF_8)
    val detail = try {
        JSONObject(bodyText).optJSONObject("error")?.optString("message")
            ?.takeIf(String::isNotBlank)
            ?: "HTTP $code"
    } catch (_: JSONException) {
        bodyText.trim().take(MAXIMUM_ERROR_TEXT_CHARS).ifBlank { "HTTP $code" }
    }
    val retryAfter = header("retry-after")?.trim()?.take(80)?.takeIf(String::isNotEmpty)
    return buildString {
        append(subject)
        append(" failed with ")
        append(code)
        append(": ")
        append(detail.take(MAXIMUM_ERROR_TEXT_CHARS))
        if (retryAfter != null) append("; retry-after: ").append(retryAfter)
    }
}

private const val STREAM_PROGRESS_IDLE_TIMEOUT_MILLIS = 120_000L
private const val STARTUP_ALLOWANCE_MILLIS = 30_000L
private const val REQUEST_BYTES_PER_ALLOWANCE_SECOND = 16_384L
private const val MAXIMUM_REQUEST_ALLOWANCE_MILLIS = 120_000L
private const val MINIMUM_OUTPUT_TOKENS_PER_SECOND = 16L
private const val DEFAULT_TEXT_OUTPUT_TOKENS = 4_096L
private const val DEFAULT_VISION_OUTPUT_TOKENS = 2_048L
private const val MINIMUM_INTERACTIVE_TIMEOUT_MILLIS = 60_000L
private const val MAXIMUM_INTERACTIVE_TIMEOUT_MILLIS = 900_000L
private const val MAXIMUM_ERROR_BODY_BYTES = 16 * 1024
private const val MAXIMUM_ERROR_TEXT_CHARS = 2_000
