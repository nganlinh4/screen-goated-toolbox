package dev.screengoated.toolbox.mobile.phonecontrol.authorization

import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlStructuralEditAuthorizationTest {
    @Test
    fun `two distinct positive candidates authorize the exact proposal`() = runTest {
        val attempted = mutableListOf<String>()
        val authorization = authorization("first", "second", "unused") { model, _, _ ->
            attempted += model
            Result.success(positive("explicit structural request"))
        }
        authorization.recordRequest("remove the third CSV column")

        val decision = authorization.evaluate(args(), preflight())

        assertTrue(decision.authorized)
        assertEquals("ok", decision.code)
        assertEquals(listOf("first", "second"), attempted)
        assertEquals(2, decision.data.getValue("positive_verdicts").jsonPrimitive.int)
        assertEquals(0, decision.data.getValue("negative_verdicts").jsonPrimitive.int)
    }

    @Test
    fun `one negative candidate vetoes an earlier positive`() = runTest {
        val authorization = authorization("first", "second", "unused") { model, _, _ ->
            Result.success(
                if (model == "first") {
                    positive("request appears explicit")
                } else {
                    negative("the request required formula preservation")
                },
            )
        }
        authorization.recordRequest("update the CSV data but preserve every formula")

        val decision = authorization.evaluate(args(), preflight())

        assertFalse(decision.authorized)
        assertEquals(
            "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED",
            decision.code,
        )
        assertEquals(1, decision.data.getValue("positive_verdicts").jsonPrimitive.int)
        assertEquals(1, decision.data.getValue("negative_verdicts").jsonPrimitive.int)
    }

    @Test
    fun `one positive plus malformed and failed checks stays unverified`() = runTest {
        val authorization = authorization("first", "second", "third") { model, _, _ ->
            when (model) {
                "first" -> Result.success(positive("explicit"))
                "second" -> Result.success("not json")
                else -> Result.failure(IllegalStateException("provider unavailable"))
            }
        }
        authorization.recordRequest("remove the third CSV column")

        val decision = authorization.evaluate(args(), preflight())

        assertFalse(decision.authorized)
        assertEquals(
            "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_UNVERIFIED",
            decision.code,
        )
        assertEquals(1, decision.data.getValue("positive_verdicts").jsonPrimitive.int)
        assertEquals(1, decision.data.getValue("malformed_verdicts").jsonPrimitive.int)
        assertEquals(1, decision.data.getValue("failed_attempts").jsonPrimitive.int)
    }

    @Test
    fun `thrown provider failure cannot bypass later independent checks`() = runTest {
        val authorization = authorization("first", "second", "third") { model, _, _ ->
            if (model == "first") {
                throw IllegalStateException("transport failed")
            }
            Result.success(positive("explicit"))
        }
        authorization.recordRequest("remove the third CSV column")

        val decision = authorization.evaluate(args(), preflight())

        assertTrue(decision.authorized)
        assertEquals(2, decision.data.getValue("positive_verdicts").jsonPrimitive.int)
        assertEquals(1, decision.data.getValue("failed_attempts").jsonPrimitive.int)
    }

    @Test
    fun `transiently unverified proposal is evaluated again`() = runTest {
        var available = false
        var candidateCalls = 0
        val authorization = authorization("first", "second") { _, _, _ ->
            candidateCalls += 1
            if (available) {
                Result.success(positive("explicit"))
            } else {
                Result.failure(IllegalStateException("provider unavailable"))
            }
        }
        authorization.recordRequest("remove the third CSV column")

        val first = authorization.evaluate(args(), preflight())
        available = true
        val second = authorization.evaluate(args(), preflight())

        assertFalse(first.authorized)
        assertTrue(second.authorized)
        assertEquals(4, candidateCalls)
    }

    @Test
    fun `interrupted request is never authorization evidence`() = runTest {
        var candidateCalls = 0
        val authorization = authorization("first", "second") { _, _, _ ->
            candidateCalls += 1
            Result.success(positive("explicit"))
        }
        authorization.turnStarted(1, 1)
        authorization.userTranscriptUpdated(1, "remove the third CSV column")
        authorization.turnInterrupted(1)

        val decision = authorization.evaluate(args(), preflight())

        assertFalse(decision.authorized)
        assertEquals(
            "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_UNVERIFIED",
            decision.code,
        )
        assertEquals(0, candidateCalls)
    }

    @Test
    fun `only the latest admitted turn and nested evidence are forwarded structurally`() = runTest {
        var capturedContext = ""
        val authorization = authorization("first", "second") { _, _, context ->
            capturedContext = context
            Result.success(positive("explicit"))
        }
        (1L..8L).forEach { turn ->
            authorization.turnStarted(turn, turn)
            authorization.userTranscriptUpdated(turn, "request-$turn")
            authorization.turnCompleted(turn, "request-$turn", "done")
        }

        val decision = authorization.evaluate(args(), preflight())

        assertTrue(decision.authorized)
        assertFalse(capturedContext.contains("\"turn_id\":1"))
        assertFalse(capturedContext.contains("\"turn_id\":7"))
        assertTrue(capturedContext.contains("\"turn_id\":8"))
        assertTrue(capturedContext.contains("\"format\":\"csv\""))
        assertTrue(capturedContext.contains("\"before_record_count\":2"))
        assertTrue(capturedContext.contains("\"after_record_count\":2"))
    }

    @Test
    fun `completed structural scope does not leak into a new independent turn`() = runTest {
        var candidateCalls = 0
        val authorization = authorization("first", "second") { _, _, _ ->
            candidateCalls += 1
            Result.success(positive("explicit"))
        }
        authorization.turnStarted(1, 1)
        authorization.userTranscriptUpdated(1, "remove the third CSV column")
        authorization.turnCompleted(1, "remove the third CSV column", "done")
        authorization.turnStarted(2, 2)

        val decision = authorization.evaluate(args(), preflight())

        assertFalse(decision.authorized)
        assertEquals(
            "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_UNVERIFIED",
            decision.code,
        )
        assertEquals(0, candidateCalls)
    }

    private fun PhoneControlStructuralEditAuthorization.recordRequest(text: String) {
        turnStarted(1, 1)
        userTranscriptUpdated(1, text)
    }

    private fun authorization(
        vararg models: String,
        request: suspend (String, String, String) -> Result<String>,
    ) = PhoneControlStructuralEditAuthorization(
        modelIds = { models.toList() },
        candidateClient = object : RequestContractCandidateClient {
            override suspend fun request(
                modelId: String,
                instruction: String,
                context: String,
            ): Result<String> = request(modelId, instruction, context)
        },
    )

    private fun args(): JsonObject = buildJsonObject {
        put("path", "/tmp/table.csv")
        put(
            "replacements",
            buildJsonArray {
                add(buildJsonObject {
                    put("old_text", "name,value,extra\nalpha,1,x\n")
                    put("new_text", "name,value\nalpha,1\n")
                    put("expected_count", 1)
                })
            },
        )
    }

    private fun preflight(): JsonObject = buildJsonObject {
        put(
            "structure",
            buildJsonObject {
                put("format", "csv")
                put("before_record_count", 2)
                put("after_record_count", 2)
                put("before_formula_count", 0)
                put("after_formula_count", 0)
            },
        )
    }

    private fun positive(reason: String) =
        """{"authorized":true,"reason":"$reason"}"""

    private fun negative(reason: String) =
        """{"authorized":false,"reason":"$reason"}"""
}
