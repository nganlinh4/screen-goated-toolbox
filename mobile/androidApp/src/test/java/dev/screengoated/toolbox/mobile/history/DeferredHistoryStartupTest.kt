package dev.screengoated.toolbox.mobile.history

import java.io.File
import java.lang.reflect.Proxy
import android.content.SharedPreferences
import dev.screengoated.toolbox.mobile.storage.SecureBootstrap
import dev.screengoated.toolbox.mobile.storage.SecureSettingsStore
import kotlinx.serialization.encodeToString
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

@OptIn(ExperimentalCoroutinesApi::class)
class DeferredHistoryStartupTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    @Test
    fun `history stays unchanged until secure bootstrap completes`() = runTest {
        val root = temporaryFolder.newFolder("history")
        val media = File(root, "media").apply { mkdirs() }
        val database = File(root, "history.json")
        val settings = File(root, "settings.json")
        val entries = (1..11).map { id ->
            val name = "$id.txt"
            File(media, name).writeText("entry $id")
            HistoryItem(id.toLong(), "timestamp", HistoryType.TEXT, "entry $id", name)
        }
        val original = Json.encodeToString(StoredHistoryDatabase(items = entries))
        database.writeText(original)
        settings.writeText(Json.encodeToString(HistorySettings(maxItems = 10, hasExplicitMaxItems = true)))
        val repository = HistoryRepository(
            persistence = historyTestPersistence(
                HistoryPaths(root, database, settings, media, true),
                Json,
            ),
            ioDispatcher = StandardTestDispatcher(testScheduler),
            deferStartupPrune = true,
        )

        assertEquals(original, database.readText())
        assertEquals(11, repository.state.value.items.size)
        assertTrue(File(media, "11.txt").exists())
        repository.completeDeferredStartup()
        advanceUntilIdle()
        assertEquals(10, repository.state.value.items.size)
        assertTrue(File(media, "11.txt").exists())
    }

    @Test
    fun `late secure read failure never prunes existing history`() {
        val root = temporaryFolder.newFolder("failed-bootstrap")
        val media = File(root, "media").apply { mkdirs() }
        val database = File(root, "history.json")
        val settings = File(root, "settings.json")
        val entries = (1..11).map { id ->
            val name = "$id.txt"
            File(media, name).writeText("entry $id")
            HistoryItem(id.toLong(), "timestamp", HistoryType.TEXT, "entry $id", name)
        }
        val original = Json.encodeToString(StoredHistoryDatabase(items = entries))
        database.writeText(original)
        settings.writeText(Json.encodeToString(HistorySettings(maxItems = 10, hasExplicitMaxItems = true)))
        var reads = 0
        val preferences = Proxy.newProxyInstance(
            SharedPreferences::class.java.classLoader,
            arrayOf(SharedPreferences::class.java),
        ) { _, method, _ ->
            if (method.name != "getString") error("unexpected preference call")
            if (++reads == 2) throw SecurityException("injected late failure")
            "synthetic"
        } as SharedPreferences
        val bootstrap = SecureBootstrap {
            val store = SecureSettingsStore(preferences, Json)
            store.loadApiKey()
            val persistence = historyTestPersistence(HistoryPaths(root, database, settings, media, true), Json)
            withHistoryStartup(persistence, persistence::close) { history ->
                store.loadApiKey()
                history
            }
        }
        assertNull(bootstrap.retry())
        assertEquals(original, database.readText())
        assertTrue(File(media, "11.txt").exists())
    }
}
