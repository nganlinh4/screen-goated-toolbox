package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.live.GeminiTranscribeVad
import java.io.File
import java.io.IOException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.double
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class GeminiLiveInputTurnBoundaryTest {
    @Test
    fun `speech ends finalize within the open capture and rearm for the next turn`() {
        var now = 0L
        val events = mutableListOf<String>()
        val boundary = GeminiLiveInputTurnBoundary(true) { now }
        val end = { events.add("end"); true }
        val speech = ShortArray(1600) { 2000 }
        boundary.sendAudio(speech, { events.add("audio"); true }, end)
        now = 180
        boundary.poll(end)
        now = 419
        boundary.poll(end)
        assertEquals(listOf("audio"), events)
        now = 420
        boundary.poll(end)
        repeat(3) { now += 100; boundary.poll(end) }
        assertEquals(listOf("audio", "end"), events)
        boundary.sendAudio(speech, { events.add("audio"); true }, end)
        now += 420
        boundary.sendAudio(ShortArray(1600), { events.add("trailing"); true }, end)
        assertEquals(listOf("audio", "end", "audio", "trailing", "end"), events)
        boundary.cancel()
        now += 1000
        boundary.poll(end)
        assertEquals(2, events.count { it == "end" })
        assertThrows(IOException::class.java) { boundary.sendAudio(speech, { true }, end) }
    }

    @Test
    fun `legacy streaming does not inherit dedicated speech-end signaling`() {
        var now = 0L
        var ends = 0
        val boundary = GeminiLiveInputTurnBoundary(false) { now }
        val end = { ends++; true }
        boundary.sendAudio(ShortArray(1600) { 2000 }, { true }, end)
        now = 1000
        boundary.poll(end)
        assertEquals(0, ends)
        boundary.finish(end)
        assertEquals(1, ends)
        boundary.poll(end)
        assertEquals(1, ends)
    }

    @Test
    fun `rejected audio is not observed and rejected end terminates input`() {
        var now = 0L
        var ends = 0
        val boundary = GeminiLiveInputTurnBoundary(true) { now }
        val end = { ends++; true }
        assertThrows(IOException::class.java) {
            boundary.sendAudio(ShortArray(1600) { 2000 }, { false }, end)
        }
        now = 1000
        boundary.poll(end)
        assertEquals(0, ends)
        boundary.sendAudio(ShortArray(1600) { 2000 }, { true }, end)
        now += 420
        assertThrows(IOException::class.java) { boundary.poll { false } }
        boundary.poll(end)
        assertEquals(0, ends)
    }

    @Test
    fun `shared speech boundary constants and connection reset match Windows`() {
        val fixture = sequenceOf(File(".."), File("../.."), File("."))
            .map { File(it, "parity-fixtures/gemini-transcribe-lifecycle/contract.json") }
            .first { it.isFile }
        val contract = Json.parseToJsonElement(fixture.readText()).jsonObject.getValue("hybridVad").jsonObject
        assertEquals(contract.getValue("speechRms").jsonPrimitive.double, GeminiTranscribeVad.SPEECH_RMS, 0.0)
        assertEquals(contract.getValue("trailingAudioMs").jsonPrimitive.long, GeminiTranscribeVad.TRAILING_AUDIO_MS)
        assertEquals(contract.getValue("endSilenceMs").jsonPrimitive.long, GeminiTranscribeVad.END_SILENCE_MS)
        val vad = GeminiTranscribeVad()
        assertFalse(vad.observeRms(0.1, 0))
        assertFalse(vad.isSafeGap())
        vad.reset()
        assertTrue(vad.isSafeGap())
        assertFalse(vad.pollEnd(1000))
        assertFalse(vad.observeRms(0.1, 1000))
        assertTrue(vad.pollEnd(1420))
        assertFalse(vad.pollEnd(2000))
    }
}
