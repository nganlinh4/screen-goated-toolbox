package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetExecutionState
import dev.screengoated.toolbox.mobile.preset.PresetSessionOwnership
import dev.screengoated.toolbox.mobile.service.tts.TTS_REQUEST_REJECTED
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong
import kotlin.concurrent.thread
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetSpeechAdmissionTest {
    @Test
    fun blockedAdmissionDoesNotHoldThePlaybackLease() {
        val ownership = PresetSessionOwnership(MutableStateFlow(PresetExecutionState(sessionId = "one", isExecuting = true)))
        val lease = ownership.effectLease("one")
        val admissionStarted = CountDownLatch(1)
        val releaseAdmission = CountDownLatch(1)
        val playbackCheckedLease = CountDownLatch(1)
        val admitted = AtomicLong(-1)
        val audioMonitor = Any()
        val admission = thread(isDaemon = true) {
            admitted.set(enqueuePresetSpeechIfCurrent(lease) {
                admissionStarted.countDown()
                check(releaseAdmission.await(5, TimeUnit.SECONDS))
                42L
            })
        }
        try {
            assertTrue("admission did not start", admissionStarted.await(2, TimeUnit.SECONDS))
            val playback = thread(isDaemon = true) {
                synchronized(audioMonitor) {
                    lease.runIfCurrent { playbackCheckedLease.countDown() }
                }
            }
            assertTrue("playback could not check its lease while admission was blocked", playbackCheckedLease.await(2, TimeUnit.SECONDS))
            playback.join(2_000)
            assertFalse(playback.isAlive)
            ownership.transition { ownership.retireEffects() }
            assertFalse("retired speech must not dispatch", lease.runIfCurrent { error("stale dispatch") })
        } finally {
            releaseAdmission.countDown()
            admission.join(2_000)
        }
        assertFalse(admission.isAlive)
        assertEquals(42L, admitted.get())
    }

    @Test
    fun retiredLeaseRejectsAdmissionWithoutEnqueueing() {
        val ownership = PresetSessionOwnership(MutableStateFlow(PresetExecutionState(sessionId = "one", isExecuting = true)))
        val lease = ownership.effectLease("one")
        ownership.transition { ownership.retireEffects() }
        var called = false
        val requestId = enqueuePresetSpeechIfCurrent(lease) { called = true; 42L }
        assertEquals(TTS_REQUEST_REJECTED, requestId)
        assertFalse(called)
    }
}
