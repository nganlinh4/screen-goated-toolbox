package dev.screengoated.toolbox.mobile.preset

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Paths

class GeminiCompletionTest {
    @Test
    fun sharedCompletionContract() {
        val path = listOf("../parity-fixtures", "../../parity-fixtures", "parity-fixtures")
            .map { Paths.get(it, "preset-system", "gemini-completion.json") }.first(Files::exists)
        val fixture = JSONObject(Files.readAllBytes(path).decodeToString())
        for (kind in listOf("stream", "unary")) {
            val cases = fixture.getJSONArray(kind)
            for (index in 0 until cases.length()) {
                val case = cases.getJSONObject(index)
                val result = runCatching {
                    if (kind == "stream") consumeGeminiStream(case.getString("body").lineSequence()) { }
                    else parseGeminiCompletion(case.getJSONObject("body"))
                }
                if (case.has("output")) assertEquals(case.getString("name"), case.getString("output"), result.getOrThrow())
                else {
                    val error = requireNotNull(result.exceptionOrNull()).message.orEmpty()
                    assertTrue("${case.getString("name")}: $error", error.contains(case.getString("error")))
                    assertTrue(shouldAdvanceRetryChain(error))
                }
            }
        }
    }
}
