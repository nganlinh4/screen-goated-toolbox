package dev.screengoated.toolbox.mobile.shared.live

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class GeminiLiveProtocolTest {
    @Test
    fun `combined frame matches shared parity fixture`() {
        val root = Json.parseToJsonElement(File(repoRoot(), FIXTURE_PATH).readText()).jsonObject
        val case = root.getValue("combinedServerFrame").jsonObject
        val frame = requireNotNull(
            parseGeminiLiveServerFrame(case.getValue("payload").toString()),
        )
        val expected = case.getValue("expected").jsonObject

        assertEquals(expected.getValue("inputTranscript").jsonPrimitive.content, frame.inputTranscript)
        assertEquals(expected.getValue("outputTranscript").jsonPrimitive.content, frame.outputTranscript)
        assertEquals(
            expected.getValue("audioPartsBase64").jsonArray.map { it.jsonPrimitive.content },
            frame.audioParts.map { it.data },
        )
        assertEquals(
            expected.getValue("visibleTextParts").jsonArray.map { it.jsonPrimitive.content },
            frame.visibleTextParts,
        )
        assertEquals(expected.getValue("responseComplete").jsonPrimitive.boolean, frame.responseComplete)
        assertEquals(expected.getValue("interrupted").jsonPrimitive.boolean, frame.interrupted)
    }

    @Test
    fun `setup completion requires a top-level field`() {
        val setup = requireNotNull(parseGeminiLiveServerFrame("""{"setupComplete":{}}"""))
        val error = requireNotNull(
            parseGeminiLiveServerFrame(
                """{"error":{"message":"setupComplete failed before session start"}}""",
            ),
        )

        assertTrue(setup.setupComplete)
        assertFalse(error.setupComplete)
        assertEquals("setupComplete failed before session start", error.error)
    }

    @Test
    fun `combined server frame preserves every content part and completion flag`() {
        val frame = requireNotNull(
            parseGeminiLiveServerFrame(
                """
                {
                  "serverContent": {
                    "inputTranscription": {"text": "source"},
                    "outputTranscription": {"text": "target"},
                    "modelTurn": {"parts": [
                      {"inlineData": {"mimeType": "audio/pcm;rate=24000", "data": "AQI="}},
                      {"text": "visible"},
                      {"text": "internal", "thought": true},
                      {"inlineData": {"data": "AwQ="}}
                    ]},
                    "turnComplete": true,
                    "interrupted": true
                  }
                }
                """.trimIndent(),
            ),
        )

        assertEquals("source", frame.inputTranscript)
        assertEquals("target", frame.outputTranscript)
        assertEquals(listOf("AQI=", "AwQ="), frame.audioParts.map { it.data })
        assertEquals(listOf("visible"), frame.visibleTextParts)
        assertTrue(frame.responseComplete)
        assertTrue(frame.interrupted)
    }

    @Test
    fun `decodes tool go away resumption and usage protocol fields`() {
        val frame = requireNotNull(
            parseGeminiLiveServerFrame(
                """
                {
                  "toolCall": {"functionCalls": [
                    {"id": "c1", "name": "act", "args": {"x": 1}}
                  ]},
                  "toolCallCancellation": {"ids": ["c1", 2]},
                  "goAway": {"timeLeft": "5.250s"},
                  "sessionResumptionUpdate": {"newHandle": "h1", "resumable": true},
                  "usageMetadata": {"totalTokenCount": 9}
                }
                """.trimIndent(),
            ),
        )

        assertTrue(frame.toolCallPresent)
        assertEquals(listOf("c1"), frame.toolCallIds)
        assertEquals("act", frame.toolCalls.single().name)
        assertEquals(1, frame.toolCalls.single().args.jsonObject.getValue("x").jsonPrimitive.int)
        assertEquals(listOf("c1"), frame.toolCancellationIds)
        assertTrue(frame.goAway)
        assertEquals("5.250s", frame.goAwayTimeLeft)
        assertEquals(5_250L, frame.goAwayTimeLeftMs)
        assertEquals(GeminiLiveSessionResumption("h1", resumable = true), frame.sessionResumption)
        assertEquals(
            9,
            requireNotNull(frame.usageMetadata).jsonObject.getValue("totalTokenCount").jsonPrimitive.int,
        )
        assertTrue(frame.recognized)
    }

    @Test
    fun `recognized presence and protobuf duration parsing are structural`() {
        val serverContent = requireNotNull(parseGeminiLiveServerFrame("""{"serverContent":{}}"""))
        val unknown = requireNotNull(parseGeminiLiveServerFrame("""{"note":"goAway in text"}"""))
        val nonObject = requireNotNull(parseGeminiLiveServerFrame("""[]"""))

        assertTrue(serverContent.serverContentPresent)
        assertTrue(serverContent.hasPostSetupObservation)
        assertTrue(serverContent.recognized)
        assertFalse(unknown.recognized)
        assertFalse(unknown.goAway)
        assertFalse(nonObject.recognized)
        assertEquals(
            2_000L,
            parseGeminiLiveServerFrame("""{"goAway":{"timeLeft":"1.999999999s"}}""")
                ?.goAwayTimeLeftMs,
        )
        assertEquals(
            1L,
            parseGeminiLiveServerFrame("""{"goAway":{"timeLeft":"0.0005s"}}""")
                ?.goAwayTimeLeftMs,
        )
        assertNull(
            parseGeminiLiveServerFrame("""{"goAway":{"timeLeft":"-1s"}}""")
                ?.goAwayTimeLeftMs,
        )
        assertNull(
            parseGeminiLiveServerFrame("""{"goAway":{"timeLeft":"1.0000000000s"}}""")
                ?.goAwayTimeLeftMs,
        )
    }

    @Test
    fun `malformed frames are rejected and string errors are retained`() {
        assertNull(parseGeminiLiveServerFrame("not-json"))
        assertEquals(
            "socket failed",
            parseGeminiLiveServerFrame("""{"error":"socket failed"}""")?.error,
        )
    }

    @Test
    fun `invalid base64 inline data does not count as content activity`() {
        val frame = requireNotNull(
            parseGeminiLiveServerFrame(
                """{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"data":"not-base64!"}},{"inlineData":{"data":"AQI"}}]}}}""",
            ),
        )

        assertTrue(frame.serverContentPresent)
        assertTrue(frame.audioParts.isEmpty())
        assertEquals(0, frame.contentCount)
    }

    @Test
    fun `blank content and null errors normalize without activity`() {
        val blankContent = requireNotNull(
            parseGeminiLiveServerFrame(
                """{"serverContent":{"inputTranscription":{"text":"  "},"modelTurn":{"parts":[{"text":""},{"inlineData":{"data":" "}}]}}}""",
            ),
        )

        assertEquals(0, blankContent.contentCount)
        assertNull(parseGeminiLiveServerFrame("""{"error":null}""")?.error)
        assertNull(parseGeminiLiveServerFrame("""{"error":"  "}""")?.error)
        assertEquals("failed", parseGeminiLiveServerFrame("""{"error":"failed"}""")?.error)
    }

    @Test
    fun `server retryability uses structural code and status`() {
        val unavailable = requireNotNull(
            parseGeminiLiveServerFrame(
                """{"error":{"code":503,"status":"UNAVAILABLE","message":"retry"}}""",
            ),
        )
        val exhausted = requireNotNull(
            parseGeminiLiveServerFrame(
                """{"error":{"status":"RESOURCE_EXHAUSTED","message":"later"}}""",
            ),
        )
        val invalid = requireNotNull(
            parseGeminiLiveServerFrame(
                """{"error":{"code":400,"status":"INVALID_ARGUMENT","message":"bad"}}""",
            ),
        )

        assertTrue(unavailable.errorRetryable)
        assertTrue(exhausted.errorRetryable)
        assertFalse(invalid.errorRetryable)
    }

    @Test
    fun `websocket request encodes the api key as one query parameter`() {
        val request = geminiLiveWebSocketRequest("key+with/symbols=")

        assertEquals(
            "https://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent",
            request.url.newBuilder().query(null).build().toString(),
        )
        assertEquals("key+with/symbols=", request.url.queryParameter("key"))
    }

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root -> File(root, FIXTURE_PATH).exists() }
            ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private companion object {
        private const val FIXTURE_PATH =
            "parity-fixtures/preset-system/gemini-live-socket-protocol.json"
    }
}
