package dev.screengoated.toolbox.mobile.service

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class S2sPlaybackOwnershipTest {
    @Test
    fun retirementWaitsForAdmittedEffectAndRejectsLaterEffects() {
        val owner = S2sPlaybackOwnership()
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val retired = CountDownLatch(1)
        val effects = AtomicInteger()
        val writer = Thread {
            owner.dispatch {
                entered.countDown()
                check(release.await(2, TimeUnit.SECONDS))
                effects.incrementAndGet()
            }
        }
        writer.start()
        assertTrue(entered.await(2, TimeUnit.SECONDS))
        val stopper = Thread { owner.retire(); retired.countDown() }
        stopper.start()
        assertFalse(retired.await(50, TimeUnit.MILLISECONDS))
        release.countDown()
        assertTrue(retired.await(2, TimeUnit.SECONDS))
        writer.join()
        stopper.join()
        assertFalse(owner.dispatch { effects.incrementAndGet() })
        assertEquals(1, effects.get())
    }
}
