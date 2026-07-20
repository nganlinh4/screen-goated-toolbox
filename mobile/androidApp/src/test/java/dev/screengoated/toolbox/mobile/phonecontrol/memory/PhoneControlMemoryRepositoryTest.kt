package dev.screengoated.toolbox.mobile.phonecontrol.memory

import java.io.File
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class PhoneControlMemoryRepositoryTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    @Test
    fun `complete finalize temp recovers and rebuilds missing index`() {
        val root = newRoot()
        val original = repository(root).append("session", turn("turn-1", 10L))
        val finalized = original.copy(revision = original.revision + 1L, finalizedAtEpochMs = 20L)
        val sessionFile = File(File(root, "sessions"), sessionFileName("session"))
        File(sessionFile.parentFile, sessionFile.name + ".tmp").writeText(
            JSON.encodeToString(finalized),
            Charsets.UTF_8,
        )
        File(root, "index.json").delete()

        val recovered = repository(root)

        assertEquals(listOf("session"), recovered.list().map { it.sessionId })
        assertEquals(finalized.records, recovered.get("session")?.records)
        assertFalse(File(sessionFile.parentFile, sessionFile.name + ".tmp").exists())
        assertTrue(File(root, "index.json").isFile)
    }

    @Test
    fun `partial temps are discarded without replacing durable state`() {
        val root = newRoot()
        val repo = repository(root)
        repo.append("session", turn("turn-1", 10L))
        val sessionFile = File(File(root, "sessions"), sessionFileName("session"))
        File(sessionFile.parentFile, sessionFile.name + ".tmp").writeText("{\"revision\":")
        val orphan = File(sessionFile.parentFile, sessionFileName("orphan") + ".tmp")
        orphan.writeText("not-json")

        val recovered = repository(root)
        val updated = recovered.append("session", turn("turn-2", 20L))

        assertEquals(4, updated.records.size)
        assertEquals(listOf("turn-1", "turn-1", "turn-2", "turn-2"), updated.records.map { it.turnId })
        assertFalse(File(sessionFile.parentFile, sessionFile.name + ".tmp").exists())
        assertFalse(orphan.exists())
    }

    @Test
    fun `one corrupt sidecar and corrupt index do not hide healthy sessions`() {
        val root = newRoot()
        val repo = repository(root)
        appendAndFinalize(repo, "healthy", 10L)
        appendAndFinalize(repo, "broken", 20L)
        val brokenFile = File(File(root, "sessions"), sessionFileName("broken"))
        brokenFile.writeText("{broken")
        File(root, "index.json").writeText("[broken")

        val recovered = repository(root)

        assertEquals(listOf("healthy"), recovered.list().map { it.sessionId })
        assertNull(recovered.get("broken"))
        assertTrue(File(File(root, "corrupt"), brokenFile.name).isFile)
        assertTrue(File(root, "index.json").readText().startsWith("{"))
    }

    @Test
    fun `unknown schema is preserved and never mistaken for corruption`() {
        val root = newRoot()
        repository(root).append("future", turn("turn", 10L))
        val futureFile = File(File(root, "sessions"), sessionFileName("future"))
        futureFile.writeText(
            futureFile.readText().replaceFirst("\"schemaVersion\":1", "\"schemaVersion\":99"),
        )

        assertThrows(PhoneControlMemorySchemaException::class.java) { repository(root) }
        assertTrue(futureFile.isFile)
        assertFalse(File(File(root, "corrupt"), futureFile.name).exists())

        val indexRoot = newRoot()
        repository(indexRoot)
        val futureIndex = File(indexRoot, "index.json")
        futureIndex.writeText(
            futureIndex.readText().replaceFirst("\"schemaVersion\":1", "\"schemaVersion\":99"),
        )
        assertThrows(PhoneControlMemorySchemaException::class.java) { repository(indexRoot) }
        assertTrue(futureIndex.readText().contains("\"schemaVersion\":99"))
    }

    @Test
    fun `missing schema is quarantined instead of silently adopting the current version`() {
        val root = newRoot()
        repository(root).append("missing-schema", turn("turn", 10L))
        val sidecar = File(File(root, "sessions"), sessionFileName("missing-schema"))
        sidecar.writeText(sidecar.readText().replaceFirst("\"schemaVersion\":1,", ""))

        val recovered = repository(root)

        assertNull(recovered.get("missing-schema"))
        assertTrue(File(File(root, "corrupt"), sidecar.name).isFile)
    }

    @Test
    fun `append order and structural roles win over timestamps and text prefixes`() {
        val root = newRoot()
        val repo = repository(root)
        repo.append(
            "older",
            turn(
                turnId = "turn-1",
                createdAt = 200L,
                assistantCreatedAt = 100L,
                userText = "Assistant: this is still user-authored",
                assistantText = "User: this is still assistant-authored",
            ),
        )
        repo.finalize("older", 300L)
        appendAndFinalize(repo, "newer", 400L)

        val stored = requireNotNull(repo.get("older"))

        assertEquals(listOf(0L, 1L), stored.records.map { it.ordinal })
        assertEquals(listOf(200L, 100L), stored.records.map { it.createdAtEpochMs })
        assertEquals("Assistant: this is still user-authored", repo.list().last().title)
        assertEquals(listOf("newer", "older"), repo.list().map { it.sessionId })
    }

    @Test
    fun `retention keeps only newest twenty finalized sidecars`() {
        val root = newRoot()
        val repo = repository(root)
        repeat(22) { index -> appendAndFinalize(repo, "session-$index", index.toLong()) }

        val summaries = repo.list(100)
        val sidecars = File(root, "sessions").listFiles { file -> file.extension == "json" }.orEmpty()

        assertEquals(20, summaries.size)
        assertEquals((21 downTo 2).map { "session-$it" }, summaries.map { it.sessionId })
        assertEquals(20, sidecars.size)
        assertFalse(File(File(root, "sessions"), sessionFileName("session-0")).exists())
        assertFalse(File(File(root, "sessions"), sessionFileName("session-1")).exists())
    }

    @Test
    fun `unicode survives sidecar index and search ready round trip`() {
        val root = newRoot()
        val text = "Xin chào — 안녕하세요 — 🧭✨"
        val repo = repository(root)
        repo.append(
            "unicode",
            turn("turn", 10L, userText = text),
        )
        repo.finalize("unicode", 20L)

        val reopened = repository(root)
        val search = reopened.searchReadyRecords().single()

        assertEquals(text, reopened.get("unicode")?.records?.first()?.text)
        assertEquals(text, reopened.list().single().title)
        assertTrue(search.searchText.startsWith(text + "\n"))
        assertEquals(
            listOf(PhoneControlMemoryRole.USER, PhoneControlMemoryRole.ASSISTANT),
            search.records.map { it.role },
        )

        val longUnicode = "x".repeat(79) + "🧭" + "tail"
        reopened.append(
            "unicode-boundary",
            turn("turn", 30L, userText = longUnicode),
        )
        reopened.finalize("unicode-boundary", 40L)
        assertEquals("x".repeat(79) + "🧭", reopened.get("unicode-boundary")?.let {
            reopened.list().first { summary -> summary.sessionId == it.sessionId }.title
        })
    }

    @Test
    fun `drafts are durable but invisible until finalized`() {
        val root = newRoot()
        repository(root).append("draft", turn("turn", 10L))

        val reopened = repository(root)
        assertTrue(reopened.list().isEmpty())
        assertNull(reopened.get("draft"))
        assertTrue(reopened.searchReadyRecords().isEmpty())

        reopened.finalize("draft", 20L)
        assertEquals("draft", reopened.list().single().sessionId)
        assertEquals("draft", reopened.get("draft")?.sessionId)
        assertEquals("draft", reopened.searchReadyRecords().single().summary.sessionId)
    }

    @Test
    fun `append retry is idempotent but conflicting turn is rejected`() {
        val root = newRoot()
        val repo = repository(root)
        val first = turn("turn", 10L)

        repo.append("session", first)
        val retried = repo.append("session", first)

        assertEquals(2, retried.records.size)
        assertThrows(IllegalArgumentException::class.java) {
            repo.append("session", turn("turn", 11L, userText = "different"))
        }
    }

    @Test
    fun `late user revision preserves the complete draft pair atomically`() {
        val root = newRoot()
        val repo = repository(root)
        val original = repo.append(
            "session",
            turn("turn", 10L, userText = "partial", assistantText = "answer"),
        )

        val revised = repo.reviseUserText("session", "turn", "complete transcript")

        assertEquals(original.revision + 1L, revised.revision)
        assertEquals("complete transcript", revised.records[0].text)
        assertEquals("answer", revised.records[1].text)
        assertEquals(original.records[0].copy(text = "complete transcript"), revised.records[0])
        assertEquals(original.records[1], revised.records[1])
        assertTrue(revised.records.containsOnlyCompleteTurns())
        assertTrue(repo.list().isEmpty())
        assertNull(repo.get("session"))

        val idempotent = repo.reviseUserText("session", "turn", "complete transcript")
        assertEquals(revised.revision, idempotent.revision)
        assertThrows(IllegalArgumentException::class.java) {
            repo.reviseUserText("session", "missing-turn", "text")
        }
        repo.finalize("session", 20L)
        assertThrows(IllegalArgumentException::class.java) {
            repo.reviseUserText("session", "turn", "too late")
        }
    }

    @Test
    fun `process start recovery drops incomplete tail and finalizes committed pairs`() {
        val root = newRoot()
        val repo = repository(root)
        repo.append("stale", turn("turn-1", 10L))
        val staleFile = File(File(root, "sessions"), sessionFileName("stale"))
        val draft = JSON.decodeFromString<PhoneControlMemorySession>(staleFile.readText())
        val incompleteTail = input(
            recordId = "interrupted-user",
            turnId = "turn-2",
            createdAt = 20L,
            role = PhoneControlMemoryRole.USER,
            text = "not committed",
        ).toStored(2L)
        staleFile.writeText(
            JSON.encodeToString(draft.copy(revision = draft.revision + 1L, records = draft.records + incompleteTail)),
        )
        val empty = PhoneControlMemorySession(
            revision = 0L,
            sessionId = "empty",
            startedAtEpochMs = 1L,
        )
        val emptyFile = File(File(root, "sessions"), sessionFileName("empty"))
        emptyFile.writeText(JSON.encodeToString(empty))

        val restarted = repository(root)
        assertTrue(restarted.list().isEmpty())
        val recovered = restarted.recoverStaleDrafts(50L)

        assertEquals(listOf("stale"), recovered.map { it.sessionId })
        assertEquals(2, restarted.get("stale")?.records?.size)
        assertFalse(restarted.get("stale")?.records.orEmpty().any { it.recordId == "interrupted-user" })
        assertFalse(emptyFile.exists())
        assertEquals(listOf("stale"), restarted.searchReadyRecords().map { it.summary.sessionId })
    }

    @Test
    fun `shared fixture owns retention visibility and screenshot policy`() {
        val fixture = JSON.parseToJsonElement(findRepoFile(FIXTURE_PATH).readText()).jsonObject
        val storage = fixture.getValue("storage").jsonObject
        val records = fixture.getValue("records").jsonObject
        val visibility = fixture.getValue("visibility").jsonObject
        val retrieval = fixture.getValue("retrieval").jsonObject

        assertEquals(PHONE_CONTROL_MEMORY_MAX_FINALIZED_SESSIONS, storage.getValue("maxFinalizedSessions").jsonPrimitive.int)
        assertTrue(storage.getValue("atomicReplace").jsonPrimitive.boolean)
        assertTrue(records.getValue("turnAndRoleAreStructural").jsonPrimitive.boolean)
        assertTrue(records.getValue("oneRecordPerTurnRole").jsonPrimitive.boolean)
        assertTrue(records.getValue("completeTurnPairAtomic").jsonPrimitive.boolean)
        assertEquals(
            "atomic_replace_existing_user_in_same_draft_pair",
            records.getValue("lateUserRevision").jsonPrimitive.content,
        )
        assertFalse(records.getValue("inferRoleFromText").jsonPrimitive.boolean)
        assertFalse(records.getValue("screenshotsPersisted").jsonPrimitive.boolean)
        assertFalse(visibility.getValue("draftsAreSearchVisible").jsonPrimitive.boolean)
        assertTrue(visibility.getValue("finalizedSessionsAreVisible").jsonPrimitive.boolean)
        assertEquals("finalized_sessions_only", retrieval.getValue("searchScope").jsonPrimitive.content)
        assertTrue(retrieval.getValue("offlineLexicalFallbackRequired").jsonPrimitive.boolean)
    }

    private fun newRoot(): File = temporaryFolder.newFolder()

    private fun repository(root: File): PhoneControlMemoryRepository {
        return PhoneControlMemoryRepository(root = root, json = JSON)
    }

    private fun appendAndFinalize(
        repo: PhoneControlMemoryRepository,
        sessionId: String,
        finalizedAt: Long,
    ) {
        repo.append(sessionId, turn("turn-$sessionId", finalizedAt))
        repo.finalize(sessionId, finalizedAt)
    }

    private fun input(
        recordId: String,
        turnId: String,
        createdAt: Long,
        role: PhoneControlMemoryRole = PhoneControlMemoryRole.USER,
        text: String = "text-$recordId",
    ): PhoneControlMemoryRecordInput {
        return PhoneControlMemoryRecordInput(recordId, turnId, role, text, createdAt)
    }

    private fun turn(
        turnId: String,
        createdAt: Long,
        assistantCreatedAt: Long = createdAt,
        userText: String = "user-$turnId",
        assistantText: String = "assistant-$turnId",
    ): PhoneControlMemoryTurnInput {
        return PhoneControlMemoryTurnInput(
            turnId = turnId,
            user = input("user-$turnId", turnId, createdAt, PhoneControlMemoryRole.USER, userText),
            assistant = input(
                "assistant-$turnId",
                turnId,
                assistantCreatedAt,
                PhoneControlMemoryRole.ASSISTANT,
                assistantText,
            ),
        )
    }

    private fun findRepoFile(path: String): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current -> current.parentFile }
            .map { root -> File(root, path) }
            .firstOrNull(File::isFile)
            ?: error("Could not locate $path from $workingDirectory")
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/phone-control/memory-contract.json"
        private val JSON = Json {
            encodeDefaults = true
            ignoreUnknownKeys = true
        }
    }
}
