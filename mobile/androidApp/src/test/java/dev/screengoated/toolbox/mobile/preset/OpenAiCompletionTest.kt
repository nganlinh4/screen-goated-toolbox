package dev.screengoated.toolbox.mobile.preset

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class OpenAiCompletionTest {
    @Test
    fun sharedCompletionContract() {
        val fixture = JSONObject(File("../../parity-fixtures/preset-system/chat-completion.json").readText())
        for (kind in listOf("stream", "unary")) {
            val cases = fixture.getJSONArray(kind)
            for (index in 0 until cases.length()) {
                val case = cases.getJSONObject(index)
                val result = runCatching {
                    if (kind == "stream") {
                        consumeOpenAiCompletion(case.getString("body").reader().buffered(), {}, {})
                    } else {
                        parseOpenAiCompletion(case.getJSONObject("body").toString())
                    }
                }
                if (case.has("output")) {
                    assertEquals(case.getString("name"), case.getString("output"), result.getOrThrow())
                } else {
                    assertTrue(case.getString("name"), shouldAdvanceRetryChain(result.exceptionOrNull()?.message.orEmpty()))
                    assertTrue(case.getString("name"), result.exceptionOrNull()?.message?.contains(case.getString("error")) == true)
                }
            }
        }
    }
}
