package dev.screengoated.toolbox.mobile.translationgummy

import java.io.File
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.float
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class TranslationGummyMeterTest {
    private fun fixture() = sequenceOf("", "../", "../../")
        .map { File("${it}parity-fixtures/translation-gummy/meter-delivery.json") }
        .first(File::isFile).readText().let { Json.parseToJsonElement(it).jsonObject }

    @Test
    fun meterBurstsDoNotRepublishFullStateOrCopyTranscriptHistory() = runTest {
        val initial = TranslationGummyState(isRunning = true,
            transcripts = List(200) { index -> TranslationGummyTranscriptItem(
                id = index.toLong(), role = TranslationGummyTranscriptRole.INPUT,
                text = "entry-$index", isFinal = true, updatedAtMs = 0,
            ) })
        val presentation = TranslationGummyPresentation(initial)
        var stateEmissions = 0
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            presentation.state.collect { stateEmissions++ }
        }
        repeat(1000) { presentation.updateVisualizerLevel(it / 1000f) }
        assertEquals(1, stateEmissions)
        assertSame(initial, presentation.state.value)
        assertSame(initial.transcripts, presentation.state.value.transcripts)
    }

    @Test
    fun sampledMeterDeliversLatestWithinBoundAndStopClearsIt() = runTest {
        val fixture = fixture()
        assertEquals(fixture.getValue("frameIntervalMs").jsonPrimitive.long, GUMMY_METER_FRAME_MS)
        val presentation = TranslationGummyPresentation(TranslationGummyState(isRunning = true))
        val values = mutableListOf<Float>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            presentation.visualizerLevels.collect { values += it }
        }
        runCurrent()
        repeat(1000) { index ->
            presentation.updateVisualizerLevel(index / 1000f)
            advanceTimeBy(1)
            runCurrent()
        }
        assertTrue(values.isNotEmpty())
        assertTrue(values.size <= fixture.getValue("maxDeliveriesPerSecond").jsonPrimitive.long)
        assertEquals(0.999f, values.last(), 0.0001f)
        presentation.mutableState.value = presentation.state.value.copy(isRunning = false)
        presentation.resetMeter()
        presentation.updateVisualizerLevel(1f)
        advanceTimeBy(GUMMY_METER_FRAME_MS)
        runCurrent()
        assertEquals(0f, values.last(), 0f)
    }

    @Test
    fun normalizationMatchesSharedFiniteRangeAndStoppedCases() {
        for (entry in fixture().getValue("normalizationCases").jsonArray.map { it.jsonObject }) {
            val value = when (entry["valueKind"]?.jsonPrimitive?.content) {
                "nan" -> Float.NaN
                "infinity" -> Float.POSITIVE_INFINITY
                else -> entry.getValue("value").jsonPrimitive.float
            }
            assertEquals(entry.getValue("expected").jsonPrimitive.float,
                normalizedGummyMeterLevel(entry.getValue("running").jsonPrimitive.boolean, value), 0.0001f)
        }
    }
}
