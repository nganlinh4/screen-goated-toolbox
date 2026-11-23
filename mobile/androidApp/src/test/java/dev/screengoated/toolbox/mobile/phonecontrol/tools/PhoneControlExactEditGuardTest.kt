package dev.screengoated.toolbox.mobile.phonecontrol.tools

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.*
import org.junit.Test

class PhoneControlExactEditGuardTest {
    private val world = ExactResourceTestWorld()
    private val guard = PhoneControlExactEditGuard(world)
    private val job = ExactResourceTestWorld.JOB

    private fun record(tool: String = "edit_text_file", path: String = world.path,
                       result: JsonObject = world.reject()): JsonObject {
        assertNull(guard.beforeAction(job, tool, world.args(path)))
        return guard.recordResult(job, tool, world.args(path), result)
    }

    @Test fun sharedCapabilityCasesPreserveTheCanonicalAssessmentBoundary() {
        record()
        for (item in ExactResourceTestWorld.fixture("exact-resource-boundary.json").getValue("cases").jsonArray) {
            val case = item.jsonObject
            val tool = case.getValue("tool").jsonPrimitive.content
            assertEquals(case.getValue("id").jsonPrimitive.content,
                case.getValue("needsAssessment").jsonPrimitive.boolean,
                guard.needsScopeCheck(tool, resourceEffectful(tool)))
        }
        guard.close()
        assertEquals(0, world.opened)
    }

    @Test fun hardLinksShareMissBudgetAndAStableReadRearmsBothNames() {
        world.entries[world.alias] = world.entries.getValue(world.path).copy(tail = "alias.csv")
        record()
        record(path = world.alias)
        assertEquals("ERR_TEXT_FILE_REREAD_REQUIRED", guard.beforeAction(job, "edit_text_file", world.args())?.code)
        record("read_text_file", world.alias, world.receipt())
        assertNull(guard.beforeAction(job, "edit_text_file", world.args()))
        assertTrue(guard.needsScopeCheck("run_command", true))
        guard.close()
        assertEquals(0, world.opened)
    }

    @Test fun atomicReplacementClearsOnlyTheSelectedDirectoryEntry() {
        world.entries[world.alias] = world.entries.getValue(world.path).copy(tail = "alias.csv")
        record()
        record(path = world.alias)
        record("read_text_file", world.path, world.receipt())
        assertNull(guard.beforeAction(job, "save_artifact", world.args()))
        world.entries[world.path] = world.entries.getValue(world.path).copy(file = "inode-two")
        val result = guard.recordResult(job, "save_artifact", world.args(), world.receipt())
        assertTrue(result.getValue("resource_boundary_reconciled").jsonPrimitive.boolean)
        assertEquals(1, guard.protectedResources().size)
        assertTrue(guard.needsScopeCheck("browser_eval", true))
        guard.close()
        assertEquals(0, world.opened)
    }

    @Test fun inPlaceCreditRequiresTheSameObjectAndReconcilesOnlyCurrentAliases() {
        world.entries[world.alias] = world.entries.getValue(world.path).copy(tail = "alias.csv")
        record()
        record(path = world.alias)
        record("read_text_file", world.path, world.receipt())
        assertNull(guard.beforeAction(job, "save_artifact", world.args()))
        world.entries[world.path] = world.entries.getValue(world.path).copy(file = "replacement")
        val wrong = guard.recordResult(job, "save_artifact", world.args(), world.receipt(atomic = false))
        assertFalse(wrong.getValue("resource_boundary_reconciled").jsonPrimitive.boolean)
        assertEquals(2, guard.protectedResources().size)
        record("save_artifact", world.alias, world.receipt(atomic = false))
        assertEquals(1, guard.protectedResources().size)
        guard.close()
    }

    @Test fun staleOrMissingDigestsCannotEarnReadOrMutationCredit() {
        record()
        record()
        record("read_text_file", result = world.receipt(hash = "b".repeat(64)))
        assertEquals("ERR_TEXT_FILE_REREAD_REQUIRED", guard.beforeAction(job, "edit_text_file", world.args())?.code)
        record("save_artifact", result = world.receipt(hash = null))
        assertEquals(1, guard.protectedResources().size)
        guard.close()
    }

    @Test fun unknownIdentityCannotClearOrBypassAnActiveBoundary() {
        world.unavailable += world.path
        record()
        assertEquals("ERR_EXACT_EDIT_RESOURCE_IDENTITY_UNAVAILABLE", guard.beforeAction(job, "save_artifact", world.args())?.code)
        world.unavailable.clear()
        record("save_artifact", result = world.receipt())
        assertTrue(guard.needsScopeCheck("future_mutation", true))
        guard.close()
        assertEquals(0, world.opened)
    }

    @Test fun caseDistinctNamesKeepSeparateEntriesEvenWhenTheyShareAnObject() {
        world.entries[world.alias] = world.entries.getValue(world.path).copy(tail = "Resource.csv")
        record()
        record(path = world.alias)
        record("read_text_file", world.path, world.receipt())
        assertNull(guard.beforeAction(job, "save_artifact", world.args()))
        world.entries[world.path] = world.entries.getValue(world.path).copy(file = "replacement")
        guard.recordResult(job, "save_artifact", world.args(), world.receipt())
        assertEquals(1, guard.protectedResources().size)
        guard.close()
    }

    @Test fun structuralRejectionsHaveTheSameBoundedTerminalTransition() {
        record(result = world.reject("ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED"))
        record(result = world.reject("ERR_TEXT_FILE_STRUCTURE_CHANGE"))
        val limited = record(result = world.reject("ERR_TEXT_FILE_FORMULA_MIXED_EDIT"))
        assertEquals("ERR_TEXT_FILE_STRUCTURE_REJECTION_LIMIT", limited.getValue("code").jsonPrimitive.content)
        assertTrue(limited.getValue("terminal_blocker").jsonPrimitive.boolean)
        guard.close()
    }

    @Test fun receiptMustMatchThePendingToolPathAndCurrentEntry() {
        record()
        assertNull(guard.beforeAction(job, "save_artifact", world.args()))
        guard.recordResult(job, "edit_text_file", world.args(), world.receipt())
        assertEquals(1, guard.protectedResources().size)
        assertNull(guard.beforeAction(job, "save_artifact", world.args()))
        world.entries[world.path] = world.entries.getValue(world.path).copy(parent = "different-parent")
        guard.recordResult(job, "save_artifact", world.args(), world.receipt())
        assertEquals(1, guard.protectedResources().size)
        guard.close()
    }

    @Test fun duplicateStartDoesNotClearEvidenceAndRetiredOrStaleJobsCannotReopenIt() {
        record()
        guard.turnStarted(1, 2)
        assertTrue(guard.needsScopeCheck("act", true))
        guard.turnCompleted(1, "request", "response")
        assertEquals(0, world.opened)
        assertEquals("ERR_CANCELLED", guard.beforeAction(job, "edit_text_file", world.args())?.code)
        val newer = PhoneControlToolJobContext(2, "newer", 2)
        assertNull(guard.beforeAction(newer, "edit_text_file", world.args()))
        assertEquals("ERR_CANCELLED", guard.beforeAction(job, "edit_text_file", world.args())?.code)
        guard.close()
        assertEquals(0, world.opened)
        guard.turnStarted(3, 3)
        assertEquals("ERR_CANCELLED", guard.beforeAction(newer.copy(turnId = 3), "observe", JsonObject(emptyMap()))?.code)
    }
}
