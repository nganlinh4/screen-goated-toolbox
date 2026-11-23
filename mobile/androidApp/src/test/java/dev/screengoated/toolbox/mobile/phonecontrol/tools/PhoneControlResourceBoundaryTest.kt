package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlAuthorityFixture
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlResourceAuthorizer
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.ResourceAuthorizationDecision
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PreparedResourceMutation
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderRouter
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.*
import org.junit.Test

class PhoneControlResourceBoundaryTest {
    private class Harness : AutoCloseable {
        val world = ExactResourceTestWorld()
        val job = PhoneControlToolJobContext(1, "boundary-job", 1)
        var dispatches = 0
        val proposals = mutableListOf<JsonObject>()
        var observed = JsonObject(emptyMap())
        var declaration: (String) -> JsonObject? = { name -> buildJsonObject {
            put("name", JsonPrimitive(name)); put("parameters", JsonObject(emptyMap()))
        } }
        var decide: suspend (JsonObject) -> ResourceAuthorizationDecision = { decision(true) }
        val boundary = PhoneControlResourceBoundary(
            PhoneControlExactEditGuard(world), object : PhoneControlResourceAuthorizer {
                override suspend fun evaluate(tool: String, mutation: PreparedResourceMutation): ResourceAuthorizationDecision =
                    error("Dedicated target checks belong to the executor")
                override suspend fun evaluateBoundary(proposal: JsonObject): ResourceAuthorizationDecision {
                    proposals += proposal
                    return decide(proposal)
                }
            }, { declaration(it) }, { observed },
        )
        val dispatcher = PhoneControlToolDispatcher(
            executor = PhoneControlHandlerExecutor { handler, actualJob, tool, _ ->
                dispatches += 1
                val spec = PhoneControlToolRegistry.byName.getValue(tool)
                PhoneControlToolExecution(toolResponse(
                    actualJob, tool, spec.capability, spec.providerIds.first(), CapabilityState.READY,
                    if (tool == "edit_text_file") "ERR_TEXT_FILE_MATCH_MISSING" else "ok", 0,
                    if (tool == "edit_text_file") EffectCertainty.PROVEN_NO_EFFECT else EffectCertainty.MAY_HAVE_OCCURRED,
                    snapshotInvalidated = handler.mutating,
                ), mutating = handler.mutating)
            },
            providerRouter = PhoneControlAuthorityFixture.load().let { ProviderRouter(it.providers, it.routes) },
            failureReporter = PhoneControlToolFailureReporter { _, _, error -> throw error },
            resourceBoundary = boundary,
        )
        suspend fun seed() { dispatcher.dispatch(job, "edit_text_file", world.args()) }
        override fun close() { dispatcher.close(); assertEquals(0, world.opened) }
    }

    @Test fun deniedOrUnverifiedScopeDispatchesNoAdditionalEffect() = runTest {
        for (code in listOf("ERR_EXACT_EDIT_EFFECT_SCOPE_REJECTED", "ERR_EXACT_EDIT_EFFECT_SCOPE_UNVERIFIED")) {
            Harness().use { h ->
                h.seed()
                h.decide = { decision(false, code) }
                val result = h.dispatcher.dispatch(h.job, "run_command", buildJsonObject { put("command", JsonPrimitive("inspect-resource")) })
                assertEquals(code, result.response.string("code"))
                assertEquals("proven_no_effect", result.response.string("effect_status"))
                assertEquals(1, h.dispatches)
            }
        }
    }

    @Test fun completeProcessAndAliasEvidenceReachAFreshCheckerOnEveryRetry() = runTest {
        Harness().use { h ->
            h.seed()
            val args = buildJsonObject {
                put("program", JsonPrimitive("resource-tool")); put("cwd", JsonPrimitive("/work"))
                put("args", kotlinx.serialization.json.JsonArray(listOf(JsonPrimitive("result.csv"))))
            }
            repeat(2) {
                val result = h.dispatcher.dispatch(h.job, "run_command", args)
                assertEquals("may_have_occurred", result.response.string("effect_status"))
                assertEquals(false, result.response.boolean("effect_verified"))
                assertNotNull(result.response["resource_boundary_authorization"])
            }
            assertEquals(3, h.dispatches)
            assertEquals(2, h.proposals.size)
            val proposal = h.proposals.first()
            assertEquals(args, proposal.getValue("proposed_action").jsonObject["arguments"])
            val evidence = proposal.getValue("evidence_untrusted").jsonObject
            assertEquals("run_command", evidence.getValue("tool_declaration").jsonObject.string("name"))
            assertEquals("/work", evidence.getValue("process").jsonObject.string("cwd"))
            assertNotNull(proposal["protected_resources"])
        }
    }

    @Test fun missingMismatchedAndOversizedDeclarationsCannotReachDispatch() = runTest {
        for (mode in 0..2) {
            Harness().use { h ->
                h.seed()
                h.declaration = { if (mode == 0) null else buildJsonObject {
                    put("name", JsonPrimitive(if (mode == 1) "different_tool" else "browser_eval"))
                    put("parameters", JsonObject(emptyMap()))
                } }
                val result = h.dispatcher.dispatch(h.job, "browser_eval", buildJsonObject {
                    put("expression", JsonPrimitive(if (mode == 2) "x".repeat(100 * 1024) else "performOperation()"))
                })
                assertEquals("ERR_EXACT_EDIT_EFFECT_SCOPE_UNVERIFIED", result.response.string("code"))
                assertEquals(1, h.dispatches)
                assertTrue(h.proposals.isEmpty())
            }
        }
    }

    @Test fun cancellationRetirementAndObservedDriftAfterApprovalNeverDispatch() = runTest {
        for (mode in 0..2) {
            Harness().use { h ->
                h.seed()
                h.decide = {
                    when (mode) {
                        0 -> h.job.effectOwner.requestCancellation()
                        1 -> h.dispatcher.turnInterrupted(1)
                        else -> h.observed = buildJsonObject { put("revision", JsonPrimitive(2)) }
                    }
                    decision(true)
                }
                val result = h.dispatcher.dispatch(h.job, "act", JsonObject(emptyMap()))
                assertEquals("proven_no_effect", result.response.string("effect_status"))
                assertEquals(1, h.dispatches)
            }
        }
    }

    @Test fun readsStayAvailableAndArtifactSavesCannotSkipTheAlternateEffectCheck() = runTest {
        Harness().use { h ->
            h.seed()
            h.dispatcher.dispatch(h.job, "observe", JsonObject(emptyMap()))
            assertTrue(h.proposals.isEmpty())
            h.decide = { decision(false, "ERR_EXACT_EDIT_EFFECT_SCOPE_REJECTED") }
            val result = h.dispatcher.dispatch(h.job, "save_artifact", h.world.args())
            assertEquals("ERR_EXACT_EDIT_EFFECT_SCOPE_REJECTED", result.response.string("code"))
            assertEquals(2, h.dispatches)
            assertNotNull(h.proposals.single().getValue("evidence_untrusted").jsonObject["dedicated_file_target"])
        }
    }

    companion object {
        private fun decision(authorized: Boolean, code: String = "ok") = ResourceAuthorizationDecision(
            authorized, code, "Independent scope result", buildJsonObject { put("authorized", JsonPrimitive(authorized)) }, null,
        )
    }
}
