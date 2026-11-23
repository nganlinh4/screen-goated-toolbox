package dev.screengoated.toolbox.mobile.service

import org.junit.Assert.assertEquals
import org.junit.Test

class SecureStoreForegroundStopTest {
    @Test
    fun acceptedStartPromotesBeforeStopping() {
        val events = mutableListOf<String>()
        settleUnavailableForeground(
            promote = { events += "promote" },
            remove = { events += "remove" },
            stop = { events += "stop" },
            failure = { events += "failure" },
        )
        assertEquals(listOf("promote", "remove", "stop"), events)
    }

    @Test
    fun rejectedPromotionStillStopsAndReportsFailure() {
        val events = mutableListOf<String>()
        settleUnavailableForeground(
            promote = { events += "promote"; throw SecurityException("denied") },
            remove = { events += "remove" },
            stop = { events += "stop" },
            failure = { events += "failure" },
        )
        assertEquals(listOf("promote", "failure", "stop"), events)
    }
}
