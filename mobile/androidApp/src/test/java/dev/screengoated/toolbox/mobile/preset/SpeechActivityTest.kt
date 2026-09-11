package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.live.SpeechActivity
import java.io.File
import kotlinx.serialization.json.*
import org.junit.Assert.assertEquals
import org.junit.Test

class SpeechActivityTest {
    @Test fun sharedActivityCases() {
        val file = sequenceOf(File("../parity-fixtures/preset-system/microphone-activity.json"), File("../../parity-fixtures/preset-system/microphone-activity.json"))
            .first { it.isFile }
        val root = Json.parseToJsonElement(file.readText()).jsonObject
        for (case in root.getValue("autoStopCases").jsonArray) {
            val value = case.jsonObject
            assertEquals(value.getValue("active").jsonPrimitive.boolean,
                SpeechActivity.autoStopActivity(value.getValue("rms").jsonPrimitive.float))
        }
        for (invalid in listOf(Float.NaN, Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY)) {
            assertEquals(false, SpeechActivity.autoStopActivity(invalid))
        }
        for (case in root.getValue("cases").jsonArray) {
            val state = SpeechActivity()
            for (frame in case.jsonObject.getValue("frames").jsonArray) {
                val value = frame.jsonObject
                assertEquals(value.getValue("active").jsonPrimitive.boolean,
                    state.observe(value.getValue("rms").jsonPrimitive.double, value.getValue("ms").jsonPrimitive.long))
            }
        }
    }
}
