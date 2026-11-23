package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Intent
import android.content.ContextWrapper
import android.content.ComponentName
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlUserStepState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PhoneControlProjectionReplayDeviceTest {
    @Test
    fun earlyStopRetryReplaysAfterTeardownOnlyWhileItsStampIsCurrent() {
        val original = PhoneControlProjectionDispatches.stopBoundary()
        val retryBoundary = requireNotNull(PhoneControlProjectionDispatches.beginTeardownForStop(original).retryBoundary)
        val sequence = PhoneControlProjectionDispatches.currentIntentSequence()
        val pending = DeferredPhoneControlStop()
        val retry = Intent().setAction(ACTION_PHONE_CONTROL_STOP)
            .putExtra(EXTRA_STOP_BOUNDARY, retryBoundary).putExtra(EXTRA_STOP_SEQUENCE, sequence)
        try {
            pending.defer(retry)
            pending.defer(Intent(retry).putExtra(EXTRA_STOP_BOUNDARY, original))
            PhoneControlProjectionDispatches.endStop()
            var replayed: PhoneControlStopStamp? = null
            pending.replay { replayed = it }
            assertEquals(PhoneControlStopStamp(retryBoundary, sequence), replayed)
            pending.replay { throw AssertionError("replayed twice") }

            pending.defer(retry)
            PhoneControlProjectionDispatches.inspectOrphanReentry(null)
            pending.replay { throw AssertionError("replayed over a later action") }
            pending.defer(retry)
            val newerConsent = PhoneControlProjectionDispatches.reserveConsent()
            try {
                pending.replay { throw AssertionError("replayed over newer consent") }
            } finally {
                PhoneControlProjectionDispatches.releaseConsent(newerConsent)
            }
        } finally {
            PhoneControlProjectionDispatches.endStop()
            PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        }
    }

    @Test
    fun receiverForwardsDeliveryStampToServiceAndPreservesLaterAction() {
        val base = InstrumentationRegistry.getInstrumentation().targetContext
        var forwarded: Intent? = null
        val context = object : ContextWrapper(base) {
            override fun startForegroundService(service: Intent): ComponentName? {
                forwarded = Intent(service)
                return service.component
            }
            override fun startService(service: Intent): ComponentName? = startForegroundService(service)
        }
        PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        val dispatch = PhoneControlProjectionDispatches.create()
        val boundary = PhoneControlProjectionDispatches.stopBoundary()
        try {
            PhoneControlProjectionDispatches.stageReentry(dispatch, Intent("before"))
            PhoneControlStopReceiver().onReceive(context, Intent(context, PhoneControlStopReceiver::class.java)
                .setAction(ACTION_PHONE_CONTROL_STOP).putExtra(EXTRA_STOP_BOUNDARY, boundary))
            val stop = requireNotNull(forwarded)
            val stamp = stop.getLongExtra(EXTRA_STOP_SEQUENCE, -1)
            PhoneControlProjectionDispatches.inspectOrphanReentry(null)
            PhoneControlProjectionDispatches.stageReentry(dispatch, Intent("after"))
            assertEquals(PhoneControlStopAdmission.Accepted,
                PhoneControlProjectionDispatches.beginStop(stop.getStringExtra(EXTRA_STOP_BOUNDARY), stamp))
            PhoneControlProjectionDispatches.endStop()
            assertEquals("after", PhoneControlProjectionDispatches.claimReplay(dispatch)?.intent?.action)
        } finally {
            PhoneControlProjectionDispatches.endStop()
            PhoneControlProjectionDispatches.finish(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
            PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        }
    }

    @Test
    fun notificationStopReceiverIsNotExported() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        @Suppress("DEPRECATION")
        val receiver = context.packageManager.getReceiverInfo(
            ComponentName(context, PhoneControlStopReceiver::class.java), 0,
        )
        assertFalse(receiver.exported)
    }

    @Test
    fun notificationStopDropsActionsThatPrecedeItsDelivery() {
        PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        val dispatch = PhoneControlProjectionDispatches.create()
        val boundary = PhoneControlProjectionDispatches.stopBoundary()
        try {
            // This action arrives after notification construction but before delivery.
            PhoneControlProjectionDispatches.inspectOrphanReentry(null)
            assertTrue(PhoneControlProjectionDispatches.stageReentry(dispatch, Intent("earlier")))
            val stamp = requireNotNull(stampNotificationStop(boundary) {
                PhoneControlProjectionDispatches.currentIntentSequence()
            })
            assertEquals(
                PhoneControlStopAdmission.Accepted,
                PhoneControlProjectionDispatches.beginStop(stamp.boundary, stamp.sequence),
            )
            assertNull(PhoneControlProjectionDispatches.claimReplay(dispatch))
        } finally {
            PhoneControlProjectionDispatches.endStop()
            PhoneControlProjectionDispatches.finish(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
            PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        }
    }

    @Test
    fun actionAfterWaitingStopSurvivesButEarlierActionCannotReturn() {
        PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        val dispatch = PhoneControlProjectionDispatches.create()
        val boundary = PhoneControlProjectionDispatches.stopBoundary()
        val stopSequence = PhoneControlProjectionDispatches.currentIntentSequence()
        try {
            assertSame(dispatch, PhoneControlProjectionDispatches.admit(dispatch.id))
            assertTrue(PhoneControlProjectionDispatches.stageReentry(dispatch, Intent("before")))
            assertTrue(PhoneControlProjectionDispatches.beginStop(boundary, stopSequence) is PhoneControlStopAdmission.Wait)
            PhoneControlProjectionDispatches.inspectOrphanReentry(null)
            assertTrue(PhoneControlProjectionDispatches.stageReentry(dispatch, Intent("after")))
            PhoneControlProjectionDispatches.finish(dispatch)
            assertEquals(PhoneControlStopAdmission.Accepted, PhoneControlProjectionDispatches.beginStop(boundary, stopSequence))
            PhoneControlProjectionDispatches.endStop()
            assertEquals("after", PhoneControlProjectionDispatches.claimReplay(dispatch)?.intent?.action)
            assertEquals(
                DetachedProjectionReentry.SUPERSEDED,
                PhoneControlProjectionDispatches.preserveDetachedReentry(stopSequence, Intent("before")) { true },
            )
        } finally {
            PhoneControlProjectionDispatches.endStop()
            PhoneControlProjectionDispatches.finish(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
            PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        }
    }

    @Test
    fun staleConsentDoesNotPublishOverNewerFlowAndLaterIntentIsRetained() {
        PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        val first = PhoneControlProjectionDispatches.reserveConsent()
        val olderSequence = PhoneControlProjectionDispatches.currentIntentSequence()
        PhoneControlProjectionDispatches.releaseConsent(first)
        PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        val second = PhoneControlProjectionDispatches.reserveConsent()
        try {
            var publications = 0
            assertEquals(
                DetachedProjectionReentry.SUPERSEDED,
                PhoneControlProjectionDispatches.preserveDetachedReentry(
                    olderSequence, Intent("old"),
                ) { publications += 1; true },
            )
            PhoneControlProjectionDispatches.releaseConsent(first)
            assertEquals(0, publications)
            assertTrue(PhoneControlProjectionDispatches.ownsConsent(second))

            PhoneControlProjectionDispatches.inspectOrphanReentry(null)
            val laterSequence = PhoneControlProjectionDispatches.currentIntentSequence()
            assertEquals(
                DetachedProjectionReentry.RETAINED,
                PhoneControlProjectionDispatches.preserveDetachedReentry(
                    laterSequence, Intent("later"),
                ) { false },
            )
            assertTrue(PhoneControlProjectionDispatches.hasDetachedReentry())
            assertTrue(PhoneControlProjectionDispatches.ownsConsent(second))
            assertEquals(
                PhoneControlStopAdmission.Accepted,
                PhoneControlProjectionDispatches.beginStop(PhoneControlProjectionDispatches.stopBoundary()),
            )
            PhoneControlProjectionDispatches.endStop()
            assertFalse(PhoneControlProjectionDispatches.hasDetachedReentry())
            assertEquals(
                DetachedProjectionReentry.SUPERSEDED,
                PhoneControlProjectionDispatches.preserveDetachedReentry(laterSequence, Intent("later")) { true },
            )
        } finally {
            PhoneControlProjectionDispatches.endStop()
            PhoneControlProjectionDispatches.releaseConsent(second)
            PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        }
    }

    @Test
    fun retainedLaterIntentSurvivesNewerConsentAndAutomaticReentry() {
        PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        val consent = PhoneControlProjectionDispatches.reserveConsent()
        PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        val sequence = PhoneControlProjectionDispatches.currentIntentSequence()
        assertEquals(
            DetachedProjectionReentry.RETAINED,
            PhoneControlProjectionDispatches.preserveDetachedReentry(sequence, Intent("later")) { false },
        )
        val dispatch = PhoneControlProjectionDispatches.create(consentGeneration = consent)
        try {
            assertFalse(PhoneControlProjectionDispatches.hasDetachedReentry())
            assertTrue(PhoneControlProjectionDispatches.stageReentry(dispatch, Intent("automatic"), false))
            assertTrue(PhoneControlProjectionDispatches.retireIfPending(dispatch))
            assertEquals("later", PhoneControlProjectionDispatches.claimReplay(dispatch)?.intent?.action)
        } finally {
            PhoneControlProjectionDispatches.finish(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
            PhoneControlProjectionDispatches.releaseConsent(consent)
            PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        }
    }

    @Test
    fun delayedStopIntentCannotStopANewerProjectionGeneration() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val delayedStop = phoneControlStopIntent(context, "test")
        val consent = PhoneControlProjectionDispatches.reserveConsent()
        val newer = PhoneControlProjectionDispatches.create(consentGeneration = consent)
        try {
            assertEquals(
                PhoneControlStopAdmission.Stale,
                PhoneControlProjectionDispatches.beginStop(delayedStop.getStringExtra(EXTRA_STOP_BOUNDARY)),
            )
            assertSame(newer, PhoneControlProjectionDispatches.current())
            val currentStop = phoneControlStopIntent(context, "test")
            assertEquals(
                PhoneControlStopAdmission.Accepted,
                PhoneControlProjectionDispatches.beginStop(currentStop.getStringExtra(EXTRA_STOP_BOUNDARY)),
            )
            PhoneControlProjectionDispatches.endStop()
            assertNull(PhoneControlProjectionDispatches.admit(newer.id))
        } finally {
            PhoneControlProjectionDispatches.endStop()
            PhoneControlProjectionDispatches.finish(newer)
            PhoneControlProjectionDispatches.claimReplay(newer)
        }
    }

    @Test
    fun orphanFallbackIsGuardedAndNewerIntentSupersedesIt() {
        var posted: Intent? = null
        var available = false
        val dispatch = PhoneControlProjectionDispatches.create {
            if (available) posted = Intent(it)
            available
        }
        try {
            assertTrue(PhoneControlProjectionDispatches.stageReentry(dispatch, Intent("cancel")))
            assertTrue(PhoneControlProjectionDispatches.retireIfPending(dispatch))
            assertFalse(PhoneControlProjectionDispatches.reapIfIdle(dispatch))
            assertSame(dispatch, PhoneControlProjectionDispatches.current())
            assertNull(posted)

            available = true
            assertTrue(PhoneControlProjectionDispatches.reapIfIdle(dispatch))
            assertNull(PhoneControlProjectionDispatches.current())
            val generation = requireNotNull(posted).getLongExtra(EXTRA_ORPHAN_PROJECTION_REENTRY, 0L)
            assertEquals(OrphanProjectionReentry.SUPERSEDED, PhoneControlProjectionDispatches.inspectOrphanReentry(null))
            assertEquals(OrphanProjectionReentry.STALE_UNOWNED, PhoneControlProjectionDispatches.inspectOrphanReentry(generation))
        } finally {
            PhoneControlProjectionDispatches.finish(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
            PhoneControlProjectionDispatches.inspectOrphanReentry(null)
        }
    }

    @Test
    fun multipleActivitiesReplayOnlyTheLatestIntentAfterTerminalReceipt() {
        val dispatch = PhoneControlProjectionDispatches.create()
        try {
            val first = PhoneControlUserStepState()
            val second = PhoneControlUserStepState()
            assertTrue(first.adoptProjectionDispatch(dispatch))
            assertTrue(second.adoptProjectionDispatch(dispatch))
            assertSame(dispatch, PhoneControlProjectionDispatches.admit(dispatch.id))
            assertTrue(PhoneControlProjectionDispatches.stageReentry(dispatch, Intent("old")))
            assertTrue(PhoneControlProjectionDispatches.stageReentry(dispatch, Intent("cancel")))
            assertFalse(PhoneControlProjectionDispatches.retireIfPending(dispatch))
            assertNull(PhoneControlProjectionDispatches.claimReplay(dispatch))

            PhoneControlProjectionDispatches.finish(dispatch)
            assertSame(dispatch, PhoneControlProjectionDispatches.current())
            assertEquals("cancel", PhoneControlProjectionDispatches.claimReplay(dispatch)?.intent?.action)
            assertNull(PhoneControlProjectionDispatches.claimReplay(dispatch))
            assertNull(PhoneControlProjectionDispatches.current())
        } finally {
            PhoneControlProjectionDispatches.finish(dispatch)
            PhoneControlProjectionDispatches.claimReplay(dispatch)
        }
    }
}
