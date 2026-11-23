package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PreparedResourceMutation
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.ResourceAuthorizationDecision
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.PreparedFileTarget
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import java.io.File
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ResourceScopedMutationTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    private fun prepared(): PreparedResourceMutation =
        PreparedFileTarget.prepare(File(temporaryFolder.root, "output.txt").absolutePath, 1024, NioTestFileTransactionIo())
            .use { PreparedResourceMutation("save_artifact", it) }

    @Test fun rejectedTargetNeverReachesMutationEdge() = runTest {
        var invoked = false
        val result = executeResourceScopedMutation("save_artifact", prepared(), { _, _ ->
            ResourceAuthorizationDecision(false, "ERR_FILE_TARGET_REQUEST_CONTRACT_REJECTED", "outside scope",
                buildJsonObject { put("authorized", false); put("original_unchanged", true) }, null)
        }) {
            invoked = true
            AndroidProviderResult.Success(buildJsonObject {})
        }
        assertFalse(invoked)
        val failure = result as AndroidProviderResult.Failure
        assertEquals("ERR_FILE_TARGET_REQUEST_CONTRACT_REJECTED", failure.code)
        assertTrue(failure.data.getValue("original_unchanged").jsonPrimitive.boolean)
    }

    @Test fun authorizedEvidenceSurvivesAnUncertainProviderFailureWithoutUpgradingIt() = runTest {
        val proposal = prepared()
        val result = executeResourceScopedMutation("save_artifact", proposal, { _, _ ->
            ResourceAuthorizationDecision(true, "ok", "scope",
                buildJsonObject { put("authorized", true); put("positive_verdicts", 2) }, proposal.targetLease)
        }) {
            AndroidProviderResult.Failure("hash_mismatch", "changed",
                data = buildJsonObject { put("retained_staging_path", "owned-stage") },
                effect = EffectCertainty.MAY_HAVE_OCCURRED)
        } as AndroidProviderResult.Failure
        val evidence = result.data.getValue("resource_scope_authorization").jsonObject
        assertTrue(evidence.getValue("authorized").jsonPrimitive.boolean)
        assertEquals(2, evidence.getValue("positive_verdicts").jsonPrimitive.content.toInt())
        val execution = providerResult(PhoneControlToolJobContext(1, "scope-test", 1), "save_artifact",
            "artifact_access", "android_app_api", true, result)
        assertEquals("may_have_occurred", execution.response.string("effect_status"))
        assertEquals(false, execution.response.boolean("effect_verified"))
        assertEquals("owned-stage", execution.response.string("retained_staging_path"))
    }

    @Test fun missingOrDifferentApprovedLeaseCannotReachMutationEdge() = runTest {
        val proposal = prepared()
        for (missing in listOf(true, false)) {
            var invoked = false
            val result = executeResourceScopedMutation("save_artifact", proposal, { _, _ ->
                ResourceAuthorizationDecision(true, "ok", "scope", buildJsonObject {},
                    if (missing) null else proposal.targetLease.copy(canonicalPath = File(temporaryFolder.root, "other.txt").absolutePath))
            }) {
                invoked = true
                AndroidProviderResult.Success(buildJsonObject {})
            } as AndroidProviderResult.Failure
            assertFalse(invoked)
            assertEquals(if (missing) "ERR_FILE_TARGET_AUTHORIZATION_LEASE_MISSING" else "ERR_FILE_TARGET_AUTHORIZATION_LEASE_CHANGED", result.code)
        }
    }

    @Test fun revisedAuthorityCannotReachMutationEdge() = runTest {
        val proposal = prepared()
        var invoked = false
        val result = executeResourceScopedMutation("save_artifact", proposal, { _, _ ->
            ResourceAuthorizationDecision(true, "ok", "prior scope", buildJsonObject {}, proposal.targetLease, isCurrent = { false })
        }) {
            invoked = true
            AndroidProviderResult.Success(buildJsonObject {})
        } as AndroidProviderResult.Failure
        assertFalse(invoked)
        assertEquals("ERR_FILE_TARGET_AUTHORIZATION_LEASE_CHANGED", result.code)
    }
}
