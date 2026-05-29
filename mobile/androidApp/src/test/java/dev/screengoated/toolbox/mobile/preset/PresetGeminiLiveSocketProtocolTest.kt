package dev.screengoated.toolbox.mobile.preset

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertFalse
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetGeminiLiveSocketProtocolTest {
    private val json = Json

    @Test
    fun `preset gemini live setup complete is structural only`() {
        val fixture = loadFixture()
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

        val textClientSource = loadSource("GeminiLivePresetClient.kt").readText()
        val audioClientSource = loadSource("AudioApiClientGemini.kt").readText()
        assertFalse(textClientSource.contains("message.contains(\"setupComplete\")"))
        assertFalse(audioClientSource.contains("message.contains(\"setupComplete\")"))
        assertTrue(textClientSource.contains("if (root.has(\"setupComplete\"))"))
        assertTrue(audioClientSource.contains("if (root.has(\"setupComplete\"))"))
    }

    @Test
    fun `preset gemini live audio errors are structural only`() {
        val fixture = loadFixture()
        val transcriptCase = fixture.getValue("transcriptContainingErrorText").jsonObject
        assertEquals(
            "The speaker said quote error quote during the demo.",
            transcriptCase.getValue("expected").jsonObject.getValue("transcript").jsonPrimitive.content,
        )

        val audioClientSource = loadSource("AudioApiClientGemini.kt").readText()
        assertFalse(audioClientSource.contains("message.contains(\"\\\"error\\\"\")"))
        assertTrue(audioClientSource.contains("root.optJSONObject(\"error\")"))
        assertTrue(audioClientSource.contains("root.optString(\"error\")"))
        assertTrue(audioClientSource.contains("extractGeminiLiveInputTranscript(root)"))
    }

    private fun loadFixture() =
        json.parseToJsonElement(File(repoRoot(), FIXTURE_PATH).readText()).jsonObject

    private fun loadSource(fileName: String): File =
        File(repoRoot(), "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset/$fileName")

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/preset-system/gemini-live-socket-protocol.json"
    }
}
