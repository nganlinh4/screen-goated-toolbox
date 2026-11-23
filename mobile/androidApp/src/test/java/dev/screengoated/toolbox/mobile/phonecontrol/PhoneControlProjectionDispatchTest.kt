package dev.screengoated.toolbox.mobile.phonecontrol

import kotlinx.coroutines.async
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlUserStepState
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlProjectionRequest
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.activity.result.ActivityResult
import org.junit.Assert.assertFalse
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.Before
import org.junit.After

@OptIn(ExperimentalCoroutinesApi::class)
class PhoneControlProjectionDispatchTest {
    @Test
    fun pendingConsentResultSurvivesRecreationButIsReleasedWithOwner() {
        val store = ViewModelStore()
        val owner = ViewModelProvider.create(store, ViewModelProvider.NewInstanceFactory())
            .get(PhoneControlUserStepState::class)
        val consent = PhoneControlProjectionDispatches.reserveConsent()
        val result = ActivityResult(0, null)
        owner.projectionRequest = PhoneControlProjectionRequest.Activation(consent)
        owner.pendingProjectionResult = result
        owner.projectionSettling = true
        val recreated = ViewModelProvider.create(store, ViewModelProvider.NewInstanceFactory())
            .get(PhoneControlUserStepState::class)
        assertSame(owner, recreated)
        assertSame(result, recreated.pendingProjectionResult)
        store.clear()
        assertFalse(PhoneControlProjectionDispatches.ownsConsent(consent))
        assertNull(recreated.pendingProjectionResult)
    }

    @Test
    fun retainedConsentWaitsForStaleServiceDestructionBeforeDelivery() = runTest {
        val oldBoundary = PhoneControlProjectionDispatches.stopBoundary()
        val consent = PhoneControlProjectionDispatches.reserveConsent()
        val teardown = PhoneControlProjectionDispatches.beginTeardownForStop(oldBoundary)
        try {
            assertEquals(consent, teardown.retainedConsent)
            val delivery = async {
                PhoneControlProjectionDispatches.awaitTeardown()
                PhoneControlProjectionDispatches.ownsConsent(consent)
            }
            runCurrent()
            assertFalse(delivery.isCompleted)
            PhoneControlProjectionDispatches.continueTeardown(teardown.retainedConsent)
            assertFalse(delivery.isCompleted)
            PhoneControlProjectionDispatches.endStop()
            assertTrue(delivery.await())
        } finally {
            PhoneControlProjectionDispatches.endStop()
            PhoneControlProjectionDispatches.releaseConsent(consent)
        }
    }

    @Test
    fun consentCancelledWhileTeardownWaitsCannotBeDelivered() = runTest {
        val oldBoundary = PhoneControlProjectionDispatches.stopBoundary()
        val consent = PhoneControlProjectionDispatches.reserveConsent()
        PhoneControlProjectionDispatches.beginTeardownForStop(oldBoundary)
        try {
            val delivery = async {
                PhoneControlProjectionDispatches.awaitTeardown()
                PhoneControlProjectionDispatches.ownsConsent(consent)
            }
            runCurrent()
            PhoneControlProjectionDispatches.releaseConsent(consent)
            PhoneControlProjectionDispatches.endStop()
            assertFalse(delivery.await())
        } finally {
            PhoneControlProjectionDispatches.endStop()
            PhoneControlProjectionDispatches.releaseConsent(consent)
        }
    }

