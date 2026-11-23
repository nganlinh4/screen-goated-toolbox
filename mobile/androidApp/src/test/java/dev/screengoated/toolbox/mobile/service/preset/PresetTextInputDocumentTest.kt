package dev.screengoated.toolbox.mobile.service.preset

import java.io.File
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class PresetTextInputDocumentTest {
    @Test
    fun oldWindowCallbacksCannotReachReplacementOrImpersonateItsOwner() {
        val scope = TextInputDocumentScope()
        scope.open("first")
        assertTrue(scope.accepts("first",TextInputEnvelope("first","mic")))
        scope.retire()
        scope.open("second")
        for (body in listOf("submit:1:text","cancel","close_window","history_up:draft","mic","request_focus","input_exit_done","dragEnd:1,2")) {
            assertFalse(scope.accepts("first",TextInputEnvelope("first",body)))
            assertFalse(scope.accepts("first",TextInputEnvelope("second",body)))
            assertFalse(scope.accepts("second",TextInputEnvelope("first",body)))
        }
        assertEquals("second",scope.activeOwner)
        assertTrue(scope.accepts("second",TextInputEnvelope("second","mic")))
    }

    @Test
    fun closingAdmitsOnlyItsOwnExitAndRetirementAbsorbsLaterCallbacks() {
        val scope = TextInputDocumentScope()
        scope.open("owner")
        val exit = TextInputEnvelope("owner","input_exit_done")
        assertFalse(scope.accepts("owner",exit))
        assertEquals("owner",scope.startClosing())
        assertNull(scope.activeOwner)
        assertNull(scope.startClosing())
        assertTrue(scope.accepts("owner",exit))
        assertFalse(scope.accepts("owner",TextInputEnvelope("owner","submit:1:text")))
        assertNull(scope.retire())
        assertFalse(scope.accepts("owner",exit))
    }

    @Test
    fun submissionsKeepOwnerRevisionAndAllTextAfterTheRevisionDelimiter() {
        assertEquals(TextInputSubmission("owner","12","text:with:colons"),parseTextInputSubmission("owner","submit:12:text:with:colons"))
        assertEquals(TextInputSubmission("owner","18446744073709551615","text"),parseTextInputSubmission("owner","submit:18446744073709551615:text"))
        for (body in listOf("submit:text","submit:-1:text","submit:+1:text","submit:18446744073709551616:text","submit:1: ","history_up:text")) {
            assertNull(body,parseTextInputSubmission("owner",body))
        }
        assertNull(parseTextInputEnvelope("unqualified"))
        assertNull(parseTextInputEnvelope(":submit:1:text"))
    }

    @Test
    fun sharedMessageCasesPreserveTheirOwnedEnvelope() {
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .first { File(it,"parity-fixtures/preset-system/text-input-overlay.json").isFile }
        val fixture = JSONObject(File(root,"parity-fixtures/preset-system/text-input-overlay.json").readText())
        val cases = fixture.getJSONArray("message_cases")
        for (index in 0 until cases.length()) {
            val row = cases.getJSONObject(index)
            if (row.isNull("expected")) continue
            val body = row.getString("expected")
            assertEquals(TextInputEnvelope("owner",body),parseTextInputEnvelope("owner:$body"))
            if (body.startsWith("submit:")) {
                val submission = requireNotNull(parseTextInputSubmission("owner",body))
                assertEquals(row.getString("revision"),submission.revision)
                assertEquals(row.getString("text").trim(),submission.text)
            }
        }
    }

    @Test
    fun invalidDocumentOwnersFailBeforeOpening() {
        for (owner in listOf(""," ","ambiguous:owner")) {
            assertTrue(runCatching { TextInputDocumentScope().open(owner) }.isFailure)
        }
    }
}
