package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.io.ByteArrayInputStream
import java.io.File
import java.util.Base64
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.*
import org.junit.Test

class CommandOutputEvidenceTest {
    @Test fun sharedCasesPreserveBytesAndNeverClaimTextExactness() {
        val path = generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) { it.parentFile }
            .map { it.resolve("parity-fixtures/phone-control/command-output-encoding.json") }
            .first { it.isFile }
        val fixture = Json.parseToJsonElement(path.readText()).jsonObject
        assertEquals(COMMAND_RAW_PREFIX_BYTES, fixture.getValue("raw_prefix_limit_bytes").jsonPrimitive.int)
        for (case in fixture.getValue("cases").jsonArray.map { it.jsonObject }) {
            val hex = case.getValue("stdout_hex").jsonPrimitive.content
            val bytes = hex.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
            val capture = BoundedCommandOutput(64 * 1024)
            capture.write(bytes, 0, bytes.size)
            val complete = case["source_complete"]?.jsonPrimitive?.boolean ?: true
            val evidence = capture.snapshot().evidence(complete)
            assertEquals(case.getValue("utf8_lossy").jsonPrimitive.boolean, evidence.getValue("output_lossy").jsonPrimitive.boolean)
            assertEquals(case.getValue("text_exact").jsonPrimitive.boolean, evidence.getValue("output_text_exact").jsonPrimitive.boolean)
            assertEquals(case.getValue("raw_complete").jsonPrimitive.boolean, evidence.getValue("output_bytes_complete").jsonPrimitive.boolean)
            assertEquals("unknown_or_mixed", evidence.getValue("output_encoding").jsonPrimitive.content)
            assertArrayEquals(bytes, Base64.getDecoder().decode(evidence.getValue("output_bytes_base64").jsonPrimitive.content))
        }
    }

    @Test fun boundedPrefixAndIncompleteReaderAreNeverReportedComplete() {
        val bytes = ByteArray(COMMAND_RAW_PREFIX_BYTES + 1) { 42 }
        val capture = BoundedCommandOutput(bytes.size)
        capture.write(bytes, 0, bytes.size)
        val complete = capture.snapshot().evidence(true)
        assertFalse(complete.getValue("output_bytes_complete").jsonPrimitive.boolean)
        assertEquals(COMMAND_RAW_PREFIX_BYTES, Base64.getDecoder().decode(complete.getValue("output_bytes_base64").jsonPrimitive.content).size)
        assertFalse(capture.snapshot().evidence(false).getValue("output_complete").jsonPrimitive.boolean)
    }

    @Test fun adbRemovesOnlyProvenMarkerBytesAndNeverClaimsEof() {
        val marker = "__SGT_ADB_RC_abcdef__"
        val original = byteArrayOf(0x61, 0x0a, 0xc7.toByte(), 0xd1.toByte())
        val wrapper = "\n${marker}0\n".toByteArray()
        val output = BoundedAdbOutput()
        assertEquals(0, output.read(ByteArrayInputStream(original + wrapper), marker))
        val evidence = output.evidence(marker, readerTerminated = true, readerSucceeded = true)
        assertArrayEquals(original, Base64.getDecoder().decode(evidence.getValue("output_bytes_base64").jsonPrimitive.content))
        assertTrue(evidence.getValue("output_lossy").jsonPrimitive.boolean)
        assertFalse(evidence.getValue("output_bytes_complete").jsonPrimitive.boolean)
        assertFalse(evidence.getValue("output_complete").jsonPrimitive.boolean)
    }

    @Test fun adbDoesNotInventBytesWithoutUnambiguousWrapperSeparation() {
        val marker = "__SGT_ADB_RC_abcdef__"
        for (bytes in listOf("payload".toByteArray(), "payload\r\n${marker}0\n".toByteArray())) {
            val output = BoundedAdbOutput()
            output.read(ByteArrayInputStream(bytes), marker)
            val evidence = output.evidence(marker, readerTerminated = true, readerSucceeded = true)
            assertFalse(evidence.getValue("output_bytes_available").jsonPrimitive.boolean)
            assertEquals("null", evidence.getValue("output_bytes_base64").toString())
            assertFalse(evidence.getValue("output_bytes_complete").jsonPrimitive.boolean)
        }
    }
}
