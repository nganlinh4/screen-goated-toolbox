package dev.screengoated.toolbox.mobile.preset

import java.io.File
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class GeminiLiveStillFrameSizeTest {
    @Test
    fun followsTheSharedWindowsFrameBudget() {
        val contract = JSONObject(
            generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
                .map { File(it, "parity-fixtures/gemini-live-session/still-frame.json") }
                .first(File::isFile).readText(),
        )
        assertEquals(contract.getInt("maxLongEdge"), GEMINI_LIVE_STILL_FRAME_MAX_LONG_EDGE)
        val cases = contract.getJSONArray("cases")
        for (index in 0 until cases.length()) {
            val case = cases.getJSONObject(index)
            val input = case.getJSONArray("input")
            val output = case.getJSONArray("output")
            assertEquals(
                case.getString("name"), output.getInt(0) to output.getInt(1),
                geminiLiveStillFrameSize(input.getInt(0), input.getInt(1)),
            )
        }
    }
}
