package dev.screengoated.toolbox.mobile.phonecontrol

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlLogTest {
    @Test
    fun `persistent diagnostic fields preserve Unicode and flatten control characters`() {
        assertEquals(
            "bắt đầu 한국어 next",
            PhoneControlLog.normalizeDiagnosticField(" bắt đầu\n한국어\tnext ", 80),
        )
    }

    @Test
    fun `persistent diagnostic fields are bounded`() {
        assertEquals(
            "12345",
            PhoneControlLog.normalizeDiagnosticField("123456789", 5),
        )
    }

    @Test
    fun `diagnostic event parser keeps typed structure and drops prose`() {
        val parsed = PhoneControlLog.parseDiagnosticEvent(
            "tool_receipt name=observe generation=42 window_changes=12 " +
                "retryable=true ignored prose",
        )

        assertEquals("tool_receipt", parsed.name)
        assertEquals("observe", parsed.fields["name"])
        assertEquals(42L, parsed.fields["generation"])
        assertEquals(12L, parsed.fields["window_changes"])
        assertEquals(true, parsed.fields["retryable"])
        assertFalse(parsed.fields.containsKey("ignored"))
    }

    @Test
    fun `diagnostic event parser does not persist a free form message`() {
        val parsed = PhoneControlLog.parseDiagnosticEvent(
            "runtime_failed an arbitrary sentence with spaces",
        )

        assertEquals("runtime_failed", parsed.name)
        assertEquals(emptyMap<String, Any>(), parsed.fields)
    }

    @Test
    fun `unknown event cannot smuggle structured looking user text`() {
        val parsed = PhoneControlLog.parseDiagnosticEvent(
            "hello name=private token=secret",
        )

        assertEquals("diagnostic_event", parsed.name)
        assertEquals(emptyMap<String, Any>(), parsed.fields)
    }

    @Test
    fun `diagnostic event parser admits only typed structural fields`() {
        val parsed = PhoneControlLog.parseDiagnosticEvent(
            "tool_receipt name=observe failure_class=contract argument_field=target " +
                "contract_reason=invalid_surface_identity pairing_code=123456 token=secret " +
                "url=https://private.example code=123456 label=private",
        )

        assertEquals(
            mapOf(
                "name" to "observe",
                "failure_class" to "contract",
                "argument_field" to "target",
                "contract_reason" to "invalid_surface_identity",
            ),
            parsed.fields,
        )
    }

    @Test
    fun `browser cleanup remains a typed event`() {
        val parsed = PhoneControlLog.parseDiagnosticEvent(
            "browser_turn_cleanup requested_count=2 verified_count=2 unresolved_count=0",
        )

        assertEquals("browser_turn_cleanup", parsed.name)
        assertEquals(2L, parsed.fields["requested_count"])
        assertEquals(2L, parsed.fields["verified_count"])
        assertEquals(0L, parsed.fields["unresolved_count"])
    }

    @Test
    fun `setup session diagnostics preserve only structural admission state`() {
        val parsed = PhoneControlLog.parseDiagnosticEvent(
            "setup_session_state state=reset_requested input_admitted=false " +
                "announcement_pending=true private=discarded",
        )

        assertEquals("setup_session_state", parsed.name)
        assertEquals("reset_requested", parsed.fields["state"])
        assertEquals(false, parsed.fields["input_admitted"])
        assertEquals(true, parsed.fields["announcement_pending"])
        assertFalse(parsed.fields.containsKey("private"))
    }

    @Test
    fun `process roles own separate bounded journal pairs`() {
        assertEquals(
            "events.jsonl",
            PhoneControlDiagnosticProcessRole.PRIMARY.currentFileName,
        )
        assertEquals(
            "events.previous.jsonl",
            PhoneControlDiagnosticProcessRole.PRIMARY.previousFileName,
        )
        assertEquals(
            "events.authority-bridge.jsonl",
            PhoneControlDiagnosticProcessRole.AUTHORITY_BRIDGE.currentFileName,
        )
        assertEquals(
            "events.authority-bridge.previous.jsonl",
            PhoneControlDiagnosticProcessRole.AUTHORITY_BRIDGE.previousFileName,
        )
    }

    @Test
    fun `bridge terminal events retain outcomes but reject secrets`() {
        val parsed = PhoneControlLog.parseDiagnosticEvent(
            "pair_result result=connected provider=sgt_adb_bridge " +
                "pairing_established=true pairing_code=123456 endpoint=private",
        )

        assertEquals("pair_result", parsed.name)
        assertEquals("connected", parsed.fields["result"])
        assertEquals("sgt_adb_bridge", parsed.fields["provider"])
        assertEquals(true, parsed.fields["pairing_established"])
        assertFalse(parsed.fields.containsKey("pairing_code"))
        assertFalse(parsed.fields.containsKey("endpoint"))
    }

    @Test
    fun `console throwable summary omits the exception message`() {
        val error = IllegalStateException("private credential")
        error.stackTrace = arrayOf(
            StackTraceElement("dev.sgt.Example", "run", "Example.kt", 42),
        )

        val summary = PhoneControlLog.consoleSummary(
            "provider_failure tool=act ignored private prose",
            error,
        )

        assertTrue(summary.contains("provider_failure tool=act"))
        assertTrue(summary.contains("throwable_type=java.lang.IllegalStateException"))
        assertTrue(summary.contains("frame_1=dev.sgt.Example.run:42"))
        assertFalse(summary.contains("private credential"))
        assertFalse(summary.contains("private prose"))
    }
}
