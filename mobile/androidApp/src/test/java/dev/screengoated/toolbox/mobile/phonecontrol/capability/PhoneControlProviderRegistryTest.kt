package dev.screengoated.toolbox.mobile.phonecontrol.capability

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlProviderRegistryTest {
    @Test
    fun `active projection is reported ready in the runtime capability snapshot`() {
        assertEquals(
            CapabilityState.READY,
            mediaProjectionCapabilityState(isReady = true),
        )
        assertEquals(
            CapabilityState.NEEDS_USER_STEP,
            mediaProjectionCapabilityState(isReady = false),
        )
    }

    @Test
    fun `model context advertises selected ready shell without unavailable provider noise`() {
        val providers = listOf(
            ProviderDefinition("android_app_api", "app", optional = false),
            ProviderDefinition("sgt_adb_bridge", "shell", optional = true),
            ProviderDefinition("root_bridge", "root", optional = true),
        )
        val evidence = PhoneControlProviderEvidence(
            catalog = PhoneControlAuthorityCatalog(
                providers = providers,
                routes = listOf(
                    CapabilityRoute(
                        "command_execution",
                        listOf("sgt_adb_bridge", "root_bridge"),
                    ),
                ),
            ),
            snapshots = listOf(
                snapshot("android_app_api", CapabilityState.READY),
                snapshot("sgt_adb_bridge", CapabilityState.READY),
                snapshot(
                    "root_bridge",
                    CapabilityState.UNAVAILABLE,
                    "Select root from the orb.",
                ),
            ),
            selectedAuthorityProviderId = "sgt_adb_bridge",
        )

        val context = evidence.modelContext()

        assertTrue(context.contains("sgt_adb_bridge is ready"))
        assertTrue(context.contains("run_command executes Android shell programs directly"))
        assertFalse(context.contains("Select root from the orb."))
        assertFalse(context.contains("root_bridge is unavailable"))
        assertEquals(
            "execution_context provider=sgt_adb_bridge provider_state=ready ready_providers=2",
            evidence.diagnosticEvent(),
        )
    }

    @Test
    fun `setup snapshot never advertises a missing shell as a blocker`() {
        val evidence = PhoneControlProviderEvidence(
            catalog = PhoneControlAuthorityCatalog(emptyList(), emptyList()),
            snapshots = listOf(
                snapshot("android_app_api", CapabilityState.READY),
                snapshot(
                    "sgt_adb_bridge",
                    CapabilityState.NEEDS_USER_STEP,
                    "Finish setup.",
                ),
            ),
            selectedAuthorityProviderId = "sgt_adb_bridge",
        )

        val context = evidence.modelContext()

        assertTrue(context.contains("Every tool probes its provider at invocation"))
        assertFalse(context.contains("Finish setup."))
        assertFalse(context.contains("needs_user_step"))
        assertFalse(context.contains("Elevated shell:"))
    }

    private fun snapshot(
        id: String,
        state: CapabilityState,
        requiredUserStep: String? = null,
    ) = ProviderSnapshot(
        providerId = id,
        state = state,
        supportedCapabilities = emptyMap(),
        evidenceTimestampMs = 1,
        requiredUserStep = requiredUserStep,
    )
}
