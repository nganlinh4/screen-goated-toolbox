package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ShizukuProtectedSetupAdapterTest {
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
