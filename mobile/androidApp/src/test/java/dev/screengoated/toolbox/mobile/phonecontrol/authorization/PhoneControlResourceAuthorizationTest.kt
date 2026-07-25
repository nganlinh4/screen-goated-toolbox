package dev.screengoated.toolbox.mobile.phonecontrol.authorization

import dev.screengoated.toolbox.mobile.phonecontrol.provider.sha256
import java.io.File
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class PhoneControlResourceAuthorizationTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    @Test
    fun `two distinct positive candidates authorize the exact target`() = runTest {
        val file = textFile("output.txt")
        val attempted = mutableListOf<String>()
        val authorization = authorization("first", "second", "unused") { model, _, _ ->
            attempted += model
            Result.success(positive("target is in scope"))
        }
        authorization.recordRequest("Update the designated output file.")

        val decision = authorization.evaluate("edit_text_file", editArgs(file))

        assertTrue(decision.authorized)
        assertEquals(listOf("first", "second"), attempted)
        assertEquals(2, decision.data.getValue("positive_verdicts").jsonPrimitive.int)
    }

    @Test
    fun `one negative candidate vetoes an earlier positive`() = runTest {
        val file = textFile("input.txt")
        val authorization = authorization("first", "second") { model, _, _ ->
            Result.success(
                if (model == "first") {
                    positive("target may be in scope")
                } else {
                    negative("the target is an input that must remain unchanged")
                },
            )
        }
        authorization.recordRequest("Read the input file without changing it.")

        val decision = authorization.evaluate("edit_text_file", editArgs(file))

        assertFalse(decision.authorized)
        assertEquals("ERR_FILE_TARGET_REQUEST_CONTRACT_REJECTED", decision.code)
        assertEquals(1, decision.data.getValue("negative_verdicts").jsonPrimitive.int)
    }

    @Test
    fun `ordinary and structural edits share one target identity without content`() = runTest {
        val file = textFile("table.csv")
        val contexts = mutableListOf<String>()
        val authorization = authorization("first", "second") { _, _, context ->
            contexts += context
            Result.success(positive("target is in scope"))
        }
        authorization.recordRequest("Update the designated table.")

        val ordinary = authorization.evaluate("edit_text_file", editArgs(file))
        val structural = authorization.evaluate("edit_text_file_structure", editArgs(file))

        assertTrue(ordinary.authorized)
        assertTrue(structural.authorized)
        assertEquals(2, contexts.size)
        assertEquals(1, contexts.distinct().size)
        assertFalse(contexts.first().contains("private-old"))
        assertFalse(contexts.first().contains("private-new"))
        assertTrue(structural.data.getValue("cached").jsonPrimitive.boolean)
    }

    @Test
    fun `interrupted request is never target authority`() = runTest {
        val file = textFile("output.txt")
        var candidateCalls = 0
        val authorization = authorization("first", "second") { _, _, _ ->
            candidateCalls += 1
            Result.success(positive("target is in scope"))
        }
        authorization.turnStarted(1, 1)
        authorization.userTranscriptUpdated(1, "Update the output file.")
        authorization.turnInterrupted(1)

        val decision = authorization.evaluate("edit_text_file", editArgs(file))

        assertFalse(decision.authorized)
        assertEquals("ERR_FILE_TARGET_REQUEST_CONTRACT_UNVERIFIED", decision.code)
        assertEquals(0, candidateCalls)
    }

    @Test
    fun `completed request scope does not leak into a new independent turn`() = runTest {
        val file = textFile("output.txt")
        var candidateCalls = 0
        val authorization = authorization("first", "second") { _, _, _ ->
            candidateCalls += 1
            Result.success(positive("target is in scope"))
        }
        authorization.turnStarted(1, 1)
        authorization.userTranscriptUpdated(1, "Update the output file.")
        authorization.turnCompleted(1, "Update the output file.", "Done.")
        authorization.turnStarted(2, 2)

        val decision = authorization.evaluate("edit_text_file", editArgs(file))

        assertFalse(decision.authorized)
        assertEquals("ERR_FILE_TARGET_REQUEST_CONTRACT_UNVERIFIED", decision.code)
        assertEquals(0, candidateCalls)
    }

    @Test
    fun `transiently unverified target is evaluated again`() = runTest {
        val file = textFile("output.txt")
        var available = false
        var candidateCalls = 0
        val authorization = authorization("first", "second") { _, _, _ ->
            candidateCalls += 1
            if (available) {
                Result.success(positive("target is in scope"))
            } else {
                Result.failure(IllegalStateException("provider unavailable"))
            }
        }
        authorization.recordRequest("Update the output file.")

        val first = authorization.evaluate("edit_text_file", editArgs(file))
        available = true
        val second = authorization.evaluate("edit_text_file", editArgs(file))

        assertFalse(first.authorized)
        assertTrue(second.authorized)
        assertEquals(4, candidateCalls)
    }

    @Test
    fun `save proposal distinguishes creation from replacement`() = runTest {
        val existing = textFile("existing.txt")
        val created = File(temporaryFolder.root, "new.txt")
        val contexts = mutableListOf<String>()
        val authorization = authorization("first", "second") { _, _, context ->
            contexts += context
            Result.success(positive("destination is in scope"))
        }
        authorization.recordRequest("Save both outputs in this folder.")

        assertTrue(authorization.evaluate("save_artifact", saveArgs(existing)).authorized)
        assertTrue(authorization.evaluate("save_artifact", saveArgs(created)).authorized)

        assertTrue(contexts.all { it.contains("\"capability_class\":\"dedicated_local_file_write\"") })
        assertTrue(contexts.any { it.contains("\"operation\":\"replace_existing_file\"") })
        assertTrue(contexts.any { it.contains("\"operation\":\"create_file\"") })
        assertTrue(contexts.none { it.contains("private-artifact-id") })
    }

    private fun textFile(name: String): File =
        temporaryFolder.newFile(name).apply { writeText("old") }

    private fun PhoneControlResourceAuthorization.recordRequest(text: String) {
        turnStarted(1, 1)
        userTranscriptUpdated(1, text)
    }

    private fun authorization(
        vararg models: String,
        request: suspend (String, String, String) -> Result<String>,
    ) = PhoneControlResourceAuthorization(
        modelIds = { models.toList() },
        candidateClient = object : RequestContractCandidateClient {
            override suspend fun request(
                modelId: String,
                instruction: String,
                context: String,
            ): Result<String> = request(modelId, instruction, context)
        },
    )

    private fun editArgs(file: File): JsonObject = buildJsonObject {
        put("path", file.absolutePath)
        put("expected_sha256", file.readBytes().sha256())
        put(
            "replacements",
            buildJsonArray {
                add(buildJsonObject {
                    put("old_text", "private-old")
                    put("new_text", "private-new")
                    put("expected_count", 1)
                })
            },
        )
    }

    private fun saveArgs(file: File): JsonObject = buildJsonObject {
        put("id", "private-artifact-id")
        put("path", file.absolutePath)
        put("overwrite", true)
    }

    private fun positive(reason: String) =
        """{"authorized":true,"reason":"$reason"}"""

    private fun negative(reason: String) =
        """{"authorized":false,"reason":"$reason"}"""
}
