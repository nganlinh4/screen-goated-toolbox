package dev.screengoated.toolbox.mobile.phonecontrol.tools

import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlRepeatFailureGuardTest {
    @Test
    fun `third structurally equivalent proven no effect failure is blocked`() {
        val guard = PhoneControlRepeatFailureGuard(retryLimit = 2)
        val arguments = buildJsonObject {
            put("verb", "click")
            put("id", 17)
        }
        val reordered = buildJsonObject {
            put("id", 17)
            put("verb", "click")
        }
        val fingerprint = guard.fingerprint(4L, "act", arguments, "9:12")

        guard.observe(fingerprint, failure("stale_target"))
        assertFalse(guard.isBlocked(guard.fingerprint(4L, "act", reordered, "9:12")))
        guard.observe(fingerprint, failure("stale_target"))

        assertTrue(guard.isBlocked(guard.fingerprint(4L, "act", reordered, "9:12")))
        assertEquals("stale_target", guard.failureCode(fingerprint))
    }

    @Test
    fun `fresh observation does not inherit a prior surface failure`() {
        val guard = PhoneControlRepeatFailureGuard(retryLimit = 1)
        val arguments = buildJsonObject { put("cell", 7) }
        val stale = guard.fingerprint(4L, "click_at", arguments, "9:12")
        guard.observe(stale, failure("stale_frame"))

        assertFalse(
            guard.isBlocked(
                guard.fingerprint(4L, "click_at", arguments, "10:13"),
            ),
        )
    }

    @Test
    fun `new turn different request and reconciled success clear prior failures`() {
        val guard = PhoneControlRepeatFailureGuard(retryLimit = 1)
        val first = guard.fingerprint(9L, "act", buildJsonObject { put("id", 1) }, "3:4")
        guard.observe(first, failure("action_rejected"))

        assertFalse(
            guard.isBlocked(
                guard.fingerprint(9L, "act", buildJsonObject { put("id", 2) }, "3:4"),
            ),
        )
        guard.observe(
            first,
            buildJsonObject {
                put("code", "ok")
                put("effect_status", "proven_no_effect")
                put("state_reconciled", true)
            },
        )
        assertFalse(guard.isBlocked(first))

        guard.observe(first, failure("action_rejected"))
        assertFalse(
            guard.isBlocked(
                guard.fingerprint(10L, "act", buildJsonObject { put("id", 1) }, "3:4"),
            ),
        )
    }

    private fun failure(code: String) = buildJsonObject {
        put("code", code)
        put("effect_status", "proven_no_effect")
        put("state_reconciled", false)
    }
}
