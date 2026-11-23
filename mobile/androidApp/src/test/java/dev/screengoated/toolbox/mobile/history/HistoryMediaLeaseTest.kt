package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionDirectory
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionFile
import java.io.File
import java.nio.file.Path
import java.nio.file.Files
import java.nio.file.Paths
import java.io.IOException
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

@OptIn(ExperimentalCoroutinesApi::class)
class HistoryMediaLeaseTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test
    fun creationDescriptorAndRouteRemainOpenThroughDatabaseWrite() = runTest {
        val root = temporaryFolder.newFolder()
        val paths = HistoryPaths(root, File(root, "history.json"), File(root, "settings.json"), File(root, "media"), false)
        val io = LeaseIo()
        val delegate = HistoryPersistence(paths, Json, { io.identity(it) }, HistoryDescriptorMediaWriter(io), NioTestFileTransactionIo())
        var retainedDuringWrite = false
        val store = object : HistoryStorage by delegate {
            override fun saveDatabase(database: StoredHistoryDatabase): HistoryWriteResult {
                retainedDuringWrite = io.created.get() == 1 && io.directories.get() > 0
                return HistoryWriteResult.COMMITTED
            }
        }
        val repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
        repository.saveText("new", "source")
        advanceUntilIdle()
        assertTrue("Creation descriptor and ancestry closed before persistence", retainedDuringWrite)
        assertEquals(0, io.created.get())
        assertEquals(0, io.directories.get())
        assertEquals(listOf("new"), repository.state.value.items.map { it.text })
    }

    @Test
    fun sharedLeaseOutcomesRetainThenReleaseExactlyTheCreatedResources() = runTest {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("history-ui/media-lease.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        assertEquals(3, contract.getValue("cases").jsonArray.size)
        for (element in contract.getValue("cases").jsonArray) {
            val case = element.jsonObject
            val paths = paths()
            val io = LeaseIo()
            val delegate = HistoryPersistence(paths, Json, { io.identity(it) }, HistoryDescriptorMediaWriter(io), NioTestFileTransactionIo())
            var writes = 0
            lateinit var repository: HistoryRepository
            val store = object : HistoryStorage by delegate {
                override fun createMedia(type: HistoryType, bytes: ByteArray): HistoryPreparedMedia {
                    val media = delegate.createMedia(type, bytes)
                    if (case.getValue("retired").jsonPrimitive.boolean) repository.clearAll()
                    return media
                }
                override fun saveDatabase(database: StoredHistoryDatabase): HistoryWriteResult {
                    if (database.items.isEmpty()) return HistoryWriteResult.COMMITTED
                    assertEquals(1, io.created.get())
                    assertTrue(io.directories.get() > 0)
                    assertTrue(repository.state.value.items.isEmpty())
                    writes++
                    return if (case.getValue("write_success").jsonPrimitive.boolean) HistoryWriteResult.COMMITTED
                        else HistoryWriteResult.REJECTED
                }
            }
            repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
            repository.saveText("new", "source")
            advanceUntilIdle()
            assertEquals(case.getValue("expected_save_writes").jsonPrimitive.int, writes)
            assertEquals(case.getValue("expected_count").jsonPrimitive.int, repository.state.value.items.size)
            assertEquals(case.getValue("expected_media").jsonPrimitive.int, paths.mediaDir.listFiles().orEmpty().size)
            assertEquals(case.getValue("expected_error").jsonPrimitive.boolean, repository.state.value.storageError != null)
            assertEquals(0, io.created.get())
            assertEquals(0, io.directories.get())
        }
    }

    @Test
    fun changedMediaBeforeWriteIsRejectedWithoutPublishingOrDeletingTheChangedFile() = runTest {
        val paths = paths()
        val io = LeaseIo()
        val delegate = HistoryPersistence(paths, Json, { io.identity(it) }, HistoryDescriptorMediaWriter(io), NioTestFileTransactionIo())
        var writes = 0
        val store = object : HistoryStorage by delegate {
            override fun createMedia(type: HistoryType, bytes: ByteArray): HistoryPreparedMedia {
                val media = delegate.createMedia(type, bytes)
                File(paths.mediaDir, media.name).writeText("edited")
                return media
            }
            override fun saveDatabase(database: StoredHistoryDatabase): HistoryWriteResult {
                writes++
                return HistoryWriteResult.COMMITTED
            }
        }
        val repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
        repository.saveText("new", "source")
        advanceUntilIdle()
        assertEquals(0, writes)
        assertTrue(repository.state.value.items.isEmpty())
        assertEquals(HistoryStorageError.WRITE_FAILED, repository.state.value.storageError)
        assertEquals("edited", paths.mediaDir.listFiles().orEmpty().single().readText())
        assertEquals(0, io.created.get())
        assertEquals(0, io.directories.get())
    }

    @Test
    fun changedMediaDuringSuccessfulWriteIsUncertainBeforeCachePublication() = runTest {
        val paths = paths()
        val io = LeaseIo()
        val delegate = HistoryPersistence(paths, Json, { io.identity(it) }, HistoryDescriptorMediaWriter(io), NioTestFileTransactionIo())
        var writes = 0
        val store = object : HistoryStorage by delegate {
            override fun saveDatabase(database: StoredHistoryDatabase): HistoryWriteResult {
                assertEquals(1, io.created.get())
                writes++
                File(paths.mediaDir, database.items.single().mediaPath).writeText("edited")
                return HistoryWriteResult.COMMITTED
            }
        }
        val repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
        repository.saveText("new", "source")
        advanceUntilIdle()
        repository.clearAll()
        advanceUntilIdle()
        assertEquals(1, writes)
        assertTrue(repository.state.value.items.isEmpty())
        assertTrue(repository.state.value.storageUnavailable)
        assertEquals(HistoryStorageError.STORAGE_CHANGED, repository.state.value.storageError)
        assertEquals("edited", paths.mediaDir.listFiles().orEmpty().single().readText())
        assertEquals(0, io.created.get())
        assertEquals(0, io.directories.get())
    }

    @Test
    fun uncertainAndThrowingWritersRetainMediaAndReleaseEveryLease() = runTest {
        for (throws in listOf(false, true)) {
            val paths = paths()
            val io = LeaseIo()
            val delegate = HistoryPersistence(paths, Json, { io.identity(it) }, HistoryDescriptorMediaWriter(io), NioTestFileTransactionIo())
            val store = object : HistoryStorage by delegate {
                override fun saveDatabase(database: StoredHistoryDatabase): HistoryWriteResult {
                    assertEquals(1, io.created.get())
                    if (throws) throw IOException("Uncertain write")
                    return HistoryWriteResult.UNAVAILABLE
                }
            }
            val repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
            repository.saveText("new", "source")
            advanceUntilIdle()
            assertTrue(repository.state.value.storageUnavailable)
            assertEquals("source", paths.mediaDir.listFiles().orEmpty().single().readText())
            assertEquals(0, io.created.get())
            assertEquals(0, io.directories.get())
        }
    }

    @Test
    fun closedLeaseCannotBeRevalidatedOrReleasedTwice() {
        var released = 0
        val media = HistoryPreparedMedia("source.txt", HistoryMediaProof("identity", 0, historyDigest(byteArrayOf())),
            validateOwned = {}, release = { released++ })
        media.revalidate()
        media.close()
        media.close()
        assertThrows(IOException::class.java) { media.revalidate() }
        assertEquals(1, released)
    }

    private fun paths(): HistoryPaths {
        val root = temporaryFolder.newFolder()
        return HistoryPaths(root, File(root, "history.json"), File(root, "settings.json"), File(root, "media"), false)
    }

    private class LeaseIo : FileTransactionIo {
        val delegate = NioTestFileTransactionIo()
        val created = AtomicInteger()
        val directories = AtomicInteger()
        private inner class Directory(val inner: TransactionDirectory) : TransactionDirectory by inner {
            init { directories.incrementAndGet() }
            override fun close() { inner.close(); directories.decrementAndGet() }
        }
        override fun directory(path: String): TransactionDirectory = Directory(delegate.directory(path))
        override fun directory(parent: TransactionDirectory, name: String): TransactionDirectory? =
            delegate.directory((parent as Directory).inner, name)?.let { Directory(it) }
        override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? {
            val file = delegate.file((parent as Directory).inner, name, create) ?: return null
            if (!create) return file
            created.incrementAndGet()
            return object : TransactionFile by file {
                override fun close() { file.close(); created.decrementAndGet() }
            }
        }
        override fun makeDirectory(parent: TransactionDirectory, name: String) = delegate.makeDirectory((parent as Directory).inner, name)
        override fun writableFile(parent: TransactionDirectory, name: String): TransactionFile? =
            delegate.writableFile((parent as Directory).inner, name)
        override fun publish(parent: TransactionDirectory, stage: String, target: String, exchange: Boolean) =
            delegate.publish((parent as Directory).inner, stage, target, exchange)
        fun identity(path: Path): String = directory(path.parent.toString()).use { parent ->
            requireNotNull(file(parent, path.fileName.toString())).use { "${it.identity.device}:${it.identity.inode}" }
        }
    }
}
