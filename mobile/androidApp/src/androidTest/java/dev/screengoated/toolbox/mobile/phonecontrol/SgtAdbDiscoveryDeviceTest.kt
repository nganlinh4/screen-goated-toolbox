package dev.screengoated.toolbox.mobile.phonecontrol

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbDiscovery
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.isSgtAdbDeviceIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.matchesSgtAdbServiceIdentity
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SgtAdbDiscoveryDeviceTest {
    @Test
    fun exactPersistedIdentitySelectsOnlyTheDevicesLocalConnectService() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val expected = requireNotNull(
            InstrumentationRegistry.getArguments().getString(EXPECTED_IDENTITY_ARGUMENT),
        )
        assertTrue(isSgtAdbDeviceIdentity(expected))

        val endpoint = SgtAdbDiscovery.connection(
            context = instrumentation.targetContext,
            expectedServiceName = expected,
            timeoutMs = DISCOVERY_TIMEOUT_MS,
        )

        assertNotNull(endpoint)
        assertEquals(expected, endpoint?.serviceName)
        assertNull(
            SgtAdbDiscovery.connection(
                context = instrumentation.targetContext,
                expectedServiceName = "$expected-other",
                timeoutMs = REJECTION_TIMEOUT_MS,
            ),
        )
    }

    @Test
    fun activePairingSurfaceBelongsToTheConnectIdentityFamily() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        assumeTrue(
            InstrumentationRegistry.getArguments()
                .getString(PAIRING_SURFACE_ACTIVE_ARGUMENT)
                .toBoolean(),
        )
        val expected = requireNotNull(
            InstrumentationRegistry.getArguments().getString(EXPECTED_IDENTITY_ARGUMENT),
        )
        val endpoint = SgtAdbDiscovery.pairing(
            context = instrumentation.targetContext,
            timeoutMs = DISCOVERY_TIMEOUT_MS,
        )

        assertNotNull(endpoint)
        assertTrue(
            "Pair and connect advertisements did not share an ADB identity family.",
            endpoint?.serviceName?.let { pairing ->
                matchesSgtAdbServiceIdentity(expected, pairing)
            } == true,
        )
    }

    private companion object {
        const val EXPECTED_IDENTITY_ARGUMENT = "sgtAdbExpectedIdentity"
        const val PAIRING_SURFACE_ACTIVE_ARGUMENT = "sgtAdbPairingSurfaceActive"
        const val DISCOVERY_TIMEOUT_MS = 10_000L
        const val REJECTION_TIMEOUT_MS = 1_500L
    }
}
