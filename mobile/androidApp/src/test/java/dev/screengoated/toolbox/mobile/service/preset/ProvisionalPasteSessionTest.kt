package dev.screengoated.toolbox.mobile.service.preset

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ProvisionalPasteSessionTest {
    private class Target(
        override val replaceable: Boolean = true,
        var value: PasteSnapshot = PasteSnapshot("", 0, 0),
    ) : ProvisionalPasteTarget {
        var valid = true
        var writes = 0
        var uncertain = false
        var corruptPostcondition = false
        override fun snapshot(): PasteSnapshot? = value.takeIf { valid }
        override fun replace(expected: PasteSnapshot, replacement: PasteSnapshot): Boolean {
            assertEquals(value, expected)
            writes++
            value = if (corruptPostcondition) replacement.copy(text = replacement.text + "external") else replacement
            return !uncertain
        }
    }

    @Test
    fun `shared Windows provisional replacement contract`() {
        val fixture = Json.parseToJsonElement(repoFile("parity-fixtures/preset-system/provisional-paste.json").readText()).jsonObject
        for (entry in fixture.getValue("cases").jsonArray) {
            val case = entry.jsonObject
            val target = Target(case.getValue("replaceable").jsonPrimitive.boolean)
            val session = ProvisionalPasteSession(target)
            for (item in case.getValue("events").jsonArray) {
                val event = item.jsonObject
                val before = target.value
                val writes = target.writes
                val text = event["text"]?.jsonPrimitive?.content.orEmpty()
                when (event.getValue("kind").jsonPrimitive.content) {
                    "interim" -> session.interim(text)
                    "final" -> session.finalSegment(text)
                    "finish" -> session.close()
                    "lost" -> target.valid = false
                    else -> error("Unknown fixture event")
                }
                val new = event["new"]?.jsonPrimitive?.content
                if (new == null) {
                    assertEquals(case.getValue("name").toString(), writes, target.writes)
                } else {
                    val old = event.getValue("old").jsonPrimitive.content
                    val start = before.end - old.length
                    assertEquals(old, before.text.substring(start, before.end))
                    val result = before.text.substring(0, start) + new + before.text.substring(before.end)
                    assertEquals(PasteSnapshot(result, start + new.length, start + new.length), target.value)
                    assertEquals(writes + 1, target.writes)
                }
            }
        }
    }

    @Test
    fun `middle caret keeps surrounding user text and exact UTF16 selection`() {
        val target = Target(value = PasteSnapshot("prefix🙂suffix", 8, 8))
        val session = ProvisionalPasteSession(target)
        session.interim("café draft")
        session.interim("🙂 revised")
        session.finalSegment("Final.")
        assertEquals(PasteSnapshot("prefix🙂Final.suffix", 14, 14), target.value)
        session.interim(" pending")
        session.close()
        assertEquals(PasteSnapshot("prefix🙂Final.suffix", 14, 14), target.value)
    }

    @Test
    fun `same final commits without another mutation and equal next segment is distinct`() {
        val target = Target()
        val session = ProvisionalPasteSession(target)
        session.interim("yes")
        session.finalSegment("yes")
        assertEquals(1, target.writes)
        session.finalSegment("yes")
        assertEquals("yesyes", target.value.text)
        assertEquals(2, target.writes)
        session.close()
        assertEquals(2, target.writes)
    }

    @Test
    fun `stop rejects interims but drains finals before removing only the remaining tail`() {
        val target = Target()
        val session = ProvisionalPasteSession(target)
        session.interim("draft")
        session.beginDrain()
        assertFalse(session.interim("late hypothesis"))
        session.finalSegment("Final.")
        session.close()
        assertFalse(session.interim("late"))
        assertFalse(session.finalSegment("late"))
        assertEquals("Final.", target.value.text)
        assertEquals(2, target.writes)
    }

    @Test
    fun `abort cleans verified provisional but never allows callbacks to revive it`() {
        val target = Target()
        val session = ProvisionalPasteSession(target)
        session.finalSegment("Saved.")
        session.interim(" draft")
        session.close()
        session.close()
        assertFalse(session.finalSegment("late"))
        assertEquals("Saved.", target.value.text)
        assertEquals(3, target.writes)
    }

    @Test
    fun `focus text or caret loss permanently suspends writes including cleanup`() {
        for (loss in 0..2) {
            val target = Target()
            val session = ProvisionalPasteSession(target)
            session.interim("draft")
            when (loss) {
                0 -> target.valid = false
                1 -> target.value = PasteSnapshot("edited", 6, 6)
                2 -> target.value = target.value.copy(start = 1, end = 1)
            }
            assertFalse(session.finalSegment("Final."))
            target.valid = true
            assertFalse(session.finalSegment("another"))
            session.close()
            assertTrue(session.suspended)
            assertEquals(1, target.writes)
        }
    }

    @Test
    fun `uncertain or transformed write never permits recovery by appending or deleting`() {
        for (transform in listOf(false, true)) {
            val target = Target().apply { uncertain = !transform; corruptPostcondition = transform }
            val session = ProvisionalPasteSession(target)
            assertFalse(session.interim("draft"))
            assertTrue(session.attempted)
            assertTrue(session.suspended)
            assertFalse(session.finalSegment("Final."))
            session.close()
            assertEquals(1, target.writes)
        }
    }

    @Test
    fun `unsupported or selected targets never acquire provisional authority`() {
        val target = Target(replaceable = false)
        val session = ProvisionalPasteSession(target)
        assertFalse(session.interim("draft"))
        assertTrue(session.finalSegment("Final."))
        assertEquals("Final.", target.value.text)
        val selected = Target(value = PasteSnapshot("user text", 0, 4))
        assertFalse(ProvisionalPasteSession(selected).finalSegment("replacement"))
        assertEquals(0, selected.writes)
        assertFalse(ProvisionalPasteSession(null).finalSegment("missing"))
    }

    @Test
    fun `streaming never emits controls or line separators`() {
        assertEquals("a b c d e f ", normalizeStreamingText("a\nb\rc\td\u001be\u2028f\u2029"))
    }

    @Test
    fun `snapshot and replacement bounds match canonical UTF16 limits`() {
        val oversized = "x".repeat(MAX_PASTE_TEXT_UNITS + 1)
        val hugeTarget = Target(value = PasteSnapshot(oversized, oversized.length, oversized.length))
        assertFalse(ProvisionalPasteSession(hugeTarget).finalSegment("x"))
        val target = Target()
        val session = ProvisionalPasteSession(target)
        assertFalse(session.interim("🙂".repeat(MAX_PASTE_INPUT_UNITS / 2 + 1)))
        assertTrue(session.suspended)
        assertEquals(0, target.writes)
        val full = "x".repeat(MAX_PASTE_TEXT_UNITS)
        val fullTarget = Target(value = PasteSnapshot(full, full.length, full.length))
        assertFalse(ProvisionalPasteSession(fullTarget).finalSegment("x"))
        assertEquals(0, fullTarget.writes)
    }

    @Test
    fun `capture binds auto paste before overlay and forwards both provider channels without loose paste`() {
        val capture = repoFile("mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/PresetAudioCaptureSession.kt").readText()
        assertTrue(capture.contains("resolvedPreset.preset.autoPaste && runtimeKind != PresetAudioRuntimeKind.STANDARD"))
        assertTrue(capture.indexOf("AccessibilityProvisionalPasteTarget.capture") < capture.indexOf("showOverlay()"))
        assertTrue(capture.contains("onInterim = { chunk ->"))
        assertTrue(capture.contains("provisionalDelivery?.beginDrain()"))
        assertTrue(capture.contains("onChunk = { chunk -> delivery?.finalSegment(chunk) }"))
        assertTrue(capture.contains("if (captureGeneration == sessionGeneration) destroy()"))
        assertFalse(capture.contains("appendTextToFocusedField("))
        val adapter = repoFile("mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/AccessibilityProvisionalPasteTarget.kt").readText()
        assertFalse(adapter.contains("ACTION_PASTE"))
        assertFalse(adapter.contains("ACTION_FOCUS"))
        assertFalse(adapter.contains("Clipboard"))
        assertTrue(adapter.contains("!node.isPassword"))
        assertTrue(adapter.contains("snapshot() == replacement"))
    }

    private fun repoFile(path: String): File = sequenceOf(File(".."), File("../.."), File("."))
        .map { File(it, path) }.first { it.isFile }
}
