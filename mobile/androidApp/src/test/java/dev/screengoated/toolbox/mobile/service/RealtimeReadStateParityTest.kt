package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.model.withDirectSpeech
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeState
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class RealtimeReadStateParityTest {
    @Test
    fun `read stays enabled after leaving direct speech and receiving transcript updates`() {
        val path = "parity-fixtures/live-translate/read-state.json"
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .first { File(it, path).isFile }
        val fixture = Json.parseToJsonElement(File(root, path).readText()).jsonObject
        for (case in fixture.getValue("cases").jsonArray) {
            val data = case.jsonObject
            var settings = RealtimeTtsSettings(enabled = data.getValue("initialEnabled").jsonPrimitive.boolean)
            for (raw in data.getValue("events").jsonArray) {
                val event = raw.jsonObject
                event["directSpeech"]?.jsonPrimitive?.boolean?.let { settings = settings.withDirectSpeech(it) }
                val expected = event.getValue("expectedEnabled").jsonPrimitive.boolean
                assertEquals(data.getValue("name").toString(), expected, settings.enabled)
                assertEquals(expected, overlayTtsState(settings, TtsRuntimeState()).enabled)
            }
        }
    }
}