    @Before
    fun installMainDispatcher() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
    }

    @After
    fun resetMainDispatcher() {
        Dispatchers.resetMain()
    }
    @Test
    fun queuedDelayIsRetiredButAdmittedDelayWaitsForReceipt() = runTest {
        val queued = PhoneControlProjectionDispatches.create()
        val queuedReplay = async { awaitSafeProjectionReplay(queued, timeoutMs = 100) }
        runCurrent()
        advanceTimeBy(101)
        runCurrent()
        assertTrue(queuedReplay.isCompleted)
        assertEquals(PhoneControlProjectionReceipt.EXPIRED, queuedReplay.await())
        assertNull(PhoneControlProjectionDispatches.admit(queued.id))
        assertTrue(PhoneControlProjectionDispatches.claimReplay(queued) != null)

        val admitted = PhoneControlProjectionDispatches.create()
        try {
            assertSame(admitted, PhoneControlProjectionDispatches.admit(admitted.id))
            val admittedReplay = async { awaitSafeProjectionReplay(admitted, timeoutMs = 100) }
            runCurrent()
            advanceTimeBy(101)
            runCurrent()
            assertFalse(admittedReplay.isCompleted)
            PhoneControlProjectionDispatches.finish(admitted)
            runCurrent()
            assertTrue(admittedReplay.isCompleted)
            assertEquals(PhoneControlProjectionReceipt.SETTLED, admittedReplay.await())
        } finally {
            PhoneControlProjectionDispatches.finish(admitted)
            PhoneControlProjectionDispatches.claimReplay(admitted)
        }
    }

    @Test
    fun laterActivityAdoptsAndCanRetireAnUnadmittedStart() = runTest {
        val firstActivity = PhoneControlUserStepState()
        val dispatch = PhoneControlProjectionDispatches.create()
        try {
            firstActivity.projectionDispatch = dispatch
            firstActivity.projectionSettling = true
            // A separate Activity has a new ViewModel but finds the same live ticket.
            val nextActivity = PhoneControlUserStepState()
            val outstanding = requireNotNull(PhoneControlProjectionDispatches.current())
            assertSame(dispatch, outstanding)
            assertTrue(nextActivity.adoptProjectionDispatch(outstanding))
            assertSame(dispatch, nextActivity.projectionDispatch)
            assertTrue(PhoneControlProjectionDispatches.retireIfPending(outstanding))
            assertEquals(PhoneControlProjectionReceipt.RETIRED, dispatch.settled.await())
            assertNull(PhoneControlProjectionDispatches.admit(dispatch.id))
            assertSame(dispatch, PhoneControlProjectionDispatches.current())
            assertTrue(PhoneControlProjectionDispatches.claimReplay(dispatch) != null)
            assertNull(PhoneControlProjectionDispatches.current())
        } finally {
            PhoneControlProjectionDispatches.finish(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
        }
    }

    @Test
    fun laterActivityMustWaitForAnAdmittedStart() = runTest {
        val dispatch = PhoneControlProjectionDispatches.create()
        try {
            assertSame(dispatch, PhoneControlProjectionDispatches.admit(dispatch.id))
            val nextActivity = PhoneControlUserStepState()
            assertTrue(nextActivity.adoptProjectionDispatch(
                requireNotNull(PhoneControlProjectionDispatches.current()),
            ))
            assertFalse(PhoneControlProjectionDispatches.retireIfPending(dispatch))
            val waiter = async { awaitSafeProjectionReplay(dispatch, timeoutMs = 1) }
            runCurrent()
            advanceTimeBy(2)
            runCurrent()
            assertFalse(waiter.isCompleted)
            PhoneControlProjectionDispatches.finish(dispatch)
            runCurrent()
            assertEquals(PhoneControlProjectionReceipt.SETTLED, waiter.await())
            assertSame(dispatch, PhoneControlProjectionDispatches.current())
            assertTrue(PhoneControlProjectionDispatches.claimReplay(dispatch) != null)
            assertNull(PhoneControlProjectionDispatches.current())
        } finally {
            PhoneControlProjectionDispatches.finish(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
        }
    }

    @Test
    fun delayedQueuedStartCanBeRetiredBeforeReplay() {
        val dispatch = PhoneControlProjectionDispatches.create()
        assertTrue(PhoneControlProjectionDispatches.retireIfPending(dispatch))
        assertTrue(dispatch.settled.isCompleted)
        assertNull(PhoneControlProjectionDispatches.admit(dispatch.id))
        assertTrue(PhoneControlProjectionDispatches.claimReplay(dispatch) != null)
    }

    @Test
    fun admittedStartMustReportTerminalReceiptBeforeReplay() {
        val dispatch = PhoneControlProjectionDispatches.create()
        try {
            assertSame(dispatch, PhoneControlProjectionDispatches.admit(dispatch.id))
            assertNull(PhoneControlProjectionDispatches.admit(dispatch.id))
            assertFalse(PhoneControlProjectionDispatches.retireIfPending(dispatch))
            assertFalse(dispatch.settled.isCompleted)
            PhoneControlProjectionDispatches.finish(dispatch)
            assertTrue(dispatch.settled.isCompleted)
            assertNull(PhoneControlProjectionDispatches.admit(dispatch.id))
        } finally {
            PhoneControlProjectionDispatches.finish(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
        }
    }

    @Test
    fun stopBeforeServiceDispatchRetiresQueuedStart() {
        val dispatch = PhoneControlProjectionDispatches.create()
        var effects = 0
        assertEquals(
            PhoneControlStopAdmission.Accepted,
            PhoneControlProjectionDispatches.beginStop(PhoneControlProjectionDispatches.stopBoundary()),
        )
        PhoneControlProjectionDispatches.endStop()
        assertEquals(PhoneControlProjectionReceipt.RETIRED, dispatch.settled.getCompleted())
        assertFalse(runProjectionDispatch(dispatch.id) { effects += 1 })
        assertEquals(0, effects)
        assertNull(PhoneControlProjectionDispatches.current())
    }

    @Test
    fun stopDuringAdmittedDispatchWaitsForTerminalReceipt() {
        val dispatch = PhoneControlProjectionDispatches.create()
        try {
            assertTrue(runProjectionDispatch(dispatch.id) {
                val admission = PhoneControlProjectionDispatches.beginStop(
                    PhoneControlProjectionDispatches.stopBoundary(),
                )
                assertTrue(admission is PhoneControlStopAdmission.Wait)
                assertTrue(runCatching { PhoneControlProjectionDispatches.reserveConsent() }.isFailure)
                assertFalse(dispatch.settled.isCompleted)
                assertSame(dispatch, PhoneControlProjectionDispatches.current())
            })
            assertEquals(PhoneControlProjectionReceipt.SETTLED, dispatch.settled.getCompleted())
            assertEquals(
                PhoneControlStopAdmission.Accepted,
                PhoneControlProjectionDispatches.beginStop(PhoneControlProjectionDispatches.stopBoundary()),
            )
            PhoneControlProjectionDispatches.endStop()
            assertNull(PhoneControlProjectionDispatches.current())
        } finally {
            PhoneControlProjectionDispatches.endStop()
            PhoneControlProjectionDispatches.finish(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
        }
    }

    @Test
    fun delayedStopCannotRetireNewerProjectionAndAdmittedStopBlocksNewStart() {
        val oldBoundary = PhoneControlProjectionDispatches.stopBoundary()
        val newer = PhoneControlProjectionDispatches.create()
        try {
            assertEquals(PhoneControlStopAdmission.Stale, PhoneControlProjectionDispatches.beginStop(oldBoundary))
            assertSame(newer, PhoneControlProjectionDispatches.current())
            assertEquals(
                PhoneControlStopAdmission.Accepted,
                PhoneControlProjectionDispatches.beginStop(PhoneControlProjectionDispatches.stopBoundary()),
            )
            assertNull(PhoneControlProjectionDispatches.admit(newer.id))
            assertTrue(runCatching { PhoneControlProjectionDispatches.create() }.isFailure)
        } finally {
            PhoneControlProjectionDispatches.endStop()
            PhoneControlProjectionDispatches.finish(newer)
            PhoneControlProjectionDispatches.claimReplay(newer)
        }
        val afterStop = PhoneControlProjectionDispatches.create()
        PhoneControlProjectionDispatches.retireIfPending(afterStop)
        PhoneControlProjectionDispatches.claimReplay(afterStop)
    }

    @Test
    fun queuedProjectionExpiresAndNewStartsWaitUntilTeardownCompletes() {
        val dispatch = PhoneControlProjectionDispatches.create()
        try {
            PhoneControlProjectionDispatches.beginTeardown()
            assertEquals(PhoneControlProjectionReceipt.EXPIRED, dispatch.settled.getCompleted())
            assertNull(PhoneControlProjectionDispatches.admit(dispatch.id))
            assertTrue(runCatching { PhoneControlProjectionDispatches.create() }.isFailure)
            assertTrue(PhoneControlProjectionDispatches.claimReplay(dispatch) != null)
            // Delivery of a retry cannot reopen admission before onDestroy.
            assertTrue(runCatching { PhoneControlProjectionDispatches.create() }.isFailure)
        } finally {
            PhoneControlProjectionDispatches.endStop()
        }
        val retry = PhoneControlProjectionDispatches.create()
        PhoneControlProjectionDispatches.retireIfPending(retry)
        PhoneControlProjectionDispatches.claimReplay(retry)
    }

    @Test
    fun unexpectedDestroyPermanentlyRetiresEarlierConsent() {
        val consent = PhoneControlProjectionDispatches.reserveConsent()
        assertTrue(PhoneControlProjectionDispatches.ownsConsent(consent))
        try {
            PhoneControlProjectionDispatches.beginTeardown()
            assertFalse(PhoneControlProjectionDispatches.ownsConsent(consent))
        } finally {
            PhoneControlProjectionDispatches.endStop()
        }
        // A late result remains stale even after a new service may start.
        assertFalse(PhoneControlProjectionDispatches.ownsConsent(consent))
        assertTrue(runCatching {
            PhoneControlProjectionDispatches.create(consentGeneration = consent)
        }.isFailure)
        val retry = PhoneControlProjectionDispatches.reserveConsent()
        assertTrue(PhoneControlProjectionDispatches.ownsConsent(retry))
        PhoneControlProjectionDispatches.releaseConsent(retry)
    }

    @Test
    fun olderStopFailureCannotPublishAfterNewConsent() {
        val first = PhoneControlProjectionDispatches.reserveConsent()
        var published = ""
        assertTrue(PhoneControlProjectionDispatches.publishForCurrentBoundary(first) { published = "first" })
        PhoneControlProjectionDispatches.releaseConsent(first)
        val second = PhoneControlProjectionDispatches.reserveConsent { published = "cleared" }
        try {
            assertFalse(PhoneControlProjectionDispatches.publishForCurrentBoundary(first) { published = "stale" })
            assertEquals("cleared", published)
            assertTrue(PhoneControlProjectionDispatches.publishForCurrentBoundary(second) { published = "second" })
            assertEquals("second", published)
        } finally {
            PhoneControlProjectionDispatches.releaseConsent(second)
        }
    }

    @Test
    fun failedTemporaryPromotionHoldsBarrierAndExpiresNewerQueuedStart() {
        val oldBoundary = PhoneControlProjectionDispatches.stopBoundary()
        val consent = PhoneControlProjectionDispatches.reserveConsent()
        val dispatch = PhoneControlProjectionDispatches.create(consentGeneration = consent)
        try {
            assertNull(PhoneControlProjectionDispatches.beginTeardownForStop(oldBoundary).retryBoundary)
            assertEquals(PhoneControlProjectionReceipt.EXPIRED, dispatch.settled.getCompleted())
            assertNull(PhoneControlProjectionDispatches.admit(dispatch.id))
            PhoneControlProjectionDispatches.claimReplay(dispatch)
            assertTrue(runCatching { PhoneControlProjectionDispatches.reserveConsent() }.isFailure)
        } finally {
            PhoneControlProjectionDispatches.endStop()
        }
        val fresh = PhoneControlProjectionDispatches.reserveConsent()
        PhoneControlProjectionDispatches.releaseConsent(fresh)
    }

    @Test
    fun delayedStaleStopPreservesNewConsentAcrossDestruction() {
        val oldBoundary = PhoneControlProjectionDispatches.stopBoundary()
        val newer = PhoneControlProjectionDispatches.reserveConsent()
        try {
            val teardown = PhoneControlProjectionDispatches.beginTeardownForStop(oldBoundary)
            assertNull(teardown.retryBoundary)
            assertEquals(newer, teardown.retainedConsent)
            assertFalse(PhoneControlProjectionDispatches.ownsConsent(newer))
            assertTrue(runCatching { PhoneControlProjectionDispatches.reserveConsent() }.isFailure)
            assertTrue(runCatching { PhoneControlProjectionDispatches.create(consentGeneration = newer) }.isFailure)
            PhoneControlProjectionDispatches.continueTeardown(teardown.retainedConsent)
        } finally {
            PhoneControlProjectionDispatches.endStop()
        }
        assertTrue(PhoneControlProjectionDispatches.ownsConsent(newer))
        val dispatch = PhoneControlProjectionDispatches.create(consentGeneration = newer)
        assertTrue(PhoneControlProjectionDispatches.retireIfPending(dispatch))
        PhoneControlProjectionDispatches.claimReplay(dispatch)
    }

    @Test
    fun consentIsSingleFlightAndRetiredOwnerCannotReleaseNewerReservation() {
        val first = PhoneControlProjectionDispatches.reserveConsent()
        try {
            assertTrue(runCatching { PhoneControlProjectionDispatches.reserveConsent() }.isFailure)
            assertTrue(PhoneControlProjectionDispatches.ownsConsent(first))
        } finally {
            PhoneControlProjectionDispatches.releaseConsent(first)
        }
        val second = PhoneControlProjectionDispatches.reserveConsent()
        try {
            assertTrue(first != second)
            PhoneControlProjectionDispatches.releaseConsent(first)
            assertTrue(PhoneControlProjectionDispatches.ownsConsent(second))
        } finally {
            PhoneControlProjectionDispatches.releaseConsent(second)
        }
    }

    @Test
    fun clearingOwnerReleasesConsentWithoutReleasingAnotherOwner() {
        val store = ViewModelStore()
        val owner = ViewModelProvider.create(store, ViewModelProvider.NewInstanceFactory())
            .get(PhoneControlUserStepState::class)
        val first = PhoneControlProjectionDispatches.reserveConsent()
        owner.projectionRequest = PhoneControlProjectionRequest.Activation(first)
        store.clear()
        assertFalse(PhoneControlProjectionDispatches.ownsConsent(first))
        val next = PhoneControlProjectionDispatches.reserveConsent()
        try {
            owner.releaseProjectionConsent()
            assertTrue(PhoneControlProjectionDispatches.ownsConsent(next))
        } finally {
            PhoneControlProjectionDispatches.releaseConsent(next)
        }
    }

    @Test
    fun staleDeliveryDoesNotMakeNewerQueuedStartLookIdle() {
        val stale = PhoneControlProjectionDispatches.create()
        PhoneControlProjectionDispatches.retireIfPending(stale)
        PhoneControlProjectionDispatches.claimReplay(stale)
        val newer = PhoneControlProjectionDispatches.create()
        try {
            assertFalse(runProjectionDispatch(stale.id) { error("stale effect") })
            assertTrue(PhoneControlProjectionDispatches.hasUnsettled())
            assertTrue(runProjectionDispatch(newer.id) {})
            assertFalse(PhoneControlProjectionDispatches.hasUnsettled())
        } finally {
            PhoneControlProjectionDispatches.finish(newer)
            PhoneControlProjectionDispatches.claimReplay(newer)
        }
    }

    @Test
    fun admittedProjectionSettlesButTeardownStillBlocksNewStarts() {
        val dispatch = PhoneControlProjectionDispatches.create()
        try {
            assertSame(dispatch, PhoneControlProjectionDispatches.admit(dispatch.id))
            PhoneControlProjectionDispatches.beginTeardown()
            assertFalse(dispatch.settled.isCompleted)
            PhoneControlProjectionDispatches.finish(dispatch)
            assertEquals(PhoneControlProjectionReceipt.SETTLED, dispatch.settled.getCompleted())
            assertTrue(PhoneControlProjectionDispatches.claimReplay(dispatch) != null)
            assertTrue(runCatching { PhoneControlProjectionDispatches.create() }.isFailure)
        } finally {
            PhoneControlProjectionDispatches.endStop()
        }
    }

    @Test
    fun deferredReplayWaitsForTeardownAfterDispatchSettles() = runTest {
        val dispatch = PhoneControlProjectionDispatches.create()
        try {
            assertSame(dispatch, PhoneControlProjectionDispatches.admit(dispatch.id))
            PhoneControlProjectionDispatches.beginTeardown()
            PhoneControlProjectionDispatches.finish(dispatch)
            val replay = async {
                awaitSafeProjectionReplay(dispatch)
                PhoneControlProjectionDispatches.awaitTeardown()
                PhoneControlProjectionDispatches.claimReplay(dispatch)
            }
            runCurrent()
            assertFalse(replay.isCompleted)
            assertTrue(runCatching { PhoneControlProjectionDispatches.create() }.isFailure)
            PhoneControlProjectionDispatches.endStop()
            runCurrent()
            assertTrue(replay.await() != null)

            val retry = PhoneControlProjectionDispatches.create()
            assertTrue(PhoneControlProjectionDispatches.retireIfPending(retry))
            assertTrue(PhoneControlProjectionDispatches.claimReplay(retry) != null)
        } finally {
            PhoneControlProjectionDispatches.endStop()
            PhoneControlProjectionDispatches.finish(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
        }
    }

    @Test
    fun stopActionIsPreparedBeforeNewGenerationBecomesVisible() {
        val previous = PhoneControlProjectionDispatches.stopBoundary()
        var prepared: String? = null
        val dispatch = PhoneControlProjectionDispatches.create(beforePublish = { next ->
            assertEquals(previous, PhoneControlProjectionDispatches.stopBoundary())
            prepared = next
        })
        try {
            assertEquals(prepared, PhoneControlProjectionDispatches.stopBoundary())
            assertTrue(previous != prepared)
        } finally {
            PhoneControlProjectionDispatches.retireIfPending(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
        }
        val stable = PhoneControlProjectionDispatches.stopBoundary()
        assertTrue(runCatching {
            PhoneControlProjectionDispatches.create(beforePublish = { error("prepare failed") })
        }.isFailure)
        assertEquals(stable, PhoneControlProjectionDispatches.stopBoundary())
        assertNull(PhoneControlProjectionDispatches.current())
    }

    @Test
    fun notificationStopUsesDeliverySequenceRatherThanConstructionSequence() {
        PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        val boundary = PhoneControlProjectionDispatches.stopBoundary()
        val builtAt = PhoneControlProjectionDispatches.currentIntentSequence()
        PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        val delivered = requireNotNull(stampNotificationStop(boundary) {
            PhoneControlProjectionDispatches.currentIntentSequence()
        })
        assertTrue(delivered.sequence != builtAt)
        assertEquals(boundary, delivered.boundary)
        PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        assertTrue(delivered.sequence != PhoneControlProjectionDispatches.currentIntentSequence())
        assertNull(stampNotificationStop(null) { error("missing boundary must not be stamped") })
    }

    @Test
    fun newConsentSupersedesOlderStopAndLaterStopRetiresItsGrant() {
        val oldStop = PhoneControlProjectionDispatches.stopBoundary()
        var prepared: String? = null
        val consent = PhoneControlProjectionDispatches.reserveConsent { next ->
            assertEquals(oldStop, PhoneControlProjectionDispatches.stopBoundary())
            prepared = next
        }
        assertEquals(consent, prepared)
        assertEquals(PhoneControlStopAdmission.Stale, PhoneControlProjectionDispatches.beginStop(oldStop))
        assertTrue(PhoneControlProjectionDispatches.ownsConsent(consent))

        val currentStop = PhoneControlProjectionDispatches.stopBoundary()
        assertEquals(PhoneControlStopAdmission.Accepted, PhoneControlProjectionDispatches.beginStop(currentStop))
        try {
            assertFalse(PhoneControlProjectionDispatches.ownsConsent(consent))
        } finally {
            PhoneControlProjectionDispatches.endStop()
        }
        assertTrue(runCatching {
            PhoneControlProjectionDispatches.create(consentGeneration = consent)
        }.isFailure)
        assertNull(PhoneControlProjectionDispatches.current())

        val newConsent = PhoneControlProjectionDispatches.reserveConsent()
        val valid = PhoneControlProjectionDispatches.create(consentGeneration = newConsent)
        assertEquals(newConsent, PhoneControlProjectionDispatches.stopBoundary())
        PhoneControlProjectionDispatches.retireIfPending(valid)
        PhoneControlProjectionDispatches.claimReplay(valid)
    }

    @Test
    fun multipleActivitiesShareOneLatestReentryAndOneReplayClaim() {
        val latest = LatestProjectionReentry<String>()
        latest.replace("older choice")
        latest.replace("cancel")
        assertEquals("cancel", latest.take())
        assertNull(latest.take())
        assertTrue(latest.isEmpty())

        val dispatch = PhoneControlProjectionDispatches.create()
        try {
            PhoneControlProjectionDispatches.admit(dispatch.id)
            PhoneControlProjectionDispatches.finish(dispatch)
            assertSame(dispatch, PhoneControlProjectionDispatches.current())
            assertTrue(PhoneControlProjectionDispatches.claimReplay(dispatch) != null)
            assertNull(PhoneControlProjectionDispatches.claimReplay(dispatch))
        } finally {
            PhoneControlProjectionDispatches.finish(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
        }
    }

    @Test
    fun orphanReplayIsSingleUseAndSupersededByNewerIntent() {
        val generation = ProjectionOrphanGeneration()
        generation.set(41L)
        assertEquals(OrphanProjectionReentry.SUPERSEDED, generation.inspect(null))
        assertEquals(OrphanProjectionReentry.STALE_UNOWNED, generation.inspect(41L))
        generation.set(42L)
        assertEquals(OrphanProjectionReentry.STALE, generation.inspect(41L))
        assertEquals(OrphanProjectionReentry.ACCEPTED, generation.inspect(42L))
        assertEquals(OrphanProjectionReentry.STALE_UNOWNED, generation.inspect(42L))
        assertEquals(OrphanProjectionReentry.NONE, generation.inspect(null))
        // A fresh process has no outstanding notification owner.
        assertEquals(OrphanProjectionReentry.STALE_UNOWNED, ProjectionOrphanGeneration().inspect(42L))
    }
}
