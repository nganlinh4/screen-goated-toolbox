package dev.screengoated.toolbox.mobile.service.preset

import java.io.File
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PasteDestinationRoutingTest {
    private fun fixture(): JSONObject {
        val file = generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .map { File(it, "parity-fixtures/preset-system/destination-routing.json") }.first(File::isFile)
        return JSONObject(file.readText())
    }

    @Test fun emptyEditableHasOneCaretButUnknownNonemptySelectionCannotBind() {
        val value = fixture().getJSONObject("emptyEditableCaret")
        assertEquals(PasteSnapshot(value.getString("text"), value.getInt("normalizedStart"), value.getInt("normalizedEnd")),
            editablePasteSnapshot(value.getString("text"), value.getInt("start"), value.getInt("end")))
        assertEquals(null, editablePasteSnapshot("existing", -1, -1))
        assertEquals(null, editablePasteSnapshot("", -1, 0))
    }
    private class Target : ProvisionalPasteTarget {
        override val replaceable = true
        var valid = true
        var value = PasteSnapshot("", 0, 0)
        override fun snapshot() = value.takeIf { valid }
        override fun replace(expected: PasteSnapshot, replacement: PasteSnapshot): Boolean {
            assertEquals(value, expected)
            value = replacement
            return true
        }
    }

    @Test fun sharedDestinationRouting() {
        val cases = fixture().getJSONArray("cases")
        for (index in 0 until cases.length()) {
            val route = PasteDestinationRouting()
            val events = cases.getJSONObject(index).getJSONArray("events")
            for (offset in 0 until events.length()) {
                val event = events.getJSONObject(offset)
                if (event.getString("kind") == "switch") { route.detach(); continue }
                val text = event.getString("text")
                val (tail, boundary) = route.project(text)
                assertEquals(event.toString(), event.getString("tail"), tail)
                route.accept(if (event.getString("kind") == "final") ProvisionalPasteEvent.Final(text)
                    else ProvisionalPasteEvent.Interim(text), boundary, true)
            }
        }
    }

    @Test fun destinationMustSettleAndReceivesOnlyContinuingSpeech() {
        val first = Target()
        val second = Target()
        var time = 0L
        val route = RebindingPasteSession(ProvisionalPasteSession(first), { second }, { time })
        assertTrue(route.deliver(ProvisionalPasteEvent.Interim("first words")))
        first.valid = false
        val next = ProvisionalPasteEvent.Interim("first words continue")
        assertFalse(route.deliver(next))
        time = 499
        assertFalse(route.deliver(next))
        time = 500
        assertTrue(route.deliver(next))
        assertEquals("first words", first.value.text)
        assertEquals(" continue", second.value.text)
        route.close()
        assertFalse(route.deliver(ProvisionalPasteEvent.Final("late")))
    }

    @Test fun preMutationFocusLossRetainsUndeliveredSpeech() {
        val second = Target()
        val first = object : ProvisionalPasteTarget {
            override val replaceable = true
            override fun snapshot() = PasteSnapshot("", 0, 0)
            override fun replace(expected: PasteSnapshot, replacement: PasteSnapshot) = false
            override fun mutate(expected: PasteSnapshot, replacement: PasteSnapshot) = PasteMutationOutcome.NO_EFFECT
        }
        var time = 0L
        val route = RebindingPasteSession(ProvisionalPasteSession(first), { second }, { time })
        val event = ProvisionalPasteEvent.Final("undelivered words")
        assertFalse(route.deliver(event))
        assertFalse(route.attempted)
        assertFalse(route.deliver(event))
        time = 500
        assertTrue(route.deliver(event))
        assertEquals("undelivered words", second.value.text)
    }

    @Test fun uncertainMutationIsConsumedAndOnlyLaterSpeechRebinds() {
        val second = Target()
        val first = object : ProvisionalPasteTarget {
            override val replaceable = true
            override fun snapshot() = PasteSnapshot("", 0, 0)
            override fun replace(expected: PasteSnapshot, replacement: PasteSnapshot) = false
            override fun mutate(expected: PasteSnapshot, replacement: PasteSnapshot) = PasteMutationOutcome.UNCERTAIN
        }
        var time = 0L
        val route = RebindingPasteSession(ProvisionalPasteSession(first), { second }, { time })
        assertTrue(route.deliver(ProvisionalPasteEvent.Interim("uncertain words")))
        assertTrue(route.attempted)
        val next = ProvisionalPasteEvent.Final("uncertain words continue")
        assertFalse(route.deliver(next))
        time = 500
        assertTrue(route.deliver(next))
        assertEquals(" continue", second.value.text)
    }
}
