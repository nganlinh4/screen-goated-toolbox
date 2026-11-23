package dev.screengoated.toolbox.mobile.ui

import java.nio.file.Files
import java.nio.file.Paths
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.MediaType
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.ResponseBody
import okhttp3.ResponseBody.Companion.toResponseBody
import okio.Buffer
import okio.BufferedSource
import org.junit.Assert.assertEquals
import org.junit.Test

class CustomModelScanResponseLimitTest {
    @Test fun sharedResponseSizeCases() {
        val path = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("preset-system/custom-model-scan-response-limit.json") }.first(Files::exists)
        val fixture = Json.parseToJsonElement(Files.readAllBytes(path).decodeToString()).jsonObject
        assertEquals(fixture.getValue("limit_bytes").jsonPrimitive.int, MAXIMUM_SCAN_BODY_BYTES)
        for (entry in fixture.getValue("cases").jsonArray) {
            val case = entry.jsonObject
            val event = case.getValue("event").jsonPrimitive.content
            val payload = when (event) {
                "exact_limit", "streamed_exact_limit" -> "12345678"
                "declared_oversize", "streamed_oversize" -> "123456789"
                "shorter_than_declared" -> "123"
                else -> error("Unknown scan response case")
            }
            val declared = when (event) {
                "streamed_oversize", "streamed_exact_limit" -> -1L
                "shorter_than_declared" -> 4L
                else -> payload.length.toLong()
            }
            val body = object : ResponseBody() {
                private val buffer = Buffer().writeUtf8(payload)
                override fun contentType(): MediaType? = null
                override fun contentLength(): Long = declared
                override fun source(): BufferedSource = buffer
            }
            val outcome = runCatching { body.use { readBoundedScanText(it, 8) } }
            assertEquals(event, case.getValue("result").jsonPrimitive.content,
                if (outcome.isSuccess) "accepted" else "rejected")
        }
    }

    @Test fun charsetAndByteOrderMarkRemainSupported() {
        val bytes = byteArrayOf(0xff.toByte(), 0xfe.toByte(), 'o'.code.toByte(), 0, 'k'.code.toByte(), 0)
        val body = bytes.toResponseBody("text/plain; charset=utf-8".toMediaType())
        assertEquals("ok", body.use { readBoundedScanText(it, bytes.size) })
    }
}
