package dev.screengoated.toolbox.mobile.preset

import java.io.IOException
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class VisionImageBudgetTest {
    @Test
    fun limitsMatchWindowsParityFixture() {
        val fixture = Files.readAllBytes(fixturePath()).decodeToString()
        val groq = JSONObject(fixture).getJSONObject("groq")

        assertEquals(3_800_000, groq.getInt("safe_request_bytes"))
        assertEquals(16_384, groq.getInt("json_reserve_bytes"))
        assertEquals(2_500_000, groq.getInt("maximum_encoded_image_bytes"))
        assertEquals(262_144, groq.getInt("minimum_encoded_image_bytes"))
    }

    @Test
    fun budgetLeavesRoomForBase64PromptAndJson() {
        val promptBytes = 32_000
        val imageBytes = groqImageByteBudget(promptBytes)
        val base64Bytes = ((imageBytes + 2) / 3) * 4

        assertTrue(base64Bytes + promptBytes + 16_384 <= 3_800_000)
        assertEquals(2_500_000, imageBytes)
    }

    @Test
    fun oversizedPromptFailsBeforeNetworkRequest() {
        assertThrows(IOException::class.java) {
            groqImageByteBudget(3_800_000)
        }
    }

    private fun fixturePath(): Path {
        return listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "vision-payload.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "vision-payload.json"),
            Paths.get("parity-fixtures", "preset-system", "vision-payload.json"),
        ).firstOrNull(Files::exists)
            ?: error("Unable to locate preset-system/vision-payload.json")
    }
}
