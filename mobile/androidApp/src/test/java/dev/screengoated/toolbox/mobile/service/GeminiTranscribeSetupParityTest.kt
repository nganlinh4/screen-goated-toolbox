package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Test

class GeminiTranscribeSetupParityTest {
    @Test
    fun `live transcription setup matches shared contract`() {
        val fixture = Json.parseToJsonElement(loadFixture().readText()).jsonObject
        val actual = Json.parseToJsonElement(
            GeminiLiveSocketClient(OkHttpClient()).buildSetupPayload(
                GeneratedLiveModelCatalog.GEMINI_TRANSCRIBE_API_MODEL,
            ),
        ).jsonObject

        assertEquals(fixture.getValue("setup"), actual.getValue("setup"))
    }

    private fun loadFixture(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val repoRoot = generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root -> File(root, FIXTURE_PATH).exists() }
            ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
        return File(repoRoot, FIXTURE_PATH)
    }

    private companion object {
        private const val FIXTURE_PATH =
            "parity-fixtures/gemini-transcribe-setup/live-transcribe.json"
    }
}
