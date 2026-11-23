package dev.screengoated.toolbox.mobile.history

import java.io.File
import java.nio.charset.CharacterCodingException
import java.nio.file.Path
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import kotlinx.serialization.json.Json
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo

@OptIn(ExperimentalCoroutinesApi::class)
class HistoryStorageSafetyTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test
    fun atomicReplacementRequiresTheLoadedTargetVersion() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json")
        val storage = HistoryAtomicFiles(NioTestFileTransactionIo())
        assertNull(storage.read(file))
        assertEquals(HistoryWriteResult.COMMITTED, storage.write(file, "first"))
        assertEquals(HistoryWriteResult.COMMITTED, storage.write(file, "second"))
        assertEquals("second", storage.read(file))
        file.writeText("external replacement")
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "third"))
        assertEquals("external replacement", file.readText())
        assertEquals("first", File(root, historyStageName(file.name)).readText())
        assertEquals(3, root.listFiles().orEmpty().size)
    }

    @Test
    fun invalidUtf8IsReportedWithoutReplacingBytes() {
        val file = temporaryFolder.newFile()
        val bytes = byteArrayOf(0xC3.toByte(), 0x28)
        file.writeBytes(bytes)
        val storage = HistoryAtomicFiles(NioTestFileTransactionIo())
        assertThrows(CharacterCodingException::class.java) { storage.read(file) }
        assertEquals(bytes.toList(), file.readBytes().toList())
    }

    @Test
    fun invalidSettingsOrUnknownSchemasDisableEveryMutation() = runTest {
        for ((database, settings) in listOf("{}" to "{broken", "{\"version\":2}" to "{}",
            "{}" to "{\"schemaVersion\":2}")) {
            val root = temporaryFolder.newFolder()
            val paths = HistoryPaths(root, File(root, "history.json"), File(root, "settings.json"),
                File(root, "media"), false)
            paths.databaseFile.writeText(database)
            paths.settingsFile.writeText(settings)
            val repository = HistoryRepository(historyTestPersistence(paths, Json), StandardTestDispatcher(testScheduler))
            repository.saveText("new", "source")
            repository.updateMaxItems(10)
            repository.resetSettingsToDefaults()
            repository.clearAll()
            advanceUntilIdle()
            assertEquals(database, paths.databaseFile.readText())
            assertEquals(settings, paths.settingsFile.readText())
            assertTrue(repository.state.value.storageUnavailable)
            assertEquals(HistoryStorageError.LOAD_FAILED, repository.state.value.storageError)
            assertTrue(!paths.mediaDir.exists())
        }
    }

    @Test
    fun uncertainPublicationRetainsMediaAndStopsLaterMutationsUntilReopen() = runTest {
        val root = temporaryFolder.newFolder()
        val paths = HistoryPaths(root, File(root, "history.json"), File(root, "settings.json"),
            File(root, "media"), false)
        val delegate = historyTestPersistence(paths, Json)
        var writes = 0
        val store = object : HistoryStorage by delegate {
            override fun saveDatabase(database: StoredHistoryDatabase): HistoryWriteResult {
                writes++
                assertEquals(HistoryWriteResult.COMMITTED, delegate.saveDatabase(database))
                return HistoryWriteResult.UNAVAILABLE
            }
        }
        val dispatcher = StandardTestDispatcher(testScheduler)
        val repository = HistoryRepository(store, dispatcher)
        repository.saveText("result", "source")
        advanceUntilIdle()
        repository.clearAll()
        repository.saveText("later", "later-source")
        advanceUntilIdle()
        assertEquals(1, writes)
        assertTrue(repository.state.value.items.isEmpty())
        assertTrue(repository.state.value.storageUnavailable)
        assertEquals(HistoryStorageError.STORAGE_CHANGED, repository.state.value.storageError)
        val durable = delegate.loadDatabase().items.single()
        assertEquals("source", delegate.mediaFile(durable)?.readText())
        val reopened = HistoryRepository(delegate, dispatcher)
        advanceUntilIdle()
        assertEquals(listOf(durable), reopened.state.value.items)
        reopened.clearAll()
        advanceUntilIdle()
        assertTrue(paths.mediaDir.listFiles().orEmpty().isEmpty())
    }

    @Test
    fun mediaCleanupRetainsChangedUnprovenAndEscapingTargets() {
        val root = temporaryFolder.newFolder()
        val directory = File(root, "media").apply { mkdirs() }
        val outside = File(root, "outside.txt").apply { writeText("outside") }
        val io = NioTestFileTransactionIo()
        val identity: (Path) -> String = { historyDescriptorTestIdentity(io, it) }
        val media = HistoryMediaStore(directory, identity, historyTestMediaWriter(identity), io)
        val (name, proof) = media.create(HistoryType.TEXT, "source".toByteArray()).use { it.name to it.proof }
        val item = HistoryItem(1, "timestamp", HistoryType.TEXT, "result", name, proof)
        File(directory, name).writeText("changed")
        media.remove(item)
        assertEquals("changed", File(directory, name).readText())
        File(directory, name).writeText("source")
        io.replaceIdentity(File(directory, name).path)
        media.remove(item)
        assertTrue(File(directory, name).exists())
        media.remove(item.copy(mediaProof = null))
        assertTrue(File(directory, name).exists())
        for (path in listOf("../outside.txt", outside.absolutePath, "nested/file.txt", "file:stream", "..")) {
            val escaped = item.copy(mediaPath = path, mediaProof = null)
            assertNull(historyMediaFile(directory, escaped, identity))
            media.remove(escaped)
        }
        assertEquals("outside", outside.readText())
        val (ownedName, ownedProof) = media.create(HistoryType.TEXT, "source".toByteArray()).use { it.name to it.proof }
        media.remove(item.copy(mediaPath = ownedName, mediaProof = ownedProof))
        assertTrue(!File(directory, ownedName).exists())
        assertTrue(File(directory, name).exists())
    }
}
