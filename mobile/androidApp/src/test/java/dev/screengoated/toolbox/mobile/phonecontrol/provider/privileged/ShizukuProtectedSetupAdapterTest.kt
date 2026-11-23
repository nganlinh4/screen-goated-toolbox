package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupResult
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ShizukuProtectedSetupAdapterTest {
    @Test
    fun `manual setup waits while owned and completes on provider readiness`() = runBlocking {
        var waits = 0
        val result = awaitManualProtectedSetup(
            owns = { true },
            providerReady = { waits == 2 },
            wait = { waits += 1 },
        )
        assertEquals(2, waits)
        assertEquals(PhoneControlProtectedSetupResult.Completed, result)
    }

    @Test
    fun `manual setup does not probe or complete after owner is lost`() = runBlocking {
        var probes = 0
        val beforeProbe = awaitManualProtectedSetup(
            owns = { false },
            providerReady = { probes += 1; true },
            wait = { error("must not wait") },
        )
        assertEquals(0, probes)
        assertEquals(PhoneControlProtectedSetupResult.Failed("checkpoint_owner_lost"), beforeProbe)

        var owned = true
        val duringProbe = awaitManualProtectedSetup(
            owns = { owned },
            providerReady = { owned = false; true },
            wait = { error("must not wait") },
        )
        assertEquals(PhoneControlProtectedSetupResult.Failed("checkpoint_owner_lost"), duringProbe)

        owned = true
        var afterWaitProbes = 0
        val duringWait = awaitManualProtectedSetup(
            owns = { owned },
            providerReady = { afterWaitProbes += 1; false },
            wait = { owned = false },
        )
        assertEquals(1, afterWaitProbes)
        assertEquals(PhoneControlProtectedSetupResult.Failed("checkpoint_owner_lost"), duringWait)
    }

    @Test
    fun `accepts one unique exact one-time code`() {
        assertArrayEquals(
            charArrayOf('1', '2', '3', '4', '5', '6'),
            uniqueAsciiOneTimeCode(
                listOf("Pair device", "123456", "123456", "Not now"),
            ),
        )
    }

    @Test
    fun `rejects ambiguity and lookalike text`() {
        assertNull(uniqueAsciiOneTimeCode(listOf("123456", "654321")))
        assertNull(uniqueAsciiOneTimeCode(listOf("code 123456", "12 34 56")))
        assertNull(uniqueAsciiOneTimeCode(listOf("１２３４５６")))
    }

    @Test
    fun `accepts code only on a structurally identified pairing surface`() {
        assertArrayEquals(
            charArrayOf('1', '2', '3', '4', '5', '6'),
            protectedPairingCode(
                listOf(
                    surfaceValue(null, "l_pairing_six_digit"),
                    surfaceValue("123456", "pairing_code"),
                    surfaceValue("192.0.2.8:37123", "ip_addr"),
                ),
            ),
        )
        assertArrayEquals(
            charArrayOf('6', '5', '4', '3', '2', '1'),
            protectedPairingCode(
                listOf(
                    surfaceValue("654321"),
                    surfaceValue("[2001:db8::1]:37123"),
                ),
            ),
        )
    }

    @Test
    fun `recognizes only structurally complete pairing readiness`() {
        assertTrue(
            protectedPairingSurfaceReady(
                listOf(
                    surfaceValue(null, "l_pairing_six_digit"),
                    surfaceValue(null, "pairing_code"),
                    surfaceValue("192.0.2.8:37123", "ip_addr"),
                ),
            ),
        )
        assertTrue(
            protectedPairingSurfaceReady(
                listOf(
                    surfaceValue("654321"),
                    surfaceValue("[2001:db8::1]:37123"),
                ),
            ),
        )
        assertFalse(protectedPairingSurfaceReady(listOf(surfaceValue("192.0.2.8:37123"))))
        assertFalse(protectedPairingSurfaceReady(listOf(surfaceValue("Developer options"))))
    }

    @Test
    fun `rejects loose digits and editable pairing lookalikes`() {
        assertNull(protectedPairingCode(listOf(surfaceValue("123456"))))
        assertNull(
            protectedPairingCode(
                listOf(
                    surfaceValue("123456", editable = true),
                    surfaceValue("192.0.2.8:37123"),
                ),
            ),
        )
        assertNull(
            protectedPairingCode(
                listOf(
                    surfaceValue("123456"),
                    surfaceValue("999.0.2.8:37123"),
                ),
            ),
        )
    }

    private fun surfaceValue(
        text: String?,
        resource: String? = null,
        editable: Boolean = false,
    ) = ProtectedPairingSurfaceValue(
        text = text,
        resourceId = resource?.let { "com.android.settings:id/$it" },
        editable = editable,
    )
}
