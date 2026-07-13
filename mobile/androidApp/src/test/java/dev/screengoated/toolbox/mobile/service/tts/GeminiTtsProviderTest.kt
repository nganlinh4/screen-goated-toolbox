package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveConnectedSession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReadySession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReceiveResult
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionException
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionFailure
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionPhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveWireFormat
import kotlinx.coroutines.awaitCancellation
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.ArrayDeque
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.atomic.AtomicLong

class GeminiTtsProviderTest {
    @Test
    fun `stale warm setup retries one fresh transport before text`() {
        val trace = mutableListOf<String>()
        val warm = FakeConnectedSession { _, _ ->
            trace += "warm-setup"
            throw GeminiLiveSessionException(
                GeminiLiveSessionFailure.ClosedBeforeReady(1001, "expired"),
            )
        }
        val freshReady = FakeReadySession(trace, "fresh-text", acceptText = true)
        val fresh = FakeConnectedSession { setup, _ ->
            assertTrue(setup.contains("\"setup\""))
            trace += "fresh-setup"
            freshReady
        }
        val provider = testProvider(ArrayDeque(listOf(warm, fresh)), trace)
        val sink = LinkedBlockingDeque<ProviderAudioEvent>()

        provider.warmUp(API_KEY)
        provider.stream(API_KEY, request(), isStale = { false }, sink = sink)

        assertEquals(
            listOf("open-1", "warm-setup", "open-2", "fresh-setup", "fresh-text"),
            trace,
        )
        assertEquals(listOf(ProviderAudioEvent.End), sink.toList())
        assertEquals(1, warm.closeCount)
        assertEquals(1, freshReady.closeCount)
    }

    @Test
    fun `rejected warm text send is checked and retried fresh only once`() {
        val trace = mutableListOf<String>()
        val warmReady = FakeReadySession(trace, "warm-text", acceptText = false)
        val warm = FakeConnectedSession { _, _ ->
            trace += "warm-setup"
            warmReady
        }
        val freshReady = FakeReadySession(trace, "fresh-text", acceptText = true)
        val fresh = FakeConnectedSession { _, _ ->
            trace += "fresh-setup"
            freshReady
        }
        val provider = testProvider(ArrayDeque(listOf(warm, fresh)), trace)
        val sink = LinkedBlockingDeque<ProviderAudioEvent>()

        provider.warmUp(API_KEY)
        provider.stream(API_KEY, request(), isStale = { false }, sink = sink)

        assertEquals(
            listOf(
                "open-1",
                "warm-setup",
                "warm-text",
                "open-2",
                "fresh-setup",
                "fresh-text",
            ),
            trace,
        )
        assertEquals(listOf(ProviderAudioEvent.End), sink.toList())
        assertEquals(1, warmReady.closeCount)
        assertEquals(1, freshReady.closeCount)
    }

    @Test
    fun `older warm completion cannot replace newer api key transport`() {
        val trace = mutableListOf<String>()
        val pendingTasks = mutableListOf<() -> Unit>()
        val old = FakeConnectedSession { _, _ -> error("old session must not be acquired") }
        val newReady = FakeReadySession(trace, "new-text", acceptText = true)
        val newer = FakeConnectedSession { _, _ ->
            trace += "new-setup"
            newReady
        }
        val clock = AtomicLong(1_000L)
        val provider = GeminiTtsProvider(
            openConnectedSession = { key ->
                trace += "open-$key"
                if (key == "old-key") old else newer
            },
            detectLanguage = { "eng" },
            elapsedRealtime = clock::getAndIncrement,
            launchBackground = pendingTasks::add,
            logTiming = {},
        )
        val sink = LinkedBlockingDeque<ProviderAudioEvent>()

        provider.warmUp("old-key")
        provider.warmUp("new-key")
        pendingTasks[1].invoke()
        pendingTasks[0].invoke()
        provider.stream("new-key", request(), isStale = { false }, sink = sink)

        assertEquals(
            listOf("open-new-key", "open-old-key", "new-setup", "new-text"),
            trace,
        )
        assertEquals(1, old.closeCount)
        assertEquals(listOf(ProviderAudioEvent.End), sink.toList())
    }

    @Test
    fun `fresh open failure emits exactly one error`() {
        val provider = GeminiTtsProvider(
            openConnectedSession = {
                throw GeminiLiveSessionException(GeminiLiveSessionFailure.OpenTimedOut)
            },
            detectLanguage = { "eng" },
            elapsedRealtime = { 1_000L },
            launchBackground = { task -> task() },
            logTiming = {},
        )
        val sink = LinkedBlockingDeque<ProviderAudioEvent>()

        provider.stream(API_KEY, request(), isStale = { false }, sink = sink)

        assertEquals(
            listOf(ProviderAudioEvent.Error("Gemini TTS websocket failed to open.")),
            sink.toList(),
        )
    }

    @Test
    fun `stale request cancels setup activation and closes warm transport`() {
        val trace = mutableListOf<String>()
        val warm = FakeConnectedSession { _, _ ->
            try {
                awaitCancellation()
            } finally {
                trace += "setup-cancelled"
            }
        }
        val provider = testProvider(ArrayDeque(listOf(warm)), trace)
        val sink = LinkedBlockingDeque<ProviderAudioEvent>()
        var staleChecks = 0

        provider.warmUp(API_KEY)
        provider.stream(
            API_KEY,
            request(),
            isStale = { ++staleChecks >= 4 },
            sink = sink,
        )

        assertEquals(listOf("open-1", "setup-cancelled"), trace)
        assertEquals(1, warm.closeCount)
        assertTrue(sink.isEmpty())
    }

    private fun testProvider(
        sessions: ArrayDeque<GeminiLiveConnectedSession>,
        trace: MutableList<String>,
    ): GeminiTtsProvider {
        val clock = AtomicLong(1_000L)
        var opens = 0
        return GeminiTtsProvider(
            openConnectedSession = {
                opens++
                trace += "open-$opens"
                sessions.removeFirst()
            },
            detectLanguage = { "eng" },
            elapsedRealtime = clock::getAndIncrement,
            launchBackground = { task -> task() },
            logTiming = {},
        )
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

    private class FakeConnectedSession(
        private val activateResult: suspend (String, Long) -> GeminiLiveReadySession,
    ) : GeminiLiveConnectedSession {
        var closeCount = 0
            private set

        override val phase: GeminiLiveSessionPhase = GeminiLiveSessionPhase.AWAITING_SETUP

        override suspend fun activate(
            setupPayload: String,
            timeoutMs: Long,
        ): GeminiLiveReadySession = activateResult(setupPayload, timeoutMs)

        override fun close() {
            closeCount++
        }
    }

    private class FakeReadySession(
        private val trace: MutableList<String>,
        private val textMarker: String,
        private val acceptText: Boolean,
    ) : GeminiLiveReadySession {
        var closeCount = 0
            private set
        private var completionPending = true

        override val phase: GeminiLiveSessionPhase = GeminiLiveSessionPhase.ACTIVE

        override fun trySend(payload: String): Boolean {
            assertTrue(payload.contains("realtimeInput"))
            trace += textMarker
            return acceptText
        }

        override suspend fun receive(timeoutMs: Long?): GeminiLiveReceiveResult {
            check(completionPending) { "Unexpected extra receive" }
            completionPending = false
            return GeminiLiveReceiveResult.Frame(
                frame = GeminiLiveServerFrame(turnComplete = true),
                wireFormat = GeminiLiveWireFormat.TEXT,
            )
        }

        override fun close() {
            closeCount++
        }
    }

    private companion object {
        private const val API_KEY = "test-key"
    }
}
