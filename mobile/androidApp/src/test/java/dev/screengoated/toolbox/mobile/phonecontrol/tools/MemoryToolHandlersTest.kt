package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRecord
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRole
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemorySearchRecord
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemorySession
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemorySummary
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class MemoryToolHandlersTest {
    @Test
    fun `search ranks Unicode-normalized lexical relevance before recency`() {
        val backend = FakeMemoryBackend(
            listOf(
                searchRecord("newer", 3_000, "unrelated subject"),
                searchRecord("older", 1_000, "Ａｌｐｈａ planning details"),
            ),
        )

        val result = MemoryToolHandlers(backend).searchMemory(JOB, query("alpha planning"))
        val hits = result.response.getValue("results").jsonArray

        assertEquals("ok", result.response.text("code"))
        assertEquals(1, hits.size)
        assertEquals("older", hits.single().jsonObject.text("id"))
    }

    @Test
    fun `blank query lists newest finalized sessions without language heuristics`() {
        val backend = FakeMemoryBackend(
            listOf(
                searchRecord("new", 9_000, "second"),
                searchRecord("old", 1_000, "first"),
            ),
        )

        val result = MemoryToolHandlers(backend).searchMemory(JOB, query(""))
        val hits = result.response.getValue("results").jsonArray

        assertEquals(listOf("new", "old"), hits.map { it.jsonObject.text("id") })
        assertTrue(hits.first().jsonObject.getValue("score").jsonPrimitive.content.toDouble() > 0.0)
    }

    @Test
    fun `search returns an honest empty result when text does not match`() {
        val backend = FakeMemoryBackend(listOf(searchRecord("one", 1_000, "alpha")))

        val result = MemoryToolHandlers(backend).searchMemory(JOB, query("omega"))

        assertEquals(0, result.response.getValue("results").jsonArray.size)
        assertEquals("no matching past conversation", result.response.text("note"))
    }

    @Test
    fun `open returns the complete structurally labeled transcript`() {
        val session = session("memory-id", 4_000, "Xin chào", "Chào bạn")
        val backend = FakeMemoryBackend(emptyList(), mapOf(session.sessionId to session))

        val result = MemoryToolHandlers(backend).openMemory(
            JOB,
            buildJsonObject { put("id", session.sessionId) },
        )

        assertEquals("ok", result.response.text("code"))
        assertEquals("User: Xin chào\nAssistant: Chào bạn", result.response.text("transcript"))
    }

    @Test
    fun `open never invents a missing conversation`() {
        val result = MemoryToolHandlers(FakeMemoryBackend(emptyList())).openMemory(
            JOB,
            buildJsonObject { put("id", "missing") },
        )

        assertEquals("memory_not_found", result.response.text("code"))
        assertEquals("proven_no_effect", result.response.text("effect_status"))
    }

    private class FakeMemoryBackend(
        private val records: List<PhoneControlMemorySearchRecord>,
        private val sessions: Map<String, PhoneControlMemorySession> = emptyMap(),
    ) : MemoryToolBackend {
        override fun searchReadyRecords(): List<PhoneControlMemorySearchRecord> = records
        override fun get(sessionId: String): PhoneControlMemorySession? = sessions[sessionId]
    }

    private companion object {
        val JOB = PhoneControlToolJobContext(1, "memory-job", 1)

        fun query(value: String): JsonObject = buildJsonObject { put("query", value) }

        fun searchRecord(
            id: String,
            finalizedAt: Long,
            text: String,
        ): PhoneControlMemorySearchRecord {
            val session = session(id, finalizedAt, text, "response")
            return PhoneControlMemorySearchRecord(
                summary = summary(session),
                records = session.records,
                searchText = session.records.joinToString("\n") { it.text },
            )
        }

        fun session(
            id: String,
            finalizedAt: Long,
            user: String,
            assistant: String,
        ) = PhoneControlMemorySession(
            revision = 1,
            sessionId = id,
            startedAtEpochMs = finalizedAt - 100,
            finalizedAtEpochMs = finalizedAt,
            records = listOf(
                PhoneControlMemoryRecord(ordinal = 0, recordId = "$id-u", turnId = "t", role = PhoneControlMemoryRole.USER, text = user, createdAtEpochMs = finalizedAt - 100),
                PhoneControlMemoryRecord(ordinal = 1, recordId = "$id-a", turnId = "t", role = PhoneControlMemoryRole.ASSISTANT, text = assistant, createdAtEpochMs = finalizedAt - 50),
            ),
        )

        fun summary(session: PhoneControlMemorySession) = PhoneControlMemorySummary(
            sessionId = session.sessionId,
            startedAtEpochMs = session.startedAtEpochMs,
            finalizedAtEpochMs = requireNotNull(session.finalizedAtEpochMs),
            recordCount = session.records.size,
            title = session.records.first().text,
            snippet = session.records.joinToString(" • ") { it.text },
        )
    }
}

private fun JsonObject.text(name: String): String = getValue(name).jsonPrimitive.content
