package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionException
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionFailure
import java.io.IOException
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.Assert.assertSame
import org.junit.Assert.fail

class GeminiS2sTerminalFailureTest {
    @Test fun nonretryableBufferLossDoesNotEnterSegmentRetry() {
        val terminal = GeminiLiveSessionException(GeminiLiveSessionFailure.Server("buffered events discarded"))
        val transient = GeminiLiveSessionException(GeminiLiveSessionFailure.Server("temporary", retryable = true))
        assertTrue(terminalSegmentFailure(terminal))
        assertFalse(terminalSegmentFailure(transient))
        assertFalse(terminalSegmentFailure(IOException("temporary")))
    }

    @Test fun fatalLoserCannotAbortWinnerButWinningOrUnresolvedFatalIsReported() {
        val error = GeminiLiveSessionException(GeminiLiveSessionFailure.Server("buffer loss"))
        val beforeWinner = HedgeTerminalFailures()
        beforeWinner.record(null, 0, error)
        beforeWinner.finish(1)
        val losing = HedgeTerminalFailures()
        losing.record(1, 0, error)
        losing.finish(1)
        for (action in listOf<() -> Unit>(
            { HedgeTerminalFailures().record(0, 0, error) },
            { HedgeTerminalFailures().also { it.record(null, 0, error) }.finish(null) },
        )) {
            try { action(); fail("expected fatal failure") } catch (caught: GeminiLiveSessionException) { assertSame(error, caught) }
        }
    }
}
