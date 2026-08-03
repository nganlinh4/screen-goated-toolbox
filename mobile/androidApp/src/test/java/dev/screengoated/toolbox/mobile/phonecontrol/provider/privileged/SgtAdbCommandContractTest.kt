package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import java.io.ByteArrayInputStream
import java.io.InputStream
import java.net.InetAddress
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SgtAdbCommandContractTest {
    @Test
    fun `pairing stages consume one monotonic deadline`() {
        val deadline = monotonicDeadline(timeoutMs = 10_000, nowMs = 2_000)

        assertEquals(12_000, deadline)
        assertEquals(7_500, remainingTimeMs(deadline, nowMs = 4_500))
        assertEquals(0, remainingTimeMs(deadline, nowMs = 13_000))
    }

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
    fun `complete status marker ends a command without waiting for transport eof`() {
        val marker = "__SGT_ADB_RC_0123456789abcdef__"
        val input = ChunkedNonTerminatingInput(
            listOf(
                "payload\n$marker",
                "12",
                "5",
                "\n",
            ),
        )
        val output = BoundedAdbOutput()

        assertEquals(125, output.read(input, marker))
        assertEquals("payload", output.visibleText())
        assertFalse(input.readPastTerminal)
    }

    @Test
    fun `stream eof without the exact complete status line is not terminal success`() {
        val marker = "__SGT_ADB_RC_0123456789abcdef__"
        val output = BoundedAdbOutput()

        assertEquals(
            null,
            output.read(
                ByteArrayInputStream("payload\n${marker}12".encodeToByteArray()),
                marker,
            ),
        )
        assertEquals(null, output.exitCode(marker))
        val embedded = BoundedAdbOutput()
        assertEquals(
            null,
            embedded.read(
                ByteArrayInputStream("payload${marker}0\n".encodeToByteArray()),
                marker,
            ),
        )
    }

    @Test
    fun `adb authority requires a successful non interrupted terminal receipt`() {
        val complete = adbAuthorityReceipt()
        val timedOut = adbAuthorityReceipt(timedOut = true, ok = true)
        val cancelled = adbAuthorityReceipt(cancelled = true, ok = true)

        assertTrue(isVerifiedAdbShellReceipt(complete))
        assertFalse(isVerifiedAdbShellReceipt(timedOut))
        assertFalse(isVerifiedAdbShellReceipt(cancelled))
        assertFalse(isVerifiedAdbShellReceipt(adbAuthorityReceipt(ok = false)))
        assertFalse(isVerifiedAdbShellReceipt(adbAuthorityReceipt(processStarted = false)))
        assertFalse(isVerifiedAdbShellReceipt(adbAuthorityReceipt(code = "process_timed_out")))
        assertFalse(isVerifiedAdbShellReceipt(adbAuthorityReceipt(authorityUid = 0)))
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

    @Test
    fun `cold reconnect failures preserve their structural cause`() {
        val endpoint = SgtAdbCommandBridge.parseProbe(
            """{"state":"degraded","code":"connection_endpoint_unavailable"}""",
        )
        val rejected = SgtAdbCommandBridge.parseProbe(
            """{"state":"needs_user_step","code":"pairing_authorization_rejected"}""",
        )
        val transport = SgtAdbCommandBridge.parseProbe(
            """{"state":"degraded","code":"connection_failed"}""",
        )

        assertEquals(
            SgtAdbBridgeCondition.CONNECTION_ENDPOINT_UNAVAILABLE,
            endpoint.condition,
        )
        assertEquals(SgtAdbBridgeCondition.AUTHORIZATION_REJECTED, rejected.condition)
        assertEquals(SgtAdbBridgeCondition.CONNECTION_FAILED, transport.condition)
        assertTrue(endpoint.requiredUserStep.orEmpty().contains("reconnect"))
        assertTrue(rejected.requiredUserStep.orEmpty().contains("pair it again"))
        assertTrue(transport.requiredUserStep.orEmpty().contains("reconnect"))
    }

    private fun adbAuthorityReceipt(
        code: String = "process_exited",
        timedOut: Boolean = false,
        cancelled: Boolean = false,
        authorityUid: Int = 2_000,
        processStarted: Boolean = true,
        ok: Boolean = !timedOut && !cancelled && code == "process_exited",
    ) = buildJsonObject {
        put("ok", ok)
        put("code", code)
        put("exit_code", 0)
        put("timed_out", timedOut)
        put("cancelled", cancelled)
        put("process_started", processStarted)
        put("authority_uid", authorityUid)
        put("output", "2000")
    }

    private class ChunkedNonTerminatingInput(chunks: List<String>) : InputStream() {
        private val chunks = ArrayDeque(chunks.map(String::encodeToByteArray))
        var readPastTerminal = false
            private set

        override fun read(): Int = error("single-byte reads are not used")

        override fun read(
            buffer: ByteArray,
            offset: Int,
            length: Int,
        ): Int {
            val chunk = chunks.removeFirstOrNull()
            if (chunk == null) {
                readPastTerminal = true
                error("reader waited for transport EOF after the terminal marker")
            }
            check(chunk.size <= length)
            chunk.copyInto(buffer, offset)
            return chunk.size
        }
    }
}
