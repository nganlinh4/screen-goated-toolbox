package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.authorization.ResourceAuthorizationDecision
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.FileMutationTargetLease
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ResourceScopedMutationTest {
    @Test
    fun `rejected target never reaches mutation edge`() = runTest {
        var invoked = false

        val result = executeResourceScopedMutation(
            tool = "edit_text_file",
            arguments = buildJsonObject { put("path", "/outside.txt") },
            authorizer = { _, _ ->
                ResourceAuthorizationDecision(
                    authorized = false,
                    code = "ERR_FILE_TARGET_REQUEST_CONTRACT_REJECTED",
                    message = "target is outside scope",
                    data = buildJsonObject {
                        put("authorized", false)
                        put("original_unchanged", true)
                    },
                    targetLease = null,
                )
            },
        ) { _ ->
            invoked = true
            AndroidProviderResult.Success(buildJsonObject {})
        }

        assertFalse(invoked)
        assertTrue(result is AndroidProviderResult.Failure)
        result as AndroidProviderResult.Failure
        assertEquals("ERR_FILE_TARGET_REQUEST_CONTRACT_REJECTED", result.code)
        assertTrue(result.data.getValue("original_unchanged").jsonPrimitive.boolean)
    }

    @Test
    fun `authorized target evidence survives provider failure`() = runTest {
        val result = executeResourceScopedMutation(
            tool = "save_artifact",
            arguments = buildJsonObject { put("path", "/output.txt") },
            authorizer = { _, _ ->
                ResourceAuthorizationDecision(
                    authorized = true,
                    code = "ok",
                    message = "target is in scope",
                    data = buildJsonObject {
                        put("authorized", true)
                        put("positive_verdicts", 2)
                    },
                    targetLease = missingTargetLease(),
                )
            },
        ) { _ ->
            AndroidProviderResult.Failure("path_exists", "already exists")
        }

        assertTrue(result is AndroidProviderResult.Failure)
        result as AndroidProviderResult.Failure
        val evidence = result.data.getValue("resource_scope_authorization").jsonObject
        assertTrue(evidence.getValue("authorized").jsonPrimitive.boolean)
        assertEquals(2, evidence.getValue("positive_verdicts").jsonPrimitive.content.toInt())
    }

    @Test
    fun `authorized verdict without a target lease cannot reach mutation edge`() = runTest {
        var invoked = false

        val result = executeResourceScopedMutation(
            tool = "save_artifact",
            arguments = buildJsonObject { put("path", "/output.txt") },
            authorizer = { _, _ ->
                ResourceAuthorizationDecision(
                    authorized = true,
                    code = "ok",
                    message = "malformed authorization",
                    data = buildJsonObject { put("authorized", true) },
                    targetLease = null,
                )
            },
        ) { _ ->
            invoked = true
            AndroidProviderResult.Success(buildJsonObject {})
        }

        assertFalse(invoked)
        assertTrue(result is AndroidProviderResult.Failure)
        result as AndroidProviderResult.Failure
        assertEquals("ERR_FILE_TARGET_AUTHORIZATION_LEASE_MISSING", result.code)
        assertTrue(result.data.getValue("original_unchanged").jsonPrimitive.boolean)
    }

    private fun missingTargetLease() = FileMutationTargetLease(
        canonicalPath = "/output.txt",
        existedBefore = false,
        expectedSha256 = null,
    )
}
