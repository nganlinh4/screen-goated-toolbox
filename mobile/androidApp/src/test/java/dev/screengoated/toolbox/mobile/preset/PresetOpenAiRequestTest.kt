package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import okhttp3.RequestBody.Companion.toRequestBody
import okio.Buffer
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class PresetOpenAiRequestTest {
    private val rejection = """{"error":{"code":"rate_limit_exceeded","type":"tokens","message":"(OTPM): Limit 1000, Requested 2048. Reduce the output allowance."}}"""

    @Test
    fun bothModalitiesRetryOnceAndDoNotBenchRequestSpecificRejections() = runBlocking {
        for (type in listOf(PresetModelType.TEXT, PresetModelType.VISION)) {
            val model = PresetModelCatalog.runtimeModels().first { it.provider == PresetModelProvider.GROQ && it.modelType == type }
            for (succeed in listOf(true, false)) {
                val sent = mutableListOf<JSONObject>()
                val client = OkHttpClient.Builder().addInterceptor { chain ->
                    val buffer = Buffer()
                    chain.request().body!!.writeTo(buffer)
                    sent.add(JSONObject(buffer.readUtf8()))
                    val success = succeed && sent.size == 2
                    Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1)
                        .code(if (success) 200 else 429).message("test")
                        .body((if (success) "ready" else rejection).toResponseBody(jsonMediaType)).build()
                }.build()
                val payload = JSONObject().put("max_completion_tokens", 2048)
                val request = Request.Builder().url("https://example.invalid/completions")
                    .post(payload.toString().toRequestBody(jsonMediaType)).build()
                val result = runCatching { client.executePresetOpenAiRequest(request, payload, "Groq", model, false).use { it.body.string() } }
                assertEquals(2, sent.size)
                assertEquals(2048, sent[0].getInt("max_completion_tokens"))
                assertEquals(1000, sent[1].getInt("max_completion_tokens"))
                if (succeed) assertEquals("ready", result.getOrThrow())
                else {
                    val error = result.exceptionOrNull()!!.message!!
                    assertTrue(error.startsWith("PROVIDER_REQUEST_LIMIT:"))
                    recordPresetModelFailureAt(model.id, error, 1_000L)
                    assertNull(claimPresetModelAttemptAt(model.id, 1_000L))
                    recordPresetModelSuccess(model.id)
                }
            }
        }
    }
}
