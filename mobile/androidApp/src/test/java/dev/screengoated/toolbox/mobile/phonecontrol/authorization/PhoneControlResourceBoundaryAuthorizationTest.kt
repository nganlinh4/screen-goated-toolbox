package dev.screengoated.toolbox.mobile.phonecontrol.authorization

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.*
import org.junit.Test

class PhoneControlResourceBoundaryAuthorizationTest {
    private fun proposal() = buildJsonObject {
        put("operation", "preserve_rejected_exact_resource_boundaries")
        put("proposed_action", buildJsonObject { put("tool", "future_mutation") })
    }
    private fun PhoneControlResourceAuthorization.request(text: String = "Continue the requested unrelated work.") {
        turnStarted(1, 1)
        userTranscriptUpdated(1, text)
    }

    @Test fun eachAlternateEffectUsesTwoDistinctFreshCandidatesAndCommittedHistory() = runTest {
        val contexts = mutableListOf<String>()
        val attempted = mutableListOf<String>()
        val policy = PhoneControlResourceAuthorization({ listOf("first", "first", "second") }, RequestContractCandidateClient { model, instruction, context ->
            assertEquals(RESOURCE_BOUNDARY_INSTRUCTION, instruction)
            contexts += context
            attempted += model
            Result.success("""{"authorized":true,"reason":"effects preserve the protected resource"}""")
        })
        policy.request()
        repeat(2) { assertTrue(policy.evaluateBoundary(proposal()).authorized) }
        assertEquals(listOf("first", "second", "first", "second"), attempted)
        assertTrue(contexts.all { it.contains("Continue the requested unrelated work.") && it.contains("proposed_resource_boundary_effect") })
    }

    @Test fun oneNegativeVetoesAndMissingHistoryDispatchesNoCandidate() = runTest {
        var calls = 0
        val policy = PhoneControlResourceAuthorization({ listOf("first", "second") }, RequestContractCandidateClient { model, _, _ ->
            calls += 1
            Result.success(if (model == "first") """{"authorized":true,"reason":"scope"}""" else """{"authorized":false,"reason":"protected effect"}""")
        })
        assertEquals("ERR_EXACT_EDIT_EFFECT_SCOPE_UNVERIFIED", policy.evaluateBoundary(proposal()).code)
        assertEquals(0, calls)
        policy.request()
        assertEquals("ERR_EXACT_EDIT_EFFECT_SCOPE_REJECTED", policy.evaluateBoundary(proposal()).code)
        assertEquals(2, calls)
    }

    @Test fun transientFailuresDoNotCacheAnUnknownDecision() = runTest {
        var available = false
        val policy = PhoneControlResourceAuthorization({ listOf("first", "second") }, RequestContractCandidateClient { _, _, _ ->
            if (available) Result.success("""{"authorized":true,"reason":"scope"}""") else Result.failure(IllegalStateException("provider unavailable"))
        })
        policy.request()
        assertFalse(policy.evaluateBoundary(proposal()).authorized)
        available = true
        assertTrue(policy.evaluateBoundary(proposal()).authorized)
    }

    @Test fun requestRevisionInvalidatesAnInFlightApproval() = runTest {
        lateinit var policy: PhoneControlResourceAuthorization
        var revised = false
        policy = PhoneControlResourceAuthorization({ listOf("first", "second") }, RequestContractCandidateClient { _, _, _ ->
            if (!revised) {
                revised = true
                policy.userTranscriptUpdated(1, "Preserve all output files.")
            }
            Result.success("""{"authorized":true,"reason":"prior scope"}""")
        })
        policy.request()
        assertEquals("ERR_EXACT_EDIT_EFFECT_SCOPE_UNVERIFIED", policy.evaluateBoundary(proposal()).code)
    }

    @Test fun candidateCancellationRemainsCancellation() = runTest {
        val policy = PhoneControlResourceAuthorization({ listOf("first", "second") }, RequestContractCandidateClient { _, _, _ ->
            throw CancellationException("cancelled")
        })
        policy.request()
        try {
            policy.evaluateBoundary(proposal())
            fail("Cancellation became a permission result")
        } catch (_: CancellationException) { }
    }
}
