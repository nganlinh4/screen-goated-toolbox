package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog
import java.io.File
import java.util.concurrent.LinkedBlockingDeque
import kotlinx.coroutines.CompletableDeferred
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
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

        val presetReady = CompletableDeferred<Unit>()
        val presetEvents = LinkedBlockingDeque<GeminiLivePresetEvent>()
        handleGeminiLivePresetMessage(setupCase.getValue("payload").toString(), presetReady, presetEvents)
        handleGeminiLivePresetMessage(errorCase.getValue("payload").toString(), presetReady, presetEvents)
        assertTrue(presetReady.isCompleted)
        assertEquals(
            errorCase.getValue("expected").jsonObject.getValue("error").jsonPrimitive.content,
            (presetEvents.removeFirst() as GeminiLivePresetEvent.Error).message,
        )

        val inputReady = CompletableDeferred<Unit>()
        val inputEvents = LinkedBlockingDeque<GeminiLiveInputEvent>()
        handleGeminiLiveMessage(
            setupCase.getValue("payload").toString(),
            inputReady,
            inputEvents,
            StringBuilder(),
            StringBuilder(),
        ) {}
        handleGeminiLiveMessage(
            errorCase.getValue("payload").toString(),
            inputReady,
            inputEvents,
            StringBuilder(),
            StringBuilder(),
        ) {}
        assertTrue(inputReady.isCompleted)
        assertEquals(
            errorCase.getValue("expected").jsonObject.getValue("error").jsonPrimitive.content,
            (inputEvents.removeFirst() as GeminiLiveInputEvent.Error).message,
        )
    }

    @Test
    fun `preset gemini live audio errors are structural only`() {
        val fixture = loadFixture()
        val transcriptCase = fixture.getValue("transcriptContainingErrorText").jsonObject
        assertEquals(
            "The speaker said quote error quote during the demo.",
            transcriptCase.getValue("expected").jsonObject.getValue("transcript").jsonPrimitive.content,
        )

        val transcript = StringBuilder()
        val finalTranscript = StringBuilder()
        val chunks = mutableListOf<String>()
        val events = LinkedBlockingDeque<GeminiLiveInputEvent>()
        handleGeminiLiveMessage(
            transcriptCase.getValue("payload").toString(),
            CompletableDeferred(),
            events,
            transcript,
            finalTranscript,
            onChunk = chunks::add,
        )

        val expected = transcriptCase.getValue("expected").jsonObject
            .getValue("transcript").jsonPrimitive.content
        assertEquals(expected, transcript.toString())
        assertEquals(expected, finalTranscript.toString())
        assertEquals(listOf(expected), chunks)
        assertEquals(GeminiLiveInputEvent.FinalTranscript, events.removeFirst())
        assertTrue(events.isEmpty())
    }

    @Test
    fun `dedicated interim corrections remain separate from authoritative chunks`() {
        val transcript = StringBuilder()
        val finalTranscript = StringBuilder()
        val chunks = mutableListOf<String>()
        val interims = mutableListOf<String>()
        val events = LinkedBlockingDeque<GeminiLiveInputEvent>()

        handleGeminiLiveMessage(
            """{"serverContent":{"interimInputTranscription":{"text":"meet Tuesday"}}}""",
            CompletableDeferred(),
            events,
            transcript,
            finalTranscript,
            onInterim = interims::add,
            onChunk = chunks::add,
        )
        handleGeminiLiveMessage(
            """{"serverContent":{"inputTranscription":{"text":"Meet Wednesday at 2:00 PM."}}}""",
            CompletableDeferred(),
            events,
            transcript,
            finalTranscript,
            onInterim = interims::add,
            onChunk = chunks::add,
        )

        assertEquals("Meet Wednesday at 2:00 PM.", finalTranscript.toString())
        assertEquals(listOf("meet Tuesday"), interims)
        assertEquals(listOf("Meet Wednesday at 2:00 PM."), chunks)
        assertEquals(GeminiLiveInputEvent.FinalTranscript, events.removeFirst())
    }

    @Test
    fun `live output limits match shared parity fixture`() {
        val limits = loadFixture().getValue("modelOutputLimits").jsonObject
        assertTrue("modelOutputLimits must not be empty", limits.isNotEmpty())

        limits.forEach { (apiModel, expected) ->
            assertEquals(
                "Catalog output limit drifted for $apiModel",
                expected.jsonPrimitive.long,
                GeneratedLiveModelCatalog.maxOutputTokens(apiModel),
            )
        }
    }

    @Test
    fun `dedicated final segments may repeat without being mistaken for cumulative text`() {
        val delivery = dev.screengoated.toolbox.mobile.shared.live.TranscriptionDelivery()
        val transcript = StringBuilder()
        val finalTranscript = StringBuilder()
        val chunks = mutableListOf<String>()
        repeat(2) {
            handleGeminiLiveMessage(
                """{"serverContent":{"inputTranscription":{"text":"hello"}}}""",
                CompletableDeferred(), LinkedBlockingDeque(), transcript, finalTranscript,
                dedicatedTranscribe = true, delivery = delivery, onChunk = chunks::add,
            )
        }
        assertEquals("hello hello", finalTranscript.toString())
        assertEquals(listOf("hello", " hello"), chunks)
    }

    @Test
    fun `streaming typing consumes shared authoritative segment events only`() {
        val delivery = dev.screengoated.toolbox.mobile.shared.live.TranscriptionDelivery()
        val fixturePath = "parity-fixtures/preset-system/streaming-typing.json"
        val fixture = json.parseToJsonElement(File(repoRoot(), fixturePath).readText()).jsonObject
        val transcript = StringBuilder()
        val finalTranscript = StringBuilder()
        val chunks = mutableListOf<String>()
        var display = ""
        for (entry in fixture.getValue("events").jsonArray) {
            val event = entry.jsonObject
            val content = org.json.JSONObject()
            event["interim"]?.jsonPrimitive?.content?.let {
                content.put("interimInputTranscription", org.json.JSONObject().put("text", it))
            }
            event["final"]?.jsonPrimitive?.content?.let {
                content.put("inputTranscription", org.json.JSONObject().put("text", it))
            }
            val countBefore = chunks.size
            handleGeminiLiveMessage(
                org.json.JSONObject().put("serverContent", content).toString(),
                CompletableDeferred(), LinkedBlockingDeque(), transcript, finalTranscript,
                dedicatedTranscribe = true,
                delivery = delivery,
                onInterim = { display = finalTranscript.toString() + it },
                onChunk = { chunks.add(it); display = finalTranscript.toString() },
            )
            assertEquals(event.getValue("typedDelta").jsonPrimitive.content, chunks.drop(countBefore).joinToString(""))
            assertEquals(event.getValue("display").jsonPrimitive.content, display)
        }
        assertEquals(fixture.getValue("finalHistory").jsonPrimitive.content, chunks.joinToString(""))
        assertEquals(chunks.joinToString(""), finalTranscript.toString())
    }

    @Test
    fun `tts model normalization uses catalog default`() {
        listOf("", "gemini", "unknown-live-model").forEach { persisted ->
            assertEquals(
                GeneratedLiveModelCatalog.DEFAULT_TTS_GEMINI_MODEL,
                GeneratedLiveModelCatalog.normalizeTtsGeminiModel(persisted),
            )
        }

        GeneratedLiveModelCatalog.ttsGeminiModels.forEach { option ->
            assertEquals(
                option.apiModel,
                GeneratedLiveModelCatalog.normalizeTtsGeminiModel(option.apiModel),
            )
        }
    }

    private fun loadFixture() =
        json.parseToJsonElement(File(repoRoot(), FIXTURE_PATH).readText()).jsonObject

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
