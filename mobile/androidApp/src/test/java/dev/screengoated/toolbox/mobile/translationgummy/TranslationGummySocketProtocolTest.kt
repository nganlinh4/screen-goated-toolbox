package dev.screengoated.toolbox.mobile.translationgummy

import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertFalse
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class TranslationGummySocketProtocolTest {
    private val json = Json { ignoreUnknownKeys = false }

    @Test
    fun `prompt fixture matches mobile system instruction builder`() {
        val fixture = loadPromptFixture()
        val config = TranslationGummyConfig(
            first = TranslationGummyLanguageProfile(
                language = "Korean",
                accent = "Busan",
                tone = "polite",
            ),
            second = TranslationGummyLanguageProfile(
                language = "English",
                tone = "easy to hear",
            ),
        )

        assertEquals("gemini-3.1-flash-live-preview", fixture.model)
        assertEquals(fixture.systemInstructionExample, config.buildSystemInstruction())
        fixture.requiredBehavior.forEach { behavior ->
            assertTrue("fixture behavior should be documented: $behavior", behavior.isNotBlank())
        }
    }

    @Test
    fun `setup payload builder keeps canonical Windows Gemini Live contract fields`() {
        val fixture = loadPromptFixture()
        val payload = json.parseToJsonElement(
            buildTranslationGummySetupPayload(
                model = fixture.model,
                instruction = fixture.systemInstructionExample,
                voiceName = "Aoede",
            ),
        ).jsonObject
        val setup = payload.getValue("setup").jsonObject
        val generation = setup.getValue("generationConfig").jsonObject
        val realtime = setup.getValue("realtimeInputConfig").jsonObject
        val activityDetection = realtime.getValue("automaticActivityDetection").jsonObject

        assertEquals("models/${fixture.model}", setup.getValue("model").jsonPrimitive.content)
        assertEquals("AUDIO", generation.getValue("responseModalities").jsonArray[0].jsonPrimitive.content)
        assertEquals(1, generation.getValue("responseModalities").jsonArray.size)
        assertEquals("minimal", generation.getValue("thinkingConfig").jsonObject.getValue("thinkingLevel").jsonPrimitive.content)
        assertFalse(generation.getValue("thinkingConfig").jsonObject.containsKey("thinkingBudget"))
        assertEquals(
            "Aoede",
            generation
                .getValue("speechConfig")
                .jsonObject
                .getValue("voiceConfig")
                .jsonObject
                .getValue("prebuiltVoiceConfig")
                .jsonObject
                .getValue("voiceName")
                .jsonPrimitive
                .content,
        )
        assertEquals(
            fixture.systemInstructionExample,
            setup
                .getValue("systemInstruction")
                .jsonObject
                .getValue("parts")
                .jsonArray[0]
                .jsonObject
                .getValue("text")
                .jsonPrimitive
                .content,
        )
        assertEquals("START_SENSITIVITY_HIGH", activityDetection.getValue("startOfSpeechSensitivity").jsonPrimitive.content)
        assertEquals("END_SENSITIVITY_HIGH", activityDetection.getValue("endOfSpeechSensitivity").jsonPrimitive.content)
        assertEquals(80, activityDetection.getValue("prefixPaddingMs").jsonPrimitive.int)
        assertEquals(320, activityDetection.getValue("silenceDurationMs").jsonPrimitive.int)
        assertEquals("START_OF_ACTIVITY_INTERRUPTS", realtime.getValue("activityHandling").jsonPrimitive.content)
        assertEquals("TURN_INCLUDES_ONLY_ACTIVITY", realtime.getValue("turnCoverage").jsonPrimitive.content)
        assertTrue(setup.containsKey("inputAudioTranscription"))
        assertTrue(setup.containsKey("outputAudioTranscription"))
    }

    @Test
    fun `Gemini 2_5 setup uses budget while 3_1 uses level`() {
        val instruction = "translate"
        val twoFiveGeneration = json.parseToJsonElement(
            buildTranslationGummySetupPayload(
                "gemini-2.5-flash-native-audio-preview-12-2025",
                instruction,
                "Aoede",
            ),
        ).jsonObject["setup"]!!.jsonObject["generationConfig"]!!.jsonObject
        val threeOneGeneration = json.parseToJsonElement(
            buildTranslationGummySetupPayload("gemini-3.1-flash-live-preview", instruction, "Aoede"),
        ).jsonObject["setup"]!!.jsonObject["generationConfig"]!!.jsonObject
        val twoFive = twoFiveGeneration.getValue("thinkingConfig").jsonObject
        val threeOne = threeOneGeneration.getValue("thinkingConfig").jsonObject

        assertEquals(8192, twoFiveGeneration.getValue("maxOutputTokens").jsonPrimitive.int)
        assertEquals(65536, threeOneGeneration.getValue("maxOutputTokens").jsonPrimitive.int)
        assertEquals(0, twoFive.getValue("thinkingBudget").jsonPrimitive.int)
        assertFalse(twoFive.containsKey("thinkingLevel"))
        assertEquals("minimal", threeOne.getValue("thinkingLevel").jsonPrimitive.content)
        assertFalse(threeOne.containsKey("thinkingBudget"))
    }

    @Test
    fun `socket parser treats setup complete as structural event only`() {
        val fixture = loadSocketFixture()
        val setupCase = fixture.getValue("setupComplete").jsonObject
        assertEquals(
            true,
            setupCase.getValue("expected").jsonObject.getValue("setupComplete").jsonPrimitive.boolean,
        )

        val errorCase = fixture.getValue("errorContainingSetupCompleteText").jsonObject
        assertEquals(
            false,
            errorCase.getValue("expected").jsonObject.getValue("setupComplete").jsonPrimitive.boolean,
        )
        assertTrue(errorCase.getValue("expected").jsonObject.getValue("error").jsonPrimitive.content.contains("setupComplete"))

        val parsedSetup = parseTranslationGummySocketUpdate(
            """{"setupComplete":{}}""",
        )
        val parsedError = parseTranslationGummySocketUpdate(
            """{"error":{"message":"setupComplete failed before session start"}}""",
        )

        assertTrue(parsedSetup.setupComplete)
        assertFalse(parsedError.setupComplete)
        assertEquals("setupComplete failed before session start", parsedError.error)
    }

    @Test
    fun `socket parser preserves every audio part in a combined frame`() {
        val case = loadGeminiLiveSocketFixture().getValue("combinedServerFrame").jsonObject
        val update = parseTranslationGummySocketUpdate(case.getValue("payload").toString())

        assertEquals(
            listOf(
                listOf(1.toByte(), 2.toByte()),
                listOf(3.toByte(), 4.toByte()),
            ),
            update.audioChunks.map(ByteArray::toList),
        )
        assertEquals("source", update.inputTranscript)
        assertEquals("target", update.outputTranscript)
        assertTrue(update.turnComplete)
        assertTrue(update.interrupted)
    }

    @Test
    fun `audio stream end payload matches socket fixture`() {
        val fixture = loadSocketFixture().getValue("audioStreamEnd").jsonObject
        val root = fixture.getValue("expectedRoot").jsonPrimitive.content
        val flag = fixture.getValue("expectedFlag").jsonPrimitive.content
        val payload = json.parseToJsonElement(buildTranslationGummyAudioStreamEndPayload()).jsonObject

        assertTrue(payload.getValue(root).jsonObject.getValue(flag).jsonPrimitive.boolean)
    }

    @Test
    fun `runtime gates capture on readiness and checks every socket send`() {
        val source = loadRuntimeSource()
        val readyIndex = source.indexOf("openGeminiLiveReadySession(httpClient, apiKey, setupPayload).use")
        val captureIndex = source.indexOf("audioCaptureController.open(")

        assertTrue(readyIndex >= 0)
        assertTrue(captureIndex > readyIndex)
        assertTrue(source.contains("is GeminiLiveReceiveResult.Frame -> handleSocketFrame("))
        assertTrue(source.contains("if (!session.trySend(buildTranslationGummyAudioStreamEndPayload()))"))
        assertTrue(source.contains("if (!session.trySend(buildTranslationGummyAudioPayload(chunk)))"))
        assertTrue(source.contains("val streamEndSent = session.trySend(buildTranslationGummyAudioStreamEndPayload())"))
        assertFalse(source.contains("BlockingWebSocketSession"))
        assertFalse(source.contains("waitForSetup("))
    }

    private fun loadPromptFixture(): PromptFixture {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val repoRoot = generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, PROMPT_FIXTURE_PATH).exists()
        } ?: error("Could not locate $PROMPT_FIXTURE_PATH from $workingDirectory")

        return json.decodeFromString(File(repoRoot, PROMPT_FIXTURE_PATH).readText())
    }

    private fun loadSocketFixture(): JsonObject {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val repoRoot = generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, SOCKET_FIXTURE_PATH).exists()
        } ?: error("Could not locate $SOCKET_FIXTURE_PATH from $workingDirectory")

        return json.parseToJsonElement(File(repoRoot, SOCKET_FIXTURE_PATH).readText()).jsonObject
    }

    private fun loadGeminiLiveSocketFixture(): JsonObject {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val repoRoot = generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, GEMINI_LIVE_SOCKET_FIXTURE_PATH).exists()
        } ?: error("Could not locate $GEMINI_LIVE_SOCKET_FIXTURE_PATH from $workingDirectory")

        return json.parseToJsonElement(
            File(repoRoot, GEMINI_LIVE_SOCKET_FIXTURE_PATH).readText(),
        ).jsonObject
    }

    private fun loadRuntimeSource(): String {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val repoRoot = generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, RUNTIME_SOURCE_PATH).exists()
        } ?: error("Could not locate $RUNTIME_SOURCE_PATH from $workingDirectory")

        return File(repoRoot, RUNTIME_SOURCE_PATH).readText()
    }

    private companion object {
        private const val PROMPT_FIXTURE_PATH = "parity-fixtures/translation-gummy/prompt-contract.json"
        private const val SOCKET_FIXTURE_PATH = "parity-fixtures/translation-gummy/socket-protocol.json"
        private const val GEMINI_LIVE_SOCKET_FIXTURE_PATH =
            "parity-fixtures/preset-system/gemini-live-socket-protocol.json"
        private const val RUNTIME_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/translationgummy/TranslationGummyRuntime.kt"
    }
}

@Serializable
private data class PromptFixture(
    val model: String,
    val systemInstructionExample: String,
    val requiredBehavior: List<String>,
)
