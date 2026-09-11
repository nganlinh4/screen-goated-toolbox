package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.live.TranscriptionDelivery
import java.io.File
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class TranscriptionDeliveryTest {
    @Test fun sharedContract() {
        val file = sequenceOf(File("../parity-fixtures/gemini-transcribe-stream/stabilization.json"), File("../../parity-fixtures/gemini-transcribe-stream/stabilization.json")).first { it.isFile }
        val root = Json.parseToJsonElement(file.readText()).jsonObject
        assertEquals(root.getValue("revisionWords").jsonPrimitive.int, TranscriptionDelivery.REVISION_WORDS)
        assertEquals(root.getValue("revisionScalars").jsonPrimitive.int, TranscriptionDelivery.REVISION_SCALARS)
        for (case in root.getValue("cases").jsonArray) {
            val state = TranscriptionDelivery()
            for (event in case.jsonObject.getValue("events").jsonArray) {
                val value = event.jsonObject
                val final = value["final"]?.jsonPrimitive?.content
                state.update(final ?: value.getValue("interim").jsonPrimitive.content, final != null)
                assertEquals(value.getValue("display").jsonPrimitive.content, state.display())
            }
        }
    }

    @Test fun stopPreservesDeliveredText() {
        val state = TranscriptionDelivery()
        state.update("still speaking", false)
        assertEquals("still speaking", state.finishPending())
        assertEquals("still speaking", state.committed)
        assertEquals("", state.finishPending())
    }

    @Test fun frozenTextKeepsNewTailAndCombiningClusters() {
        val state = TranscriptionDelivery()
        val old = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu"
        state.update(old, false)
        state.update(old.replace("alpha", "ALPHA") + " nu", true)
        assertTrue(state.committed.startsWith("alpha beta"))
        assertTrue(state.committed.endsWith("mu nu"))
        val unicode = TranscriptionDelivery()
        unicode.update("a\u0301".repeat(100), false)
        unicode.update("b".repeat(200), true)
        assertTrue(unicode.committed.startsWith("a\u0301".repeat(68)))
    }
}
