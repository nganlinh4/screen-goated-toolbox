package dev.screengoated.toolbox.mobile.history

import java.io.File
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

@OptIn(ExperimentalCoroutinesApi::class)
class HistoryCommitFailureTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test
    fun failedWritesPreserveCommittedItemsAndBackingBytes() = runTest {
        val failures = mutableListOf<String>()
        for (action in listOf("save", "delete", "clear", "prune", "startup")) {
            val paths = paths(action)
            val count = if (action == "prune" || action == "startup") 11 else 1
            val items = (0 until count).map { index ->
                HistoryItem(index.toLong(), "timestamp", HistoryType.TEXT, "old-$index", "$index.txt")
                    .also { File(paths.mediaDir, it.mediaPath).writeText(it.text) }
            }
            paths.databaseFile.writeText(Json.encodeToString(StoredHistoryDatabase(items = items)))
            paths.settingsFile.writeText(Json.encodeToString(HistorySettings(
                maxItems = if (action == "startup") 10 else 20,
                hasExplicitMaxItems = true,
            )))
            val original = paths.databaseFile.readBytes().toList()
            val delegate = historyTestPersistence(paths, Json)
            assertEquals(items, delegate.loadDatabase().items)
            val store = object : HistoryStorage by delegate {
                override fun saveDatabase(database: StoredHistoryDatabase) = HistoryWriteResult.REJECTED
            }
            val repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
            advanceUntilIdle()
            when (action) {
                "save" -> repository.saveText("new", "source")
                "delete" -> repository.delete(items.single().id)
                "clear" -> repository.clearAll()
                "prune" -> repository.updateMaxItems(10)
            }
            advanceUntilIdle()
            if (repository.state.value.items != items) failures += "$action changed visible items"
            if (paths.databaseFile.readBytes().toList() != original) failures += "$action changed durable bytes"
            val actual = paths.mediaDir.listFiles().orEmpty().associate { it.name to it.readText() }
            if (actual != items.associate { it.mediaPath to it.text }) failures += "$action changed backing bytes"
        }
        assertEquals(emptyList<String>(), failures)
    }

    @Test
    fun malformedDatabaseIsNeverReplacedByNewHistory() = runTest {
        val paths = paths("malformed")
        paths.databaseFile.writeText("{unfinished")
        val repository = HistoryRepository(historyTestPersistence(paths, Json), StandardTestDispatcher(testScheduler))
        repository.saveText("new", "source")
        repository.clearAll()
        advanceUntilIdle()
        assertEquals("{unfinished", paths.databaseFile.readText())
    }

    @Test
    fun deferredStartupDoesNotRewriteSettingsDuringRead() {
        val paths = paths("deferred")
        val original = Json.encodeToString(HistorySettings(maxItems = 200))
        paths.settingsFile.writeText(original)
        HistoryRepository(historyTestPersistence(paths, Json), deferStartupPrune = true)
        assertEquals(original, paths.settingsFile.readText())
    }

    @Test
    fun rejectedSettingsWriteNeverChangesRetentionPolicy() = runTest {
        val paths = paths("settings")
        val delegate = historyTestPersistence(paths, Json)
        val store = object : HistoryStorage by delegate {
            override fun saveSettings(settings: HistorySettings) = HistoryWriteResult.REJECTED
        }
        val repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
        repository.updateMaxItems(10)
        advanceUntilIdle()
        assertEquals(DEFAULT_HISTORY_LIMIT, repository.state.value.maxItems)
    }

    private fun paths(name: String): HistoryPaths {
        val root = temporaryFolder.newFolder(name)
        return HistoryPaths(root, File(root, "history.json"), File(root, "settings.json"),
            File(root, "media").apply { mkdirs() }, false)
    }
}
