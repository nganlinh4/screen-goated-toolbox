package dev.screengoated.toolbox.mobile.phonecontrol.authorization

import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RequestContractCheckerTest {
    @Test
    fun `quorum needs two positives and stops at the first veto`() = runTest {
        val attempted = mutableListOf<String>()
        val checker = checker("first", "second", "third") { model ->
            attempted += model
            if (model == "second") VETO else APPROVE
        }

        val report = requireNotNull(checker.assess("instruction", "context"))

        assertFalse(report.authorized)
        assertEquals(REQUEST_CONTRACT_REJECTED, report.rejectionStatus)
        assertEquals(listOf("first", "second"), attempted)
    }

    @Test
    fun `history revision advances on every committed request change`() {
        val checker = checker("first") { APPROVE }
        assertNull(checker.history())

        checker.turnStarted(1, 1)
        checker.userTranscriptUpdated(1, "Update the designated output.")
        val first = requireNotNull(checker.history())
        assertTrue(checker.isCurrent(first.revision))

        checker.userRequestSubmitted(1, "Update the designated output and keep its header.")
        assertFalse(checker.isCurrent(first.revision))
        assertTrue("keep its header" in requireNotNull(checker.history()).requests.toString())
    }

    @Test
    fun `cached verdicts never outlive the history revision they judged`() {
        val cache = RequestContractVerdictCache<String>(capacity = 2)
        cache.put(revision = 1, key = "a", verdict = "approved")
        assertEquals("approved", cache.get(revision = 1, key = "a"))

        assertNull(cache.get(revision = 2, key = "a"))
        assertNull(cache.get(revision = 1, key = "a"))
    }

    @Test
    fun `the turn after an interruption continues the interrupted request`() {
        val checker = checker("first") { APPROVE }
        checker.turnStarted(1, 1)
        checker.userTranscriptUpdated(1, "Rename the report.")
        checker.turnInterrupted(1)
        assertNull("an interrupted request alone is never authority", checker.history())

        // The coordinator's barge-in order: interrupt the acting turn, then start the next.
        checker.turnStarted(2, 2)
        checker.userTranscriptUpdated(2, "Use today's date in the name.")

        assertEquals(listOf(1L, 2L), turnIds(checker))
    }

    @Test
    fun `lineage carries over one turn only and never after a completed turn`() {
        val checker = checker("first") { APPROVE }
        checker.turnStarted(1, 1)
        checker.userTranscriptUpdated(1, "Rename the report.")
        checker.turnInterrupted(1)
        checker.turnStarted(2, 2)
        checker.userTranscriptUpdated(2, "Use today's date in the name.")
        checker.turnCompleted(2, "Use today's date in the name.", "Done.")

        checker.turnStarted(3, 3)
        checker.userTranscriptUpdated(3, "Open the calendar.")

        assertEquals(listOf(3L), turnIds(checker))
    }

    private fun turnIds(checker: RequestContractChecker): List<Long> =
        requireNotNull(checker.history()).requests.map { request ->
            request.jsonObject.getValue("turn_id").jsonPrimitive.long
        }

    @Test
    fun `ordinary and structural checks share one history identity`() {
        val checker = checker("first") { APPROVE }
        checker.turnStarted(3, 3)
        checker.userTranscriptUpdated(3, "Update the designated output.")
        val history = requireNotNull(checker.history()).requests
        val proposal = buildJsonObject { put("path", "/tmp/output.csv") }

        val ordinary = requireNotNull(requestContractContext(history, "proposed_resource_mutation", proposal))
        val structural = requireNotNull(requestContractContext(history, "proposed_structural_change", proposal))

        assertEquals(ordinary.substringBefore("\"proposed_"), structural.substringBefore("\"proposed_"))
    }

    private fun checker(vararg models: String, answer: (String) -> String) = RequestContractChecker(
        modelIds = { models.toList() },
        candidateClient = RequestContractCandidateClient { model, _, _ -> Result.success(answer(model)) },
    )

    private companion object {
        const val APPROVE = """{"authorized":true,"reason":"in scope"}"""
        const val VETO = """{"authorized":false,"reason":"out of scope"}"""
    }
}
