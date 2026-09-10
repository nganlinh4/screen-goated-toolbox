package dev.screengoated.toolbox.mobile.service

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import org.junit.Assert.assertEquals
import org.junit.Test

class GeminiTranscriptionRecoveryTest {
    @Test
    fun `replays canonical transcription recovery contract`() {
        val path = "parity-fixtures/gemini-live-session/transcription-recovery.json"
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .first { File(it, path).isFile }
        val fixture = Json.parseToJsonElement(File(root, path).readText()).jsonObject
        for (case in fixture.getValue("cases").jsonArray) {
            val recovery = GeminiTranscriptionRecovery()
            for (raw in case.jsonObject.getValue("events").jsonArray) {
                val event = raw.jsonObject
                event["audioMs"]?.jsonPrimitive?.longOrNull?.let(recovery::audioSent)
                event["boundaryMs"]?.jsonPrimitive?.longOrNull?.let(recovery::inputFlushed)
                if (event["reset"]?.jsonPrimitive?.booleanOrNull == true ||
                    event["progress"]?.jsonPrimitive?.booleanOrNull == true
                ) recovery.reset()
                event["tickMs"]?.jsonPrimitive?.longOrNull?.let { now ->
                    assertEquals(
                        case.jsonObject.getValue("name").jsonPrimitive.content,
                        event.getValue("expectReconnect").jsonPrimitive.booleanOrNull,
                        recovery.shouldReconnect(now),
                    )
                }
            }
        }
    }
}
