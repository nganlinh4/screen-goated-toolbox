package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionDirectory
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionFile
import dev.screengoated.toolbox.mobile.storage.SecureBootstrap
import dev.screengoated.toolbox.mobile.storage.SecureSettingsStore
import android.content.SharedPreferences
import java.io.File
import java.lang.reflect.Proxy
import java.nio.file.Files
import java.nio.file.Paths
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

@OptIn(ExperimentalCoroutinesApi::class)
class HistoryStartupOwnershipTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test fun failedBootstrapReleasesPinsAndRetryStartsOnlyItsOwnWorker() = runTest {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("history-ui/startup-ownership.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        val cases = contract.getValue("cases").jsonArray.associate { element ->
            val case = element.jsonObject
            case.getValue("event").jsonPrimitive.content to case
        }
        assertEquals(setOf("bootstrap_failure", "bootstrap_success"), cases.keys)
        val failureCase = cases.getValue("bootstrap_failure")
        val successCase = cases.getValue("bootstrap_success")
        val root = temporaryFolder.newFolder()
        val database = File(root, "history.json").apply { writeText(Json.encodeToString(StoredHistoryDatabase())) }
        val settings = File(root, "settings.json").apply { writeText(Json.encodeToString(HistorySettings())) }
        val original = database.readText()
        val paths = HistoryPaths(root, database, settings, File(root, "media"), false)
        var openHandles = 0
        var closes = 0
        var attempts = 0
        var livePersistence: HistoryPersistence? = null
        var failedHistory: HistoryRepository? = null
        val unavailablePreferences = Proxy.newProxyInstance(
            SharedPreferences::class.java.classLoader, arrayOf(SharedPreferences::class.java),
        ) { _, method, _ ->
            if (method.name == "getString") throw SecurityException("unavailable") else error("Unexpected preferences operation")
        } as SharedPreferences
        val bootstrap = SecureBootstrap {
            val delegate = NioTestFileTransactionIo()
            val io = object : FileTransactionIo by delegate {
                override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? {
                    val opened = delegate.file(parent, name, create) ?: return null
                    openHandles++
                    return object : TransactionFile by opened {
                        override fun close() { try { opened.close() } finally { openHandles-- } }
                    }
                }
            }
            val persistence = HistoryPersistence(paths, Json, ::historyTestFileIdentity, historyTestMediaWriter(), io)
            withHistoryStartup(persistence, { closes++; persistence.close() }, StandardTestDispatcher(testScheduler)) { history ->
                history.saveText("retired", "old source")
                history.clearAll()
                history.saveText("result", "source")
                if (++attempts == 1) {
                    failedHistory = history
                    testScheduler.runCurrent()
                    SecureSettingsStore(unavailablePreferences, Json).loadCustomModels()
                }
                livePersistence = persistence
                history
            }
        }
        assertNull(bootstrap.retry())
        advanceUntilIdle()
        assertEquals(1, closes)
        assertEquals(0, openHandles)
        assertEquals(original, database.readText())
        assertTrue(!paths.mediaDir.exists())
        assertEquals(failureCase.getValue("storage").jsonPrimitive.content,
            if (closes == 1 && openHandles == 0) "closed" else "retained")
        assertEquals(failureCase.getValue("pending_changes").jsonPrimitive.content,
            if (database.readText() == original && !paths.mediaDir.exists()) "discarded" else "ordered")
        requireNotNull(failedHistory).completeDeferredStartup()
        advanceUntilIdle()
        assertEquals(original, database.readText())
        val history = requireNotNull(bootstrap.retry())
        assertEquals(2, openHandles)
        advanceUntilIdle()
        assertEquals(listOf("result"), history.state.value.items.map { it.text })
        assertEquals(1, closes)
        assertEquals(successCase.getValue("storage").jsonPrimitive.content,
            if (closes == 1 && openHandles > 0) "retained" else "closed")
        assertEquals(successCase.getValue("pending_changes").jsonPrimitive.content,
            if (history.state.value.items.map { it.text } == listOf("result")) "ordered" else "discarded")
        requireNotNull(livePersistence).close()
        assertEquals(0, openHandles)
    }

    @Test fun cleanupFailureDoesNotReplaceTheBootstrapFailure() = runTest {
        val root = temporaryFolder.newFolder()
        val paths = HistoryPaths(root, File(root, "history.json"), File(root, "settings.json"), File(root, "media"), false)
        val persistence = historyTestPersistence(paths, Json)
        val original = IllegalStateException("bootstrap failed")
        val failure = assertThrows(IllegalStateException::class.java) {
            withHistoryStartup(persistence, { persistence.close(); error("close failed") },
                StandardTestDispatcher(testScheduler)) { throw original }
        }
        assertTrue(failure === original)
        assertEquals(listOf("close failed"), failure.suppressed.map { it.message })
        advanceUntilIdle()
        assertTrue(!paths.databaseFile.exists())
    }
}
