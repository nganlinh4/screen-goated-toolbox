package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlOverlayAttachBackoffTest {
    private var clock = 10_000L
    private val backoff = PhoneControlOverlayAttachBackoff { clock }
    private val host = Any()

    @Test
    fun failedAttachIsNotRetriedOnEveryPublish() {
        assertTrue(backoff.mayAttach(host))
        assertEquals(1_000L, backoff.failed(host))

        repeat(50) {
            clock += 10
            assertFalse(backoff.mayAttach(host))
        }
        clock = 11_000L
        assertTrue(backoff.mayAttach(host))
    }

    @Test
    fun repeatedFailuresBackOffExponentiallyUpToTheCap() {
        assertEquals(
            listOf(1_000L, 2_000L, 4_000L, 8_000L, 16_000L, 30_000L, 30_000L),
            (1..7).map { backoff.failed(host) },
        )
    }

    @Test
    fun newAuthorityOrResetClearsTheFailure() {
        backoff.failed(host)
        assertTrue(backoff.mayAttach(Any()))
        backoff.failed(host)
        backoff.reset()
        assertTrue(backoff.mayAttach(host))
        assertEquals(1_000L, backoff.failed(host))
    }

    @Test
    fun controllerGatesAttachAndSchedulesTheRetry() {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) directory = requireNotNull(directory.parentFile)
        val source = File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/overlay/PhoneControlOverlayController.kt",
        ).readText()
        assertTrue(source.contains("orb == null && !attachBackoff.mayAttach(host)"))
        assertTrue(source.contains("mainHandler.postDelayed({ if (!destroyed) render() }, attachBackoff.failed(host))"))
    }
}
