package dev.screengoated.toolbox.mobile.shared.live

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class GeminiLiveLifecycleAdapterTest {
    @Test
    fun closedBeforeReady1008_isNotRetried() = runTest {
        val opened = mutableListOf<RejectingSession>()
        val adapter = adapter(closeCode = 1008, opened = opened)

        assertNull(adapter.ensureReady())

        assertEquals(GeminiLiveLifecyclePhase.FAILED, adapter.state.phase)
        assertEquals(1, opened.size)
        assertTrue("the rejected socket is closed", opened.single().closed)
    }

    @Test
    fun closedBeforeReady1007_isNotRetried() = runTest {
        val adapter = adapter(closeCode = 1007, opened = mutableListOf())
        adapter.ensureReady()
        assertEquals(GeminiLiveLifecyclePhase.FAILED, adapter.state.phase)
    }

    @Test
    fun closedBeforeReadyServerRestart_backsOffForReconnect() = runTest {
        val adapter = adapter(closeCode = 1011, opened = mutableListOf())
        adapter.ensureReady()
        assertEquals(GeminiLiveLifecyclePhase.BACKING_OFF, adapter.state.phase)
    }

    @Test
    fun closeCodeClassificationMatchesFatalSetupCloses() {
        for (code in listOf(1002, 1003, 1007, 1008, 1009, 1010)) {
            assertEquals(code.toString(), false, geminiLiveCloseRetryable(code))
        }
        for (code in listOf(null, 1000, 1001, 1006, 1011, 1013)) {
            assertEquals(code.toString(), true, geminiLiveCloseRetryable(code))
        }
    }

    private fun adapter(closeCode: Int, opened: MutableList<RejectingSession>) = GeminiLiveLifecycleAdapter(
        policy = GeminiLiveLifecyclePolicy.continuous(),
        clockMs = { 1_000L },
        openConnectedSession = { RejectingSession(closeCode).also { opened += it } },
        setupPayload = { "{}" },
    )

    private class RejectingSession(private val closeCode: Int) : GeminiLiveConnectedSession {
        var closed = false
        override val phase: GeminiLiveSessionPhase = GeminiLiveSessionPhase.AWAITING_SETUP

        override suspend fun activate(
            setupPayload: String,
            timeoutMs: Long,
            dispatchSetup: ((() -> Boolean) -> Boolean)?,
        ): GeminiLiveReadySession = throw GeminiLiveSessionException(
            GeminiLiveSessionFailure.ClosedBeforeReady(closeCode, "API key expired. Please renew the API key."),
        )

        override fun close() {
            closed = true
        }
    }
}
