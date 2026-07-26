package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReadySession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReceiveResult
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionPhase
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.channels.Channel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlRuntimeOutboundTest {
    @Test
    fun `microphone precedes control evidence and ambient pixels`() {
        val audio = Channel<ShortArray>(1)
        val screen = Channel<String>(1)
        val control = PhoneControlSessionPayloadQueue()
        val session = RecordingSession()
        val bufferedAudio = AtomicInteger(1)
        val audioSent = AtomicLong()
        val screenSent = AtomicLong()
        val inputSignals = AtomicInteger()
        val activitySignals = AtomicInteger()

        assertTrue(audio.trySend(shortArrayOf(1, -1)).isSuccess)
        assertTrue(control.offer("tool-response", PhoneControlOutboundKind.TOOL_RESPONSE))
        assertTrue(
            control.offer("tool-screen", PhoneControlOutboundKind.TOOL_SCREEN_EVIDENCE),
        )
        assertTrue(screen.trySend("ambient-screen").isSuccess)

        val outbound = PhoneControlRuntimeOutbound(
            visualEvidence = PhoneControlRuntimeVisualEvidence(),
            audioFrames = audio,
            bufferedAudio = bufferedAudio,
            controlPayloads = control,
            screenFrames = screen,
            screenReconciliationQueued = AtomicBoolean(false),
            sender = PhoneControlOutboundSender { 1L },
            audioFramesSent = audioSent,
            screenFramesSent = screenSent,
            pendingWorkCount = { 0 },
            turnPhase = { PhoneControlTurnPhase.LISTENING },
            userSpeaking = { false },
            userInterfaceGoals = PhoneControlUserInterfaceGoalQueue(),
            onInputSent = { inputSignals.incrementAndGet() },
            onInputActivity = { activitySignals.incrementAndGet() },
            onFreshScreenDelivered = {},
        )

        assertTrue(outbound.flush(session))
        assertTrue(session.sent.first().contains("\"realtimeInput\""))
        assertEquals("tool-response", session.sent[1])
        assertEquals("tool-screen", session.sent[2])
        assertEquals("ambient-screen", session.sent[3])
        assertEquals(0, bufferedAudio.get())
        assertEquals(1L, audioSent.get())
        assertEquals(1L, screenSent.get())
        assertEquals(2, inputSignals.get())
        assertEquals(1, activitySignals.get())
        assertEquals(0, control.snapshot().count)
    }

    private class RecordingSession : GeminiLiveReadySession {
        val sent = mutableListOf<String>()
        override val phase: GeminiLiveSessionPhase = GeminiLiveSessionPhase.ACTIVE

        override fun trySend(payload: String): Boolean {
            sent += payload
            return true
        }

        override suspend fun receive(timeoutMs: Long?): GeminiLiveReceiveResult =
            GeminiLiveReceiveResult.TimedOut

        override fun close() = Unit
    }
}
