package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import okhttp3.Request
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import java.io.IOException
import kotlin.math.ceil
import kotlin.coroutines.coroutineContext

internal suspend fun okhttp3.OkHttpClient.executePresetOpenAiRequest(
    request: Request,
    payload: JSONObject,
    providerName: String,
    model: PresetModelDescriptor,
    streamingEnabled: Boolean,
    allowRateLimitWait: Boolean = false,
): okhttp3.Response {
    var retried = false
    var activeRequest = request
    while (true) {
        coroutineContext.ensureActive()
        val response = newPresetCall(activeRequest, model, streamingEnabled, job = coroutineContext[Job]).execute()
        ModelUsageStats.update(model.provider, model.fullName, response.headers)
        if (response.isSuccessful) return response

        val code = response.code
        val retryAfter = response.header("retry-after")?.trim()?.take(80)
        val retryAfterSeconds = retryAfter
            ?.toDoubleOrNull()
            ?.let(::ceil)
            ?.toLong()
        val body = response.body.string()
        response.close()
        val allowance = if (model.provider == PresetModelProvider.GROQ) {
            groqOutputAllowanceRetry(code, retried, body, (payload.opt("max_completion_tokens") as? Number)?.toLong())
        } else null
        if (allowance != null) {
            payload.put("max_completion_tokens", allowance)
            activeRequest = request.newBuilder().post(payload.toString().toRequestBody("application/json".toMediaType())).build()
            retried = true
            continue
        }
        val retryDelayMillis = groqVisionRetryDelayMillis(
            providerName = providerName,
            statusCode = code,
            alreadyRetried = retried,
            retryAfterSeconds = retryAfterSeconds,
        )
        if (allowRateLimitWait && retryDelayMillis != null) {
            retried = true
            delay(retryDelayMillis)
            continue
        }
        if (code == 401 || code == 403) {
            throw IOException(invalidApiKeyMessage(providerName))
        }
        throw IOException(
            buildString {
                if (model.provider == PresetModelProvider.GROQ) append(groqRequestLimitPrefix(code, body))
                append("$providerName request failed with $code: ")
                append(providerErrorMessage(code, body))
                if (!retryAfter.isNullOrBlank()) append("; retry-after: ").append(retryAfter)
            },
        )
    }
}
