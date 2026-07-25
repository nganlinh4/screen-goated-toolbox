package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlStructuralEditAuthorization
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.RequestContractCandidateClient
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.ResourceAuthorizationDecision
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidFileProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.ExactReplacement
import dev.screengoated.toolbox.mobile.phonecontrol.provider.FileMutationTargetLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.sha256
import java.io.File
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class StructuralEditToolCoordinatorTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    @Test
    fun `first structural call returns a token and never consults authorization`() = runTest {
        var candidateCalls = 0
        val scenario = scenario { _, _, _ ->
            candidateCalls += 1
            Result.success(positive())
        }

        val result = scenario.coordinator.execute(scenario.args(), scenario.request)

        assertFailure(result, "ERR_TEXT_FILE_STRUCTURE_CHANGE")
        assertEquals(0, candidateCalls)
        assertEquals(ORIGINAL, scenario.file.readText())
    }

    @Test
    fun `token with fewer than two positive verdicts cannot commit`() = runTest {
        val scenario = scenario(models = listOf("only")) { _, _, _ ->
            Result.success(positive())
        }
        scenario.authorization.recordRequest("remove the extra CSV column")
        val token = scenario.preflightToken()

        val result = scenario.coordinator.execute(scenario.args(token), scenario.request)

        val failure = assertFailure(
            result,
            "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_UNVERIFIED",
        )
        assertTrue(failure.data.getValue("original_unchanged").jsonPrimitive.boolean)
        assertEquals(ORIGINAL, scenario.file.readText())
    }

    @Test
    fun `any negative verdict blocks the private commit edge`() = runTest {
        val scenario = scenario { model, _, _ ->
            Result.success(
                if (model == "first") {
                    positive()
                } else {
                    """{"authorized":false,"reason":"request preserves structure"}"""
                },
            )
        }
        scenario.authorization.recordRequest("change values and preserve the CSV shape")
        val token = scenario.preflightToken()

        val result = scenario.coordinator.execute(scenario.args(token), scenario.request)

        assertFailure(
            result,
            "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED",
        )
        assertEquals(ORIGINAL, scenario.file.readText())
    }

    @Test
    fun `two positive verdicts commit once with verified authorization evidence`() = runTest {
        val scenario = scenario { _, _, _ -> Result.success(positive()) }
        scenario.authorization.recordRequest("remove the extra CSV column")
        val token = scenario.preflightToken()

        val result = scenario.coordinator.execute(
            scenario.args(token),
            scenario.request,
        ) as AndroidProviderResult.Success

        assertTrue(result.effectMayHaveOccurred)
        assertTrue(result.effectVerified)
        assertFalse(result.data.getValue("original_unchanged").jsonPrimitive.boolean)
        assertEquals(UPDATED, scenario.file.readText())
        val evidence = result.data.getValue("request_contract_authorization").jsonObject
        assertTrue(evidence.getValue("authorized").jsonPrimitive.boolean)
        val scope = result.data.getValue("resource_scope_authorization").jsonObject
        assertTrue(scope.getValue("authorized").jsonPrimitive.boolean)
    }

    private fun scenario(
        models: List<String> = listOf("first", "second"),
        candidate: suspend (String, String, String) -> Result<String>,
    ): Scenario {
        val file = temporaryFolder.newFile("table-${System.nanoTime()}.csv").apply {
            writeText(ORIGINAL)
        }
        val authorization = PhoneControlStructuralEditAuthorization(
            modelIds = { models },
            candidateClient = object : RequestContractCandidateClient {
                override suspend fun request(
                    modelId: String,
                    instruction: String,
                    context: String,
                ): Result<String> = candidate(modelId, instruction, context)
            },
        )
        val request = ExactEditRequest(
            path = file.absolutePath,
            expectedSha256 = file.readBytes().sha256(),
            replacements = listOf(ExactReplacement(ORIGINAL, UPDATED, 1)),
        )
        return Scenario(
            file = file,
            authorization = authorization,
            coordinator = StructuralEditToolCoordinator(
                AndroidFileProvider { null },
                authorization,
                resourceAuthorization = { _, _ ->
                    ResourceAuthorizationDecision(
                        authorized = true,
                        code = "ok",
                        message = "target is in scope",
                        data = buildJsonObject {
                            put("authorized", true)
                            put("positive_verdicts", 2)
                        },
                        targetLease = FileMutationTargetLease(
                            canonicalPath = file.canonicalPath,
                            existedBefore = true,
                            expectedSha256 = request.expectedSha256,
                        ),
                    )
                },
            ),
            request = request,
        )
    }

    private fun PhoneControlStructuralEditAuthorization.recordRequest(text: String) {
        turnStarted(1, 1)
        userTranscriptUpdated(1, text)
    }

    private fun assertFailure(
        result: AndroidProviderResult,
        code: String,
    ): AndroidProviderResult.Failure {
        assertTrue(result is AndroidProviderResult.Failure)
        return (result as AndroidProviderResult.Failure).also {
            assertEquals(code, it.code)
        }
    }

    private data class Scenario(
        val file: File,
        val authorization: PhoneControlStructuralEditAuthorization,
        val coordinator: StructuralEditToolCoordinator,
        val request: ExactEditRequest,
    ) {
        fun args(token: String? = null): JsonObject = buildJsonObject {
            put("path", request.path)
            put("expected_sha256", request.expectedSha256)
            token?.let { put("structural_change_token", it) }
            put(
                "replacements",
                buildJsonArray {
                    request.replacements.forEach { replacement ->
                        add(buildJsonObject {
                            put("old_text", replacement.oldText)
                            put("new_text", replacement.newText)
                            put("expected_count", replacement.expectedCount)
                        })
                    }
                },
            )
        }

        suspend fun preflightToken(): String {
            val result = coordinator.execute(args(), request) as AndroidProviderResult.Failure
            return result.data.getValue("structural_change_token").jsonPrimitive.content
        }
    }

    private fun positive() =
        """{"authorized":true,"reason":"explicit structural request"}"""

    private companion object {
        const val ORIGINAL = "name,value,extra\nalpha,1,x\n"
        const val UPDATED = "name,value\nalpha,1\n"
    }
}
