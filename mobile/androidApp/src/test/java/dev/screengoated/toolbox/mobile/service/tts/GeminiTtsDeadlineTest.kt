package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveConnectedSession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReadySession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReceiveResult
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionPhase
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.atomic.AtomicLong
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class GeminiTtsDeadlineTest {
    @Test
    fun openSocketThatNeverCompletesTheTurnFailsAtTheDeadline() {
        val clock = AtomicLong(0L)
        val ready = SilentReadySession(onReceive = { clock.addAndGet(5_000L) })
        val provider = GeminiTtsProvider(
            openConnectedSession = { AcceptingConnectedSession(ready) },
            detectLanguage = { "eng" },
            elapsedRealtime = clock::get,
            launchBackground = { task -> task() },
            logTiming = {},
        )
        val sink = LinkedBlockingDeque<ProviderAudioEvent>()

        provider.stream("key", request(), isStale = { false }, sink = sink)

        assertEquals(listOf(ProviderAudioEvent.Error(GEMINI_TTS_TIMEOUT_MESSAGE)), sink.toList())
        assertEquals(1, ready.closeCount)
        // 90 s budget at 5 s per silent receive: bounded, not an endless loop.
        assertTrue(ready.receives in 17..19)
    }

    private fun request(): TtsRequest = TtsRequest(
        text = "hello",
        consumer = TtsConsumer.SETTINGS_PREVIEW,
        priority = TtsPriority.PREVIEW,
        requestMode = TtsRequestMode.NORMAL,
        settingsSnapshot = TtsRequestSettingsSnapshot(
            method = MobileTtsMethod.GEMINI_LIVE,
            geminiModel = "test-live-model",
            geminiVoice = "Aoede",
            speedPreset = MobileTtsSpeedPreset.NORMAL,
            languageConditions = emptyList(),
            edgeSettings = MobileEdgeTtsSettings(),
        ),
        ownerToken = "test",
    )

    private class AcceptingConnectedSession(
        private val ready: GeminiLiveReadySession,
    ) : GeminiLiveConnectedSession {
        override val phase: GeminiLiveSessionPhase = GeminiLiveSessionPhase.AWAITING_SETUP

        override suspend fun activate(
            setupPayload: String,
            timeoutMs: Long,
            dispatchSetup: ((() -> Boolean) -> Boolean)?,
        ): GeminiLiveReadySession {
            if (dispatchSetup != null && !dispatchSetup { true }) error("Setup rejected")
            return ready
        }

        override fun close() = Unit
    }

    private class SilentReadySession(private val onReceive: () -> Unit) : GeminiLiveReadySession {
        var closeCount = 0
            private set
        var receives = 0
            private set

        override val phase: GeminiLiveSessionPhase = GeminiLiveSessionPhase.ACTIVE

        override fun trySend(payload: String): Boolean = true

        override suspend fun receive(timeoutMs: Long?): GeminiLiveReceiveResult {
            receives += 1
            onReceive()
            return GeminiLiveReceiveResult.TimedOut
        }

        override fun close() {
            closeCount++
        }
    }
}
