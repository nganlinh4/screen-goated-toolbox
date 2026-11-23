package dev.screengoated.toolbox.mobile.service.tts

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.json.JSONObject
import java.nio.file.Files
import java.nio.file.Paths

class WsolaStretcherTest {
    @Test
    fun sharedStreamFinalizationCases() {
        val path = listOf("../parity-fixtures/live-translate/wsola-finalization.json", "../../parity-fixtures/live-translate/wsola-finalization.json", "parity-fixtures/live-translate/wsola-finalization.json")
            .map(Paths::get).first(Files::exists)
        val fixture = JSONObject(Files.readAllBytes(path).decodeToString())
        fun samples(name: String): ShortArray {
            val array = fixture.getJSONObject(name).getJSONArray("samples")
            return ShortArray(array.length()) { array.getInt(it).toShort() }
        }
        val first = samples("shortStream")
        val second = samples("replacementStream")
        val speed = fixture.getJSONObject("shortStream").getDouble("speed")
        val stretcher = WsolaStretcher(24_000)
        assertTrue(stretcher.stretch(first, speed).isEmpty())
        assertArrayEquals(first, stretcher.finish())
        assertTrue(stretcher.finish().isEmpty())
        assertTrue(stretcher.stretch(first, speed).isEmpty())
        stretcher.reset()
        assertTrue(stretcher.stretch(second, speed).isEmpty())
        assertArrayEquals(second, stretcher.finish())
        assertTrue(stretcher.stretch(first, speed).isEmpty())
        assertArrayEquals(first + second, stretcher.stretch(second, fixture.getDouble("normalSpeed")))
    }

    @Test
    fun shortBufferedAudioFinishesAndNeverLeaksToNextRequest() {
        val stretcher = WsolaStretcher(24_000)
        val first = ShortArray(100) { 1_000 }
        val second = ShortArray(90) { -2_000 }
        assertTrue(stretcher.stretch(first, 0.75).isEmpty())
        assertArrayEquals(first, stretcher.finish())
        assertTrue(stretcher.stretch(second, 0.75).isEmpty())
        assertArrayEquals(second, stretcher.finish())
        assertTrue(stretcher.finish().isEmpty())
    }

    @Test
    fun resetDiscardsAnInterruptedRequestsBufferedSamples() {
        val stretcher = WsolaStretcher(24_000)
        assertTrue(stretcher.stretch(ShortArray(100) { 1_000 }, 0.75).isEmpty())
        stretcher.reset()
        val replacement = ShortArray(100) { -2_000 }
        assertTrue(stretcher.stretch(replacement, 0.75).isEmpty())
        assertArrayEquals(replacement, stretcher.finish())
    }

    @Test
    fun emittedAudioFinalizesItsOverlapAndTailWithoutLongSilence() {
        val stretcher = WsolaStretcher(24_000)
        val emitted = stretcher.stretch(ShortArray(3_000) { 1_234 }, 0.75)
        val tail = stretcher.finish()
        assertTrue(emitted.isNotEmpty())
        assertTrue(tail.isNotEmpty())
        assertTrue(tail.size < 1_000)
        assertTrue(tail.all { it > 0 })
        assertTrue(stretcher.finish().isEmpty())
    }

    @Test
    fun returningToNormalSpeedPreservesTheBufferedPrefix() {
        val stretcher = WsolaStretcher(24_000)
        val prefix = ShortArray(100) { 500 }
        val next = ShortArray(100) { -500 }
        assertTrue(stretcher.stretch(prefix, 0.75).isEmpty())
        assertArrayEquals(prefix + next, stretcher.stretch(next, 1.0))
        assertTrue(stretcher.finish().isEmpty())
    }
}
