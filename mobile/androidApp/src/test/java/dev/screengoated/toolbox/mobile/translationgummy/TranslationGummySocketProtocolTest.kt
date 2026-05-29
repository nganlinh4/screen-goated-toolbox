package dev.screengoated.toolbox.mobile.translationgummy

import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
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
        val source = loadSourceFile().readText()

        assertTrue(source.contains(".put(\"responseModalities\", JSONArray().put(\"AUDIO\"))"))
        assertTrue(source.contains(".put(\"thinkingConfig\", JSONObject().put(\"thinkingBudget\", 0))"))
        assertFalse(source.contains("thinkingLevel"))
        assertTrue(source.contains(".put(\"startOfSpeechSensitivity\", \"START_SENSITIVITY_HIGH\")"))
        assertTrue(source.contains(".put(\"endOfSpeechSensitivity\", \"END_SENSITIVITY_HIGH\")"))
        assertTrue(source.contains(".put(\"prefixPaddingMs\", 80)"))
        assertTrue(source.contains(".put(\"silenceDurationMs\", 320)"))
        assertTrue(source.contains(".put(\"activityHandling\", \"START_OF_ACTIVITY_INTERRUPTS\")"))
        assertTrue(source.contains(".put(\"turnCoverage\", \"TURN_INCLUDES_ONLY_ACTIVITY\")"))
        assertTrue(source.contains(".put(\"inputAudioTranscription\", JSONObject())"))
        assertTrue(source.contains(".put(\"outputAudioTranscription\", JSONObject())"))
    }

    @Test
    fun `socket parser treats setup complete as structural event only`() {
        val fixture = loadSocketFixture()
        val source = loadSourceFile().readText()
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
        assertFalse(source.contains("message.contains(\"setupComplete\")"))
        assertTrue(source.contains("val root = JSONObject(message)"))
        assertTrue(source.contains("if (root.has(\"setupComplete\"))"))
    }

    @Test
    fun `audio stream end payload matches socket fixture`() {
        val fixture = loadSocketFixture().getValue("audioStreamEnd").jsonObject
        val root = fixture.getValue("expectedRoot").jsonPrimitive.content
        val flag = fixture.getValue("expectedFlag").jsonPrimitive.content
        val source = loadSourceFile().readText()

        assertTrue(source.contains(".put(\"$root\", JSONObject().put(\"$flag\", true))"))
    }

    private fun loadSourceFile(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.map { root -> File(root, SOURCE_PATH) }
            .firstOrNull(File::exists)
            ?: error("Could not locate $SOURCE_PATH from $workingDirectory")
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

    private companion object {
        private const val PROMPT_FIXTURE_PATH = "parity-fixtures/translation-gummy/prompt-contract.json"
        private const val SOCKET_FIXTURE_PATH = "parity-fixtures/translation-gummy/socket-protocol.json"
        private const val SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/translationgummy/TranslationGummySocketProtocol.kt"
    }
}

@Serializable
private data class PromptFixture(
    val model: String,
    val systemInstructionExample: String,
    val requiredBehavior: List<String>,
)
