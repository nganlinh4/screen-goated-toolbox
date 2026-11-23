package dev.screengoated.toolbox.mobile.service.tts

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Test
import java.io.File

class AudioTrackPlayerLockingTest {
    @Test
    fun routeFieldsAreReadOncePerTtlAndAfterInvalidation() {
        var reads = 0
        val cache = PlaybackRouteCache(ttlMs = 1_000L) {
            reads += 1
            route(reads)
        }

        val first = cache.get(nowMs = 10_000L)
        assertSame(first, cache.get(nowMs = 10_500L))
        assertSame(first, cache.get(nowMs = 10_999L))
        assertEquals(1, reads)

        assertEquals(route(2), cache.get(nowMs = 11_000L))
        cache.invalidate()
        assertEquals(route(3), cache.get(nowMs = 11_001L))
        // A clock that moved backwards never serves an entry of unknown age.
        assertEquals(route(4), cache.get(nowMs = 5_000L))
        assertEquals(4, reads)
    }

    @Test
    fun pcm16DecodingIsLittleEndianAndIgnoresATrailingByte() {
        val bytes = byteArrayOf(0x01, 0x00, 0xFF.toByte(), 0x7F, 0x00, 0x80.toByte(), 0x05)
        assertArrayEquals(shortArrayOf(1, Short.MAX_VALUE, Short.MIN_VALUE), decodePcm16Le(bytes))
    }

    @Test
    fun noPlaybackWriteHoldsThePlayerMonitorAcrossAWait() {
        val source = File(repoRoot(), PLAYER_PATH).readText()
        assertFalse("blocking writes hold the monitor until the chunk is consumed", source.contains("WRITE_BLOCKING"))
        listOf(
            "fun playPcm24kInterruptibly(",
            "fun finishPcm24kInterruptibly(",
            "fun playNativePcm24kInterruptibly(",
            "fun playNativePcm24k(",
        ).forEach { signature ->
            val preceding = source.substringBefore(signature).trimEnd().lines().last()
            assertFalse("$signature must not be @Synchronized", preceding.contains("@Synchronized"))
        }
        assertFalse("the unused blocking playPcm24k entry point is gone", source.contains("fun playPcm24k("))
    }

    private fun route(seed: Int) = PlaybackRouteInfo(
        audioMode = seed,
        communicationDevice = "device-$seed",
        routedDevice = null,
        voiceCallVolume = seed,
        voiceCallMaxVolume = 15,
        musicVolume = seed,
        musicMaxVolume = 15,
    )

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current -> current.parentFile }
            .firstOrNull { File(it, PLAYER_PATH).exists() }
            ?: error("Could not locate $PLAYER_PATH from $workingDirectory")
    }

    private companion object {
        const val PLAYER_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/tts/AudioTrackPlayer.kt"
    }
}
