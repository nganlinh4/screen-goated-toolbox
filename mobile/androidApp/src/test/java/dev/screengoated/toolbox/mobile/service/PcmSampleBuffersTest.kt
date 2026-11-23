package dev.screengoated.toolbox.mobile.service

import java.io.File
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PcmSampleBuffersTest {
    @Test
    fun shortBufferGrowsWithoutLosingSamples() {
        val buffer = ShortSampleBuffer(initialCapacity = 2)
        buffer.add(1)
        buffer.addAll(shortArrayOf(2, 3, 4, 5))
        buffer.add(6)
        assertArrayEquals(shortArrayOf(1, 2, 3, 4, 5, 6), buffer.toShortArray())
        assertEquals(6, buffer.size)
    }

    @Test
    fun floatBatchWaitsForMinimumAndKeepsOnlyTheNewestMaximum() {
        val batch = PcmFloatBatch(minSamples = 4, maxSamples = 6)
        batch.append(shortArrayOf(16_384, 16_384, 16_384))
        assertNull(batch.take())

        batch.append(shortArrayOf(Short.MIN_VALUE, 0, 8_192, 8_192, 8_192))
        val taken = requireNotNull(batch.take())
        assertEquals(2, taken.droppedSamples)
        assertArrayEquals(floatArrayOf(0.5f, -1f, 0f, 0.25f, 0.25f, 0.25f), taken.samples, 0f)
        assertNull(batch.take())
    }

    @Test
    fun floatBufferDropsFromTheFront() {
        val buffer = FloatSampleBuffer(initialCapacity = 1)
        (1..5).forEach { buffer.add(it.toFloat()) }
        buffer.dropFirst(2)
        assertEquals(3, buffer.size)
        assertEquals(3f, buffer[0], 0f)
        assertEquals(5f, buffer[2], 0f)
        buffer.dropFirst(10)
        assertTrue(buffer.isEmpty())
    }

    @Test
    fun hotPathsNoLongerBoxSamplesAndDecoderSetupReleasesTheCodec() {
        var root = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(root, ".claude/parity").exists()) root = requireNotNull(root.parentFile)
        val base = "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service"
        val boxed = Regex("""(ArrayList|mutableListOf|MutableList)<(Short|Float|Byte)>""")
        listOf("tts/Mp3Decoder.kt", "tts/WsolaStretcher.kt").forEach { name ->
            assertFalse(name, boxed.containsMatchIn(File(root, "$base/$name").readText()))
        }
        val decoder = File(root, "$base/tts/Mp3Decoder.kt").readText()
        val setupFailure = decoder.substringAfter("} catch (_: Throwable) {").substringBefore("return ByteArray(0)")
        assertTrue(setupFailure.contains("createdDecoder?.let { codec -> runCatching { codec.release() } }"))
        assertTrue(setupFailure.contains("extractor.release()"))
    }
}
