package dev.screengoated.toolbox.mobile.phonecontrol

import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlExternalGoalRegistryTest {
    @Test
    fun `slot keeps one transient bounded goal`() {
        var now = 10L
        val slot = PhoneControlExternalGoalSlot(now = { now }, maximumAgeMs = 50L)

        assertTrue(slot.offer("  first  "))
        assertEquals("first", slot.peek())
        assertFalse(slot.offer("second"))
        assertFalse(slot.complete("different"))
        assertTrue(slot.complete("first"))
        assertNull(slot.peek())
        assertFalse(slot.offer("x".repeat(1_025)))

        assertTrue(slot.offer("expires"))
        now = 61L
        assertNull(slot.peek())
    }

    @Test
    fun `shared prompt limit counts code points like Windows`() {
        val fixture = Json.parseToJsonElement(
            Files.readAllBytes(fixturePath()).decodeToString(),
        ).jsonObject
        val limit = fixture.getValue("maximumPromptCharacters").jsonPrimitive.int
        assertEquals(limit, PhoneControlExternalGoalSlot.MAXIMUM_CHARS)

        // Each emoji is one code point but two UTF-16 units.
        val emoji = "😀"
        val accepted = PhoneControlExternalGoalSlot(now = { 0L })
        assertTrue(accepted.offer(emoji.repeat(limit)))
        val rejected = PhoneControlExternalGoalSlot(now = { 0L })
        assertFalse(rejected.offer(emoji.repeat(limit + 1)))
    }

    private fun fixturePath(): Path {
        val relative = Paths.get("parity-fixtures", "phone-control", "external-control.json")
        val candidates = listOf(
            Paths.get("..").resolve(relative),
            Paths.get("..", "..").resolve(relative),
            relative,
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate external-control parity fixture.")
    }
}
