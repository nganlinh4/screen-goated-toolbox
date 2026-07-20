package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEvent
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePolicy
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReconnectReason
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionLifecycle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class VoiceActivityHangoverTest {
    @Test
    fun `sub-threshold noise neither starts nor extends voice activity`() {
        val activity = VoiceActivityHangover(ACTIVE_THRESHOLD, HANGOVER_MS)

        activity.observe(ACTIVE_THRESHOLD - 0.001f, 100L)
        assertFalse(activity.isActive(100L))

        activity.observe(ACTIVE_THRESHOLD, 200L)
        activity.observe(ACTIVE_THRESHOLD - 0.001f, 900L)
        assertTrue(activity.isActive(1_000L))
        assertFalse(activity.isActive(1_001L))
    }

    @Test
    fun `new and out-of-order speech frames preserve the newest hangover`() {
        val activity = VoiceActivityHangover(ACTIVE_THRESHOLD, HANGOVER_MS)

        assertTrue(activity.observe(0.03f, 1_000L))
        assertFalse(activity.observe(0.04f, 900L))
        assertTrue(activity.isActive(1_800L))
        assertFalse(activity.isActive(1_801L))

        assertFalse(activity.observe(0.03f, 1_700L))
        assertTrue(activity.isActive(2_500L))
        assertTrue(activity.observe(0.03f, 2_501L))
    }

    @Test
    fun `go away safe gap waits until speech hangover expires`() {
        val lifecycle = activeLifecycle()
        val activity = VoiceActivityHangover(ACTIVE_THRESHOLD, HANGOVER_MS)
        activity.observe(0.03f, 100L)
        lifecycle.updateWorkState(100L, activity.isActive(100L))
        lifecycle.reduce(
            200L,
            GeminiLiveLifecycleEvent.Frame(
                GeminiLiveLifecycleFrame(generation = 1L, goAwayTimeLeftMs = 5_000L),
            ),
        )

        lifecycle.updateWorkState(300L, activity.isActive(300L))
        assertTrue(lifecycle.reduce(300L, GeminiLiveLifecycleEvent.Tick).isEmpty())
        assertEquals(GeminiLiveLifecyclePhase.ACTIVE, lifecycle.state.phase)

        lifecycle.updateWorkState(901L, activity.isActive(901L))
        val effects = lifecycle.reduce(901L, GeminiLiveLifecycleEvent.Tick)
        assertReconnect(effects, GeminiLiveReconnectReason.GO_AWAY_SAFE_GAP)
        assertEquals(GeminiLiveLifecyclePhase.BACKING_OFF, lifecycle.state.phase)
    }

    @Test
    fun `go away deadline still forces reconnect during active speech`() {
        val lifecycle = activeLifecycle()
        val activity = VoiceActivityHangover(ACTIVE_THRESHOLD, HANGOVER_MS)
        lifecycle.reduce(
            200L,
            GeminiLiveLifecycleEvent.Frame(
                GeminiLiveLifecycleFrame(generation = 1L, goAwayTimeLeftMs = 1_000L),
            ),
        )
        activity.observe(0.03f, 1_100L)
        lifecycle.updateWorkState(1_200L, activity.isActive(1_200L))

        val effects = lifecycle.reduce(1_200L, GeminiLiveLifecycleEvent.Tick)
        assertReconnect(effects, GeminiLiveReconnectReason.GO_AWAY_DEADLINE)
        assertEquals(GeminiLiveLifecyclePhase.BACKING_OFF, lifecycle.state.phase)
    }

    private fun activeLifecycle(): GeminiLiveSessionLifecycle {
        return GeminiLiveSessionLifecycle(GeminiLiveLifecyclePolicy.agent()).apply {
            reduce(0L, GeminiLiveLifecycleEvent.Start)
            reduce(0L, GeminiLiveLifecycleEvent.SocketOpened(1L))
            reduce(
                0L,
                GeminiLiveLifecycleEvent.Frame(
                    GeminiLiveLifecycleFrame(generation = 1L, setupComplete = true),
                ),
            )
        }
    }

    private fun GeminiLiveSessionLifecycle.updateWorkState(atMs: Long, speaking: Boolean) {
        reduce(
            atMs,
            GeminiLiveLifecycleEvent.WorkState(
                pendingWorkCount = 0L,
                bufferedInputCount = 0L,
                userSpeaking = speaking,
            ),
        )
    }

    private fun assertReconnect(
        effects: List<GeminiLiveLifecycleEffect>,
        reason: GeminiLiveReconnectReason,
    ) {
        val reconnect = effects.filterIsInstance<GeminiLiveLifecycleEffect.ScheduleReconnect>()
            .single()
        assertEquals(reason, reconnect.reason)
        assertTrue(effects.any { it is GeminiLiveLifecycleEffect.CloseSocket })
    }

    private companion object {
        const val ACTIVE_THRESHOLD = 0.015f
        const val HANGOVER_MS = 800L
    }
}
