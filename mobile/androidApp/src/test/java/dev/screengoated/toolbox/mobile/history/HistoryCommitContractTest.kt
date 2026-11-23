package dev.screengoated.toolbox.mobile.history

import java.io.File
import java.nio.file.Files
import java.nio.file.Paths
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

@OptIn(ExperimentalCoroutinesApi::class)
class HistoryCommitContractTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test
    fun sharedCommitCasesKeepVisibleDurableAndBackingStateConsistent() = runTest {
        val candidates = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures"))
        val fixture = candidates.map { it.resolve("history-ui/commit-boundaries.json") }.first(Files::exists)
        val cases = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString())
            .jsonObject.getValue("cases").jsonArray
        assertEquals(13, cases.size)
        for (element in cases) {
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val root = temporaryFolder.newFolder(name)
            val paths = HistoryPaths(root, File(root, "history.json"), File(root, "settings.json"),
                File(root, "media"), false)
            val delegate = historyTestPersistence(paths, Json)
            val initial = (0 until case.getValue("initial").jsonPrimitive.int).map { index ->
                val (fileName, proof) = delegate.createMedia(HistoryType.TEXT, "old-$index".toByteArray()).use { it.name to it.proof }
                HistoryItem(if (case["duplicate_ids"]?.jsonPrimitive?.boolean == true) 0 else index.toLong(),
                    "timestamp", HistoryType.TEXT, "old-$index", fileName, proof)
            }
            paths.databaseFile.writeText(Json.encodeToString(StoredHistoryDatabase(items = initial)))
            paths.settingsFile.writeText(Json.encodeToString(HistorySettings(
                maxItems = case["initial_limit"]?.jsonPrimitive?.int ?: 10, hasExplicitMaxItems = true,
            )))
            var durable = initial
            var writes = 0
            lateinit var repository: HistoryRepository
            val store = object : HistoryStorage by delegate {
                override fun saveDatabase(database: StoredHistoryDatabase): HistoryWriteResult {
                    // Database replacement is entered before visible publication or old-media cleanup.
                    assertEquals(name, durable, repository.state.value.items)
                    durable.forEach { assertEquals(if (it.text == "new") "new-source" else it.text,
                        File(paths.mediaDir, it.mediaPath).readText()) }
                    val accepted = case["write_results"]?.jsonArray?.get(writes)?.jsonPrimitive?.boolean
                        ?: case["write_success"]?.jsonPrimitive?.boolean ?: true
                    writes++
                    if (!accepted) return HistoryWriteResult.REJECTED
                    val result = delegate.saveDatabase(database)
                    assertEquals(name, HistoryWriteResult.COMMITTED, result)
                    durable = database.items
                    return result
                }
            }
            repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
            case.getValue("actions").jsonArray.forEach { action ->
                when (action.jsonPrimitive.content) {
                    "save" -> repository.saveText("new", "new-source")
                    "delete" -> repository.delete(initial.first().id)
                    "clear" -> repository.clearAll()
                    "prune" -> repository.updateMaxItems(10)
                    else -> error("Unknown shared history action")
                }
            }
            advanceUntilIdle()
            val state = repository.state.value
            if (case.getValue("actions").jsonArray.any { it.jsonPrimitive.content == "prune" }) {
                assertEquals(name, 10, state.maxItems)
                assertEquals(name, 10, delegate.loadSettings().maxItems)
            }
            assertEquals(name, case.getValue("expected_order").jsonArray.map { it.jsonPrimitive.content },
                state.items.map { it.text })
            assertEquals(name, case.getValue("expected_count").jsonPrimitive.int, state.items.size)
            assertEquals(name, case.getValue("expected_writes").jsonPrimitive.int, writes)
            assertEquals(name, case.getValue("expected_error").jsonPrimitive.boolean, state.storageError != null)
            assertEquals(name, durable, state.items)
            assertEquals(name, durable, delegate.loadDatabase().items)
            val media = paths.mediaDir.listFiles().orEmpty()
            assertEquals(name, case.getValue("expected_media").jsonPrimitive.int, media.size)
            assertEquals(name, durable.map { it.mediaPath }.toSet(), media.map { it.name }.toSet())
            durable.forEach { item ->
                assertTrue(name, delegate.mediaFile(item) != null)
                assertEquals(name, if (item.text == "new") "new-source" else item.text,
                    File(paths.mediaDir, item.mediaPath).readText())
            }
        }
    }
}
