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
): Call = newCall(request).also { call ->
    benchmarkDerivedTimeoutMillis(model.typicalLatencyMs, streamingEnabled)?.let { timeout ->
        call.timeout().timeout(timeout, TimeUnit.MILLISECONDS)
    }
}

internal fun benchmarkDerivedTimeoutMillis(
    typicalLatencyMs: Int?,
    streamingEnabled: Boolean,
): Long? {
    if (streamingEnabled) return null
    return typicalLatencyMs
        ?.toLong()
        ?.times(INTERACTIVE_TIMEOUT_MULTIPLIER)
        ?.coerceIn(MINIMUM_INTERACTIVE_TIMEOUT_MILLIS, MAXIMUM_INTERACTIVE_TIMEOUT_MILLIS)
        ?: MAXIMUM_INTERACTIVE_TIMEOUT_MILLIS
}

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

private const val INTERACTIVE_TIMEOUT_MULTIPLIER = 10L
private const val MINIMUM_INTERACTIVE_TIMEOUT_MILLIS = 10_000L
private const val MAXIMUM_INTERACTIVE_TIMEOUT_MILLIS = 30_000L
private const val MAXIMUM_ERROR_BODY_BYTES = 16 * 1024
private const val MAXIMUM_ERROR_TEXT_CHARS = 2_000
