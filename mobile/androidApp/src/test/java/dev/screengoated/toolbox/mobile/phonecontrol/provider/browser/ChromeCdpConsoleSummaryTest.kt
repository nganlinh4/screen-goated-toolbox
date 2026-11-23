package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ChromeCdpConsoleSummaryTest {
    @Test
    fun consoleApiArgsBecomeText() {
        val summary = summarizeConsoleEvents(listOf(CONSOLE_CALL)).single().jsonObject
        assertEquals("warning", summary.getValue("level").jsonPrimitive.content)
        assertEquals("loaded 42 [object Object]", summary.getValue("text").jsonPrimitive.content)
        assertFalse(summary.getValue("text_truncated").jsonPrimitive.boolean)
    }

    @Test
    fun logEntryReportsItsOwnLevelTextAndUrl() {
        val entry = event(
            """{"method":"Log.entryAdded","params":{"entry":{"level":"error","text":"blocked","url":"https://a.test/x.js"}}}""",
        )
        val summary = summarizeConsoleEvents(listOf(entry)).single().jsonObject
        assertEquals("error", summary.getValue("level").jsonPrimitive.content)
        assertEquals("blocked", summary.getValue("text").jsonPrimitive.content)
        assertEquals("https://a.test/x.js", summary.getValue("url").jsonPrimitive.content)
    }

    @Test
    fun longConsoleTextIsBounded() {
        val long = event(
            """{"method":"Runtime.consoleAPICalled","params":{"type":"log","args":[{"type":"string","value":"${"x".repeat(2_500)}"}]}}""",
        )
        val summary = summarizeConsoleEvents(listOf(long)).single().jsonObject
        assertEquals(2_000, summary.getValue("text").jsonPrimitive.content.length)
        assertTrue(summary.getValue("text_truncated").jsonPrimitive.boolean)
    }

    @Test
    fun runtimeLifecycleEventsDoNotEvictConsoleOutput() {
        val buffer = CdpEventBuffer()
        buffer.add("Runtime.consoleAPICalled", CONSOLE_CALL)
        repeat(300) {
            buffer.add("Runtime.executionContextCreated", event("""{"method":"Runtime.executionContextCreated","params":{}}"""))
        }
        assertEquals(listOf(CONSOLE_CALL), buffer.console())
    }

    private companion object {
        fun event(raw: String): JsonObject = Json.parseToJsonElement(raw).jsonObject

        val CONSOLE_CALL = event(
            """{"method":"Runtime.consoleAPICalled","params":{"type":"warning","args":[""" +
                """{"type":"string","value":"loaded"},{"type":"number","value":42,"description":"42"},""" +
                """{"type":"object","description":"[object Object]"}]}}""",
        )
    }
}
