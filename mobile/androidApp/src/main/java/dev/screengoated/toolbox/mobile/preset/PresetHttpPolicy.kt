package dev.screengoated.toolbox.mobile.preset

import okhttp3.Call
import okhttp3.EventListener
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okio.Buffer
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException
import java.util.concurrent.TimeUnit

internal fun OkHttpClient.newPresetCall(
    request: Request,
    model: PresetModelDescriptor,
    streamingEnabled: Boolean,
): PresetCall {
    val encodedRequestBytes = runCatching { request.body?.contentLength() ?: 0L }
        .getOrDefault(0L)
        .coerceAtLeast(0L)
    val policy = presetRequestDeadlinePolicy(model, encodedRequestBytes)
    val readTimeout = if (streamingEnabled) {
        minOf(policy.responseStartTimeoutMillis, policy.progressIdleTimeoutMillis)
    } else {
        policy.responseStartTimeoutMillis
    }
    val transportState = PresetTransportState(request.body != null)
    val call = newBuilder()
        .eventListener(transportState)
        .connectTimeout(policy.connectTimeoutMillis, TimeUnit.MILLISECONDS)
        .writeTimeout(policy.sendTimeoutMillis, TimeUnit.MILLISECONDS)
        .readTimeout(readTimeout, TimeUnit.MILLISECONDS)
        .build()
        .newCall(request)
    call.timeout().timeout(policy.attemptTimeoutMillis, TimeUnit.MILLISECONDS)
    return PresetCall(call, transportState)
}

internal class PresetCall(
    private val call: Call,
    private val transportState: PresetTransportState,
) {
    fun execute(): Response = try {
        call.execute()
    } catch (error: IOException) {
        if (transportState.failedBeforeResponse() && !error.isCancellation()) {
            throw IOException(
                "$PROVIDER_TRANSPORT_UNAVAILABLE: ${error.message ?: "transport failed"}",
                error,
            )
        }
        throw error
    }
}

internal class PresetTransportState(
    private val requestHasBody: Boolean,
) : EventListener() {
    @Volatile
    private var phase = PresetTransportPhase.CONNECTING

    override fun requestHeadersStart(call: Call) {
        phase = PresetTransportPhase.SENDING
    }

    override fun requestHeadersEnd(call: Call, request: Request) {
        if (!requestHasBody) phase = PresetTransportPhase.AWAITING_RESPONSE
    }

    override fun requestBodyEnd(call: Call, byteCount: Long) {
        phase = PresetTransportPhase.AWAITING_RESPONSE
    }

    override fun responseHeadersStart(call: Call) {
        phase = PresetTransportPhase.AWAITING_RESPONSE
    }

    fun failedBeforeResponse(): Boolean = phase != PresetTransportPhase.AWAITING_RESPONSE
}

private enum class PresetTransportPhase {
    CONNECTING,
    SENDING,
    AWAITING_RESPONSE,
}

private fun IOException.isCancellation(): Boolean = message.equals("canceled", ignoreCase = true)

internal data class PresetRequestDeadlinePolicy(
    val connectTimeoutMillis: Long,
    val sendTimeoutMillis: Long,
    val responseStartTimeoutMillis: Long,
    val progressIdleTimeoutMillis: Long,
    val attemptTimeoutMillis: Long,
)

