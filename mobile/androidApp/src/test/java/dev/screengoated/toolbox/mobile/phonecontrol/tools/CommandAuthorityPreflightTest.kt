package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityTargetAuthority
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.runSgtAdbDispatch
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.buildJsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CommandAuthorityPreflightTest {
    @Test
    fun activeProtectedWindowRefusesDispatchWithoutInspectingCommand() = runTest {
        var dispatches = 0
        val failure = commandAuthorityPreflight(
            observe = { observation(AccessibilityTargetAuthority.OS_OWNED_USER_STEP) },
            currentGeneration = { 1 },
        )
        if (failure == null) dispatches++
        assertEquals(0, dispatches)
        assertEquals("os_owned_confirmation", failure?.code)
        assertEquals("complete_os_owned_confirmation", failure?.requiredUserStep)
        assertEquals(CapabilityState.NEEDS_USER_STEP, failure?.state)
        assertFalse(failure!!.effectMayHaveOccurred)
    }

    @Test
    fun unavailableOrEmptyObservationFailsClosedAndRoutineWindowPasses() = runTest {
        val unavailable = commandAuthorityPreflight(observe = {
            AccessibilityProviderResult.Failure("unavailable", "Unavailable", retryable = true)
        }, currentGeneration = { 1 })
        assertEquals("authority_observation_unavailable", unavailable?.code)
        assertTrue(unavailable!!.freshObservationRequired)
        assertFalse(unavailable.effectMayHaveOccurred)
        val empty = commandAuthorityPreflight(observe = { observation(AccessibilityTargetAuthority.ROUTINE, empty = true) }, currentGeneration = { 1 })
        assertEquals("authority_observation_unavailable", empty?.code)
        val noActive = commandAuthorityPreflight(
            observe = { observation(AccessibilityTargetAuthority.ROUTINE, active = false) },
            currentGeneration = { 1 },
        )
        assertEquals("authority_observation_unavailable", noActive?.code)
        assertNull(commandAuthorityPreflight(observe = { observation(AccessibilityTargetAuthority.ROUTINE) }, currentGeneration = { 1 }))
        val stale = commandAuthorityPreflight(observe = { observation(AccessibilityTargetAuthority.ROUTINE) }, currentGeneration = { 2 })
        assertEquals("authority_observation_unavailable", stale?.code)
        val unattributed = commandAuthorityPreflight(
            observe = { observation(AccessibilityTargetAuthority.ROUTINE, packageName = null) },
            currentGeneration = { 1 },
        )
        assertEquals("authority_observation_unavailable", unattributed?.code)
        val withController = commandAuthorityPreflight(
            observe = {
                val base = observation(AccessibilityTargetAuthority.ROUTINE) as AccessibilityProviderResult.Success
                AccessibilityProviderResult.Success(base.value.copy(windows = base.value.windows +
                    base.value.windows.single().copy(packageName = null, controllerOwned = true)))
            },
            currentGeneration = { 1 },
        )
        assertNull(withController)
    }

    @Test
    fun queuedAdbDispatchSamplesAuthorityOnlyAfterEnteringItsDispatcher() = runTest {
        val dispatcher = QueuedDispatcher()
        var protected = false
        var samples = 0
        var dispatches = 0
        val result = async {
            runSgtAdbDispatch(
                dispatcher = dispatcher,
                beforeDispatch = {
                    samples++
                    commandAuthorityPreflight(
                        observe = {
                            observation(if (protected) AccessibilityTargetAuthority.OS_OWNED_USER_STEP else AccessibilityTargetAuthority.ROUTINE)
                        },
                        currentGeneration = { 1 },
                    )
                },
                reserve = { true },
                dispatch = { dispatches++; buildJsonObject { } },
            )
        }
        runCurrent()
        assertEquals(0, samples)
        protected = true
        dispatcher.runNext()
        runCurrent()
        assertEquals(1, samples)
        assertEquals(0, dispatches)
        val failure = result.await() as dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandResult.Failure
        assertEquals("os_owned_confirmation", failure.code)
        assertFalse(failure.effectMayHaveOccurred)
    }

    private class QueuedDispatcher : CoroutineDispatcher() {
        private val tasks = ArrayDeque<Runnable>()

        override fun dispatch(context: CoroutineContext, block: Runnable) {
            tasks.addLast(block)
        }

        fun runNext() = tasks.removeFirst().run()
    }

    private fun observation(
        authority: AccessibilityTargetAuthority,
        empty: Boolean = false,
        active: Boolean = true,
        packageName: String? = "example.test",
    ): AccessibilityProviderResult<AccessibilityObservation> = AccessibilityProviderResult.Success(
        AccessibilityObservation(
            generation = 1,
            observedAtMs = 1,
            displayRotation = 0,
            densityDpi = 160,
            windows = if (empty) emptyList() else listOf(
                AccessibilityWindowSnapshot(
                    id = 1,
                    displayId = 0,
                    layer = 1,
                    type = "application",
                    title = null,
                    packageName = packageName,
                    active = active,
                    focused = active,
                    bounds = TargetBounds(0, 0, 100, 100),
                    targetAuthority = authority,
                ),
            ),
            elements = emptyList(),
            truncated = false,
        ),
    )
}
