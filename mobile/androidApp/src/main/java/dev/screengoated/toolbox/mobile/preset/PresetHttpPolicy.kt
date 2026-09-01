package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.ThreadContextElement
import okhttp3.Call
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okio.Buffer
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.coroutines.AbstractCoroutineContextElement
import kotlin.coroutines.CoroutineContext

internal fun OkHttpClient.newPresetCall(
    request: Request,
    model: PresetModelDescriptor,
    streamingEnabled: Boolean,
): Call {
    val encodedRequestBytes = runCatching { request.body?.contentLength() ?: 0L }
        .getOrDefault(0L)
        .coerceAtLeast(0L)
    val remainingChainMillis = currentPresetChainRemainingMillis()
    if (remainingChainMillis != null && remainingChainMillis <= 0L) {
        throw IOException(INTERACTIVE_CHAIN_TIMEOUT_ERROR)
    }
    val policy = presetRequestDeadlinePolicy(model, encodedRequestBytes)
        .cappedBy(remainingChainMillis)
    val readTimeout = if (streamingEnabled) {
        minOf(policy.responseStartTimeoutMillis, policy.progressIdleTimeoutMillis)
    } else {
        policy.responseStartTimeoutMillis
    }
    val call = newBuilder()
        .connectTimeout(policy.connectTimeoutMillis, TimeUnit.MILLISECONDS)
        .writeTimeout(policy.sendTimeoutMillis, TimeUnit.MILLISECONDS)
        .readTimeout(readTimeout, TimeUnit.MILLISECONDS)
        .build()
        .newCall(request)
    call.timeout().timeout(policy.attemptTimeoutMillis, TimeUnit.MILLISECONDS)
    return call
}

internal data class PresetRequestDeadlinePolicy(
    val connectTimeoutMillis: Long,
    val sendTimeoutMillis: Long,
    val responseStartTimeoutMillis: Long,
    val progressIdleTimeoutMillis: Long,
    val attemptTimeoutMillis: Long,
) {
    fun cappedBy(capMillis: Long?): PresetRequestDeadlinePolicy {
        if (capMillis == null) return this
        val cap = capMillis.coerceAtLeast(1L)
        return copy(
            connectTimeoutMillis = minOf(connectTimeoutMillis, cap),
            sendTimeoutMillis = minOf(sendTimeoutMillis, cap),
            responseStartTimeoutMillis = minOf(responseStartTimeoutMillis, cap),
            progressIdleTimeoutMillis = minOf(progressIdleTimeoutMillis, cap),
            attemptTimeoutMillis = minOf(attemptTimeoutMillis, cap),
        )
    }
}

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

internal fun presetChainTimeoutMillis(
    model: PresetModelDescriptor,
    encodedRequestBytes: Long,
): Long = (model.typicalLatencyMs?.toLong() ?: INTERACTIVE_DEFAULT_LATENCY_MILLIS)
    .saturatingMultiply(INTERACTIVE_CHAIN_LATENCY_MULTIPLIER)
    .saturatingAdd(requestAllowanceMillis(encodedRequestBytes))
    .coerceIn(MINIMUM_INTERACTIVE_CHAIN_MILLIS, MAXIMUM_INTERACTIVE_CHAIN_MILLIS)

private fun requestAllowanceMillis(encodedRequestBytes: Long): Long = encodedRequestBytes
    .coerceAtLeast(0L)
    .saturatingMultiply(1_000L)
    .saturatingAdd(INTERACTIVE_REQUEST_BYTES_PER_SECOND - 1L)
    .div(INTERACTIVE_REQUEST_BYTES_PER_SECOND)
    .coerceAtMost(MAXIMUM_INTERACTIVE_REQUEST_ALLOWANCE_MILLIS)

internal class PresetChainDeadline private constructor(
    private val deadlineNanos: Long,
) : ThreadContextElement<Long?>, AbstractCoroutineContextElement(Key) {
    companion object Key : CoroutineContext.Key<PresetChainDeadline> {
        fun afterMillis(timeoutMillis: Long): PresetChainDeadline = PresetChainDeadline(
            System.nanoTime().saturatingAdd(timeoutMillis.saturatingMultiply(NANOS_PER_MILLISECOND)),
        )
    }

    override fun updateThreadContext(context: CoroutineContext): Long? {
        val previous = presetChainDeadlineNanos.get()
        presetChainDeadlineNanos.set(deadlineNanos)
        return previous
    }

    override fun restoreThreadContext(context: CoroutineContext, oldState: Long?) {
        if (oldState == null) presetChainDeadlineNanos.remove()
        else presetChainDeadlineNanos.set(oldState)
    }
}

private fun currentPresetChainRemainingMillis(): Long? {
    val deadline = presetChainDeadlineNanos.get() ?: return null
    val remainingNanos = deadline - System.nanoTime()
    if (remainingNanos <= 0L) return 0L
    return remainingNanos.saturatingAdd(NANOS_PER_MILLISECOND - 1L) / NANOS_PER_MILLISECOND
}

private val presetChainDeadlineNanos = ThreadLocal<Long?>()

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
private const val INTERACTIVE_CHAIN_LATENCY_MULTIPLIER = 5L
private const val INTERACTIVE_CONNECT_TIMEOUT_MILLIS = 5_000L
private const val INTERACTIVE_SEND_BASE_MILLIS = 2_000L
private const val MAXIMUM_INTERACTIVE_SEND_TIMEOUT_MILLIS = 10_000L
private const val MINIMUM_INTERACTIVE_RESPONSE_START_MILLIS = 2_500L
private const val MAXIMUM_INTERACTIVE_RESPONSE_START_MILLIS = 15_000L
private const val MINIMUM_INTERACTIVE_PROGRESS_IDLE_MILLIS = 2_000L
private const val MAXIMUM_INTERACTIVE_PROGRESS_IDLE_MILLIS = 8_000L
private const val MINIMUM_INTERACTIVE_ATTEMPT_MILLIS = 5_000L
private const val MAXIMUM_INTERACTIVE_ATTEMPT_MILLIS = 30_000L
private const val MINIMUM_INTERACTIVE_CHAIN_MILLIS = 8_000L
private const val MAXIMUM_INTERACTIVE_CHAIN_MILLIS = 30_000L
private const val NANOS_PER_MILLISECOND = 1_000_000L
internal const val INTERACTIVE_CHAIN_TIMEOUT_ERROR = "INTERACTIVE_CHAIN_TIMEOUT"
private const val MAXIMUM_ERROR_BODY_BYTES = 16 * 1024
private const val MAXIMUM_ERROR_TEXT_CHARS = 2_000
