package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The wireless ADB transport cannot authenticate the adbd command endpoint, so
 * SGT Bridge must refuse before any discovery, pairing, connect, command or
 * browser tunnel reaches the network or the bridge service.
 */
class SgtAdbPeerTrustFailClosedTest {
    @Test
    fun peerTrustIsUnavailable() {
        assertFalse(SgtAdbPeerTrust.isAvailable())
    }

    @Test
    fun everyTransportEntryRefusesBeforeItsFirstEffect() {
        GATES.forEach { gate ->
            val body = source(gate.file).substringAfter(gate.signature, missingDelimiterValue = "")
            assertTrue("${gate.file}: ${gate.signature} not found", body.isNotEmpty())
            val guard = body.indexOf(GUARD)
            val effect = body.indexOf(gate.firstEffect)
            assertTrue("${gate.signature} must check peer trust", guard >= 0)
            assertTrue("${gate.signature} must reach ${gate.firstEffect}", effect >= 0)
            assertTrue("${gate.signature} acts before the peer-trust check", guard < effect)
        }
    }

    private data class Gate(val file: String, val signature: String, val firstEffect: String)

    private fun source(name: String): String {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, "$PRIVILEGED_DIR/$name").readText()
    }

    private companion object {
        const val PRIVILEGED_DIR =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/provider/privileged"
        const val GUARD = "if (!SgtAdbPeerTrust.isAvailable())"
        const val MANAGER = "SgtAdbConnectionManager.kt"
        const val BRIDGE = "SgtAdbCommandBridge.kt"
        const val SERVICE = "SgtAdbBridgeService.kt"
        val GATES = listOf(
            Gate(MANAGER, "suspend fun pairAndConnect(", "SgtAdbDiscovery.pairing("),
            Gate(MANAGER, "private suspend fun connectDiscoveredUntil(", "SgtAdbPairingStore.deviceIdentity("),
            Gate(BRIDGE, "suspend fun pairProtected(", "withContext(Dispatchers.IO)"),
            Gate(BRIDGE, "suspend fun executeAuthorized(", "awaitRemote(context)"),
            Gate(BRIDGE, "suspend fun openBrowserTunnel(", "awaitRemote(context)"),
            Gate(SERVICE, "override fun connectAndVerify(", "manager().connectDiscovered("),
            Gate(SERVICE, "override fun pairAndVerify(", "pairingCode.isAsciiPairingCode()"),
            Gate(SERVICE, "override fun runCommand(", "runner().run("),
            Gate(SERVICE, "override fun openBrowserTunnel(", "manager().connectDiscovered("),
        )
    }
}
