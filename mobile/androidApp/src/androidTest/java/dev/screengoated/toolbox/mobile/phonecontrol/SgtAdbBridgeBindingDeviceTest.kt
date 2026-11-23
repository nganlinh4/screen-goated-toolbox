package dev.screengoated.toolbox.mobile.phonecontrol

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbServiceClient
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbCommandBridge
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.BrowserTunnelResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandResult
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlOperationId
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SgtAdbBridgeBindingDeviceTest {
    @Test
    fun unavailablePeerTrustStopsEveryBridgeEntryAndPreservesPairing() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val pairedBefore = SgtAdbCommandBridge.hasPairing(context)
        assertEquals(CapabilityState.UNSUPPORTED, SgtAdbCommandBridge.probe(context).state)
        assertEquals(CapabilityState.UNSUPPORTED, SgtAdbCommandBridge.reconnect(context).state)
        assertEquals(
            CapabilityState.UNSUPPORTED,
            SgtAdbCommandBridge.pairProtected(context, charArrayOf('1', '2', '3', '4', '5', '6')).state,
        )
        val tunnel = SgtAdbCommandBridge.openBrowserTunnel(context) as BrowserTunnelResult.Failure
        assertEquals(CapabilityState.UNSUPPORTED, tunnel.state)
        var dispatched = false
        val commandResult = SgtAdbCommandBridge.executeAuthorized(
            context, PhoneControlEffectOwner(PhoneControlOperationId(1, 1, "peer-check")),
            "/system/bin/id", emptyList(), null, 1000,
            beforeDispatch = { dispatched = true; null },
        ) as PrivilegedCommandResult.Failure
        assertEquals(CapabilityState.UNSUPPORTED, commandResult.state)
        assertFalse(commandResult.effectMayHaveOccurred)
        assertFalse(dispatched)
        SgtAdbServiceClient(context).use { client ->
            val service = client.await()
            for (response in listOf(
                service.connectAndVerify(1000),
                service.pairAndVerify("123456", 1000),
                service.openBrowserTunnel(1000),
            )) {
                val result = Json.parseToJsonElement(response).jsonObject
                assertEquals("unsupported", result.getValue("state").jsonPrimitive.content)
                assertEquals("peer_authentication_unavailable", result.getValue("code").jsonPrimitive.content)
            }
            val command = Json.parseToJsonElement(
                service.runCommand("peer-check", "/system/bin/id", emptyArray(), null, 1000),
            ).jsonObject
            assertFalse(command.getValue("process_started").jsonPrimitive.boolean)
            assertFalse(command.getValue("effect_may_have_occurred").jsonPrimitive.boolean)
        }
        assertEquals(pairedBefore, SgtAdbCommandBridge.hasPairing(context))
    }

    @Test
    fun privateBridgeCanBeClosedAndReboundWithoutReusingStaleCallbacks() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext

        repeat(REBIND_ATTEMPTS) {
            SgtAdbServiceClient(context).use { client ->
                assertTrue(client.await().asBinder().pingBinder())
            }
        }
    }

    private companion object {
        const val REBIND_ATTEMPTS = 3
    }
}
