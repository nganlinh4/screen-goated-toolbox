package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.net.InetAddress
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SgtAdbCommandContractTest {
    @Test
    fun `shell quoting preserves every exact argument as one word`() {
        assertEquals("''", shellQuote(""))
        assertEquals("'plain'", shellQuote("plain"))
        assertEquals("'one'\\''two'", shellQuote("one'two"))

        val script = commandScript(
            program = "/system/bin/printf",
            args = listOf("%s", "value; touch /data/local/tmp/nope", "one'two"),
            cwd = "/data/local/tmp/a b",
            statusMarker = "__STATUS__",
        )

        assertTrue("cd '/data/local/tmp/a b'" in script)
        assertTrue("'/system/bin/printf' '%s'" in script)
        assertTrue("'value; touch /data/local/tmp/nope'" in script)
        assertTrue("'one'\\''two'" in script)
        assertFalse("exec touch" in script)
    }

    @Test
    fun `endpoint must be loopback or an exact current interface address`() {
        val loopback = InetAddress.getByName("127.0.0.1")
        val local = InetAddress.getByName("192.0.2.10")
        val other = InetAddress.getByName("192.0.2.11")

        assertTrue(isLocalEndpointAddress(loopback, emptyList()))
        assertTrue(isLocalEndpointAddress(local, listOf(local)))
        assertFalse(isLocalEndpointAddress(other, listOf(local)))
    }

    @Test
    fun `reconnect accepts only the persisted adb identity family`() {
        val paired = "adb-14141FDF600081-QXjCrW"

        assertTrue(isSgtAdbDeviceIdentity(paired))
        assertTrue(matchesSgtAdbServiceIdentity(paired, paired))
        assertTrue(matchesSgtAdbServiceIdentity("adb-14141FDF600081-TnSdi9", paired))
        assertTrue(matchesSgtAdbServiceIdentity("adb-14141FDF600081", paired))
        assertTrue(
            matchesSgtAdbServiceIdentity(
                "adb-14141FDF600081-QXjCrW",
                "adb-14141FDF600081",
            ),
        )
        assertFalse(matchesSgtAdbServiceIdentity("adb-OTHER-TnSdi9", paired))
        assertFalse(matchesSgtAdbServiceIdentity("studio-device-ABC123", null))
        assertFalse(matchesSgtAdbServiceIdentity("adb-device-ABC123.", null))
    }

    @Test
    fun `dedicated adb process repeats the public exact argv bounds`() {
        assertEquals(
            null,
            validateSgtAdbCommandRequest(
                operationId = "turn:1:job:2",
                program = "/system/bin/printf",
                args = listOf("%s", "ok"),
                cwd = "/data/local/tmp",
                timeoutMs = 60_000,
            ),
        )
        assertTrue(
            validateSgtAdbCommandRequest(
                operationId = "turn:1:job:2",
                program = "/system/bin/printf",
                args = List(17) { "arg" },
                cwd = "/data/local/tmp",
                timeoutMs = 60_000,
            ) != null,
        )
        assertTrue(
            validateSgtAdbCommandRequest(
                operationId = "turn:1:job:2",
                program = "/system/bin/printf",
                args = listOf("x".repeat(4_097)),
                cwd = "/data/local/tmp",
                timeoutMs = 60_000,
            ) != null,
        )
    }

    @Test
    fun `registry follows fixture order and drops unknown transports`() {
        val ordered = PrivilegedCommandProviderRegistry.ordered(
            listOf(
                "sgt_adb_bridge",
                "unknown_future_provider",
                "shizuku_shell",
                "root_bridge",
            ),
        )

        assertEquals(
            listOf("sgt_adb_bridge", "shizuku_shell", "root_bridge"),
            ordered.map(PrivilegedCommandProvider::providerId),
        )
    }

    @Test
    fun `pairing success survives a delayed connect probe`() {
        val pending = SgtAdbCommandBridge.parseProbe(
            """
            {
              "state": "degraded",
              "code": "paired_connect_pending",
              "pairing_established": true,
              "device_identity": "adb-device-ABC123"
            }
            """.trimIndent(),
        )
        val endpointMissing = SgtAdbCommandBridge.parseProbe(
            """
            {
              "state": "needs_user_step",
              "code": "pairing_endpoint_unavailable",
              "pairing_established": false
            }
            """.trimIndent(),
        )

        assertEquals(CapabilityState.DEGRADED, pending.state)
        assertEquals(SgtAdbBridgeCondition.CONNECTING, pending.condition)
        assertTrue(pending.pairingEstablished)
        assertEquals("adb-device-ABC123", pending.deviceIdentity)
        assertTrue(sgtAdbPairingRelayCompleted(pending))
        assertEquals(
            SgtAdbBridgeCondition.WIRELESS_DEBUGGING_UNAVAILABLE,
            endpointMissing.condition,
        )
        assertFalse(endpointMissing.pairingEstablished)
        assertFalse(sgtAdbPairingRelayCompleted(endpointMissing))

        val incomplete = pending.copy(deviceIdentity = null)
        assertFalse(sgtAdbPairingRelayCompleted(incomplete))
    }
}
