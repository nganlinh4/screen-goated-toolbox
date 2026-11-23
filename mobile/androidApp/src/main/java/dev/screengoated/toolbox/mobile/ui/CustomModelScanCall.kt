package dev.screengoated.toolbox.mobile.ui

import java.io.IOException
import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.Call
import okhttp3.Callback
import okhttp3.Response
import okhttp3.ResponseBody
import okhttp3.ResponseBody.Companion.toResponseBody
import okio.Buffer

internal const val MAXIMUM_SCAN_BODY_BYTES = 10 * 1024 * 1024

internal fun readBoundedScanText(body: ResponseBody, maximum: Int = MAXIMUM_SCAN_BODY_BYTES): String {
    require(maximum >= 0)
    val declared = body.contentLength()
    if (declared < -1 || declared > maximum) throw IOException("Scan response exceeds its size limit")
    val source = body.source()
    val buffer = Buffer()
    while (buffer.size <= maximum) {
        val count = source.read(buffer, minOf(8_192L, maximum.toLong() + 1 - buffer.size))
        if (count == -1L) break
        if (count == 0L) throw IOException("Scan response made no progress")
    }
    if (buffer.size > maximum) throw IOException("Scan response exceeds its size limit")
    if (declared >= 0 && declared != buffer.size) throw IOException("Scan response was incomplete")
    return buffer.readByteArray().toResponseBody(body.contentType()).string()
}

internal suspend fun <T> readScanResponse(call: Call, parse: (Response) -> T): T =
    suspendCancellableCoroutine { continuation ->
        continuation.invokeOnCancellation { call.cancel() }
        val callback = object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                continuation.resumeWith(Result.failure(e))
            }

            override fun onResponse(call: Call, response: Response) {
                if (!continuation.isActive) {
                    response.close()
                    return
                }
                continuation.resumeWith(runCatching { response.use(parse) })
            }
        }
        try { call.enqueue(callback) } catch (error: Exception) { continuation.resumeWith(Result.failure(error)) }
    }