internal fun presetRequestDeadlinePolicy(
    model: PresetModelDescriptor,
    encodedRequestBytes: Long,
): PresetRequestDeadlinePolicy {
    val latencyMillis = model.typicalLatencyMs?.toLong() ?: INTERACTIVE_DEFAULT_LATENCY_MILLIS
    val requestAllowance = requestAllowanceMillis(encodedRequestBytes)
    val attemptTimeout = latencyMillis
        .saturatingMultiply(INTERACTIVE_ATTEMPT_LATENCY_MULTIPLIER)
        .saturatingAdd(requestAllowance)
        .coerceIn(MINIMUM_INTERACTIVE_ATTEMPT_MILLIS, MAXIMUM_INTERACTIVE_ATTEMPT_MILLIS)
    return PresetRequestDeadlinePolicy(
        connectTimeoutMillis = minOf(INTERACTIVE_CONNECT_TIMEOUT_MILLIS, attemptTimeout),
        sendTimeoutMillis = INTERACTIVE_SEND_BASE_MILLIS
            .saturatingAdd(requestAllowance)
            .coerceAtMost(MAXIMUM_INTERACTIVE_SEND_TIMEOUT_MILLIS)
            .coerceAtMost(attemptTimeout),
        responseStartTimeoutMillis = latencyMillis
            .saturatingMultiply(INTERACTIVE_RESPONSE_START_LATENCY_MULTIPLIER)
            .saturatingAdd(requestAllowance)
            .coerceIn(MINIMUM_INTERACTIVE_RESPONSE_START_MILLIS, MAXIMUM_INTERACTIVE_RESPONSE_START_MILLIS)
            .coerceAtMost(attemptTimeout),
        progressIdleTimeoutMillis = latencyMillis
            .saturatingMultiply(INTERACTIVE_PROGRESS_IDLE_LATENCY_MULTIPLIER)
            .coerceIn(MINIMUM_INTERACTIVE_PROGRESS_IDLE_MILLIS, MAXIMUM_INTERACTIVE_PROGRESS_IDLE_MILLIS)
            .coerceAtMost(attemptTimeout),
        attemptTimeoutMillis = attemptTimeout,
    )
}

private fun requestAllowanceMillis(encodedRequestBytes: Long): Long = encodedRequestBytes
    .coerceAtLeast(0L)
    .saturatingMultiply(1_000L)
    .saturatingAdd(INTERACTIVE_REQUEST_BYTES_PER_SECOND - 1L)
    .div(INTERACTIVE_REQUEST_BYTES_PER_SECOND)
    .coerceAtMost(MAXIMUM_INTERACTIVE_REQUEST_ALLOWANCE_MILLIS)

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

private const val INTERACTIVE_DEFAULT_LATENCY_MILLIS = 3_000L
private const val INTERACTIVE_REQUEST_BYTES_PER_SECOND = 262_144L
private const val MAXIMUM_INTERACTIVE_REQUEST_ALLOWANCE_MILLIS = 3_000L
private const val INTERACTIVE_RESPONSE_START_LATENCY_MULTIPLIER = 3L
private const val INTERACTIVE_PROGRESS_IDLE_LATENCY_MULTIPLIER = 2L
private const val INTERACTIVE_ATTEMPT_LATENCY_MULTIPLIER = 4L
private const val INTERACTIVE_CONNECT_TIMEOUT_MILLIS = 5_000L
private const val INTERACTIVE_SEND_BASE_MILLIS = 2_000L
private const val MAXIMUM_INTERACTIVE_SEND_TIMEOUT_MILLIS = 10_000L
private const val MINIMUM_INTERACTIVE_RESPONSE_START_MILLIS = 2_500L
private const val MAXIMUM_INTERACTIVE_RESPONSE_START_MILLIS = 15_000L
private const val MINIMUM_INTERACTIVE_PROGRESS_IDLE_MILLIS = 2_000L
private const val MAXIMUM_INTERACTIVE_PROGRESS_IDLE_MILLIS = 8_000L
private const val MINIMUM_INTERACTIVE_ATTEMPT_MILLIS = 5_000L
private const val MAXIMUM_INTERACTIVE_ATTEMPT_MILLIS = 30_000L
internal const val PROVIDER_TRANSPORT_UNAVAILABLE = "PROVIDER_TRANSPORT_UNAVAILABLE"
private const val MAXIMUM_ERROR_BODY_BYTES = 16 * 1024
private const val MAXIMUM_ERROR_TEXT_CHARS = 2_000
