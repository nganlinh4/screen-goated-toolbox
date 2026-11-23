package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.AtomicPublicationUnavailable
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionDirectory
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionFile
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.Paths
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** Explicit sync/namespace ordering stand-in; not kernel or power-loss acceptance. */
class HistoryDirectoryDurabilityTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test
    fun successfulPublicationRequiresDirectoryDurability() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = DurabilityIo()
        val storage = HistoryAtomicFiles(io)
        storage.read(file)
        assertEquals(HistoryWriteResult.COMMITTED, storage.write(file, "new"))
        val sync = "sync:${root.path}"
        assertEquals("Publication reported committed without syncing its directory", sync, io.events.last())
        val stageSync = io.events.indexOf(sync, io.events.indexOf("file_sync") + 1)
        assertTrue(stageSync > io.events.indexOf("file_sync"))
        assertTrue(stageSync < io.events.indexOf("publish"))
        assertTrue(io.events.lastIndexOf(sync) > io.events.indexOf("publish"))
        assertEquals(1, io.opened)
        storage.close()
        assertEquals(0, io.opened)
    }

    @Test
    fun sharedDurabilityCasesPreserveEffectCertaintyAndBothVersions() {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("history-ui/directory-durability.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        assertEquals(4, contract.getValue("cases").jsonArray.size)
        for (element in contract.getValue("cases").jsonArray) {
            val case = element.jsonObject
            val root = temporaryFolder.newFolder()
            val file = File(root, "history.json").apply { writeText("old") }
            val io = DurabilityIo()
            io.failurePhase = case.getValue("fault_phase").takeUnless { it == JsonNull }?.jsonPrimitive?.content
            val storage = HistoryAtomicFiles(io)
            storage.read(file)
            val label = case.getValue("name").jsonPrimitive.content
            assertEquals(label, case.getValue("expected").jsonPrimitive.content, storage.write(file, "new").name)
            assertEquals(label, case.getValue("expected_publications").jsonPrimitive.int, io.publications)
            assertEquals(label, if (io.publications == 0) "old" else "new", file.readText())
            assertTrue(label, root.listFiles().orEmpty().any { it.readText() == "old" })
            assertEquals(label, 1, io.opened)
            storage.close()
            assertEquals(label, 0, io.opened)
        }
    }

    @Test
    fun newlyCreatedDirectoryNamesAreSyncedFromTheDeepestChangedParentOutward() {
        val base = temporaryFolder.newFolder()
        val first = File(base, "first")
        val root = File(first, "second")
        val file = File(root, "history.json")
        val io = DurabilityIo()
        val storage = HistoryAtomicFiles(io)
        assertNull(storage.read(file))
        assertFalse(root.exists())
        assertEquals(HistoryWriteResult.COMMITTED, storage.write(file, "new"))
        val firstParentSync = io.events.indexOf("sync:${first.path}")
        val outerParentSync = io.events.indexOf("sync:${base.path}")
        assertTrue(firstParentSync >= 0)
        assertTrue(firstParentSync < outerParentSync)
        assertTrue(outerParentSync < io.events.indexOf("create"))
        assertEquals(1, io.opened)
        storage.close()
        assertEquals(0, io.opened)
    }

    @Test
    fun failedAncestorSyncPreventsCreatingAnyStageOrPublishingDatabaseBytes() {
        val base = temporaryFolder.newFolder()
        val root = File(base, "first/second")
        val file = File(root, "history.json")
        val io = DurabilityIo().apply { failurePath = base.path }
        val storage = HistoryAtomicFiles(io)
        assertNull(storage.read(file))
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
        assertFalse(file.exists())
        assertFalse(io.events.contains("create"))
        assertEquals(0, io.publications)
        assertEquals(0, io.opened)
    }

    @Test
    fun mediaNameDurabilityPrecedesReturningTheRetainedCreationLease() {
        val root = File(temporaryFolder.newFolder(), "media/nested")
        val io = DurabilityIo()
        HistoryDescriptorMediaWriter(io).create(root.toPath(), "source.txt", "source".toByteArray()).use { media ->
            assertEquals("sync:${root.path}", io.events.last())
            assertTrue(io.events.indexOf("file_sync") < io.events.lastIndexOf("sync:${root.path}"))
            assertTrue(io.opened > 0)
            media.revalidate()
            assertEquals(historyDigest("source".toByteArray()), media.proof.sha256)
        }
        assertEquals(0, io.opened)
    }

    @Test
    fun failedMediaDirectorySyncRetainsBytesAndReleasesEveryDescriptor() {
        val root = temporaryFolder.newFolder()
        val io = DurabilityIo().apply { failurePhase = "staged" }
        assertThrows(IOException::class.java) {
            HistoryDescriptorMediaWriter(io).create(root.toPath(), "source.txt", "source".toByteArray())
        }
        assertEquals("source", File(root, "source.txt").readText())
        assertEquals(0, io.opened)
    }

    @Test
    fun readsDoNotAcquireDirectorySyncCapabilityOrCreateStorage() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = DurabilityIo().apply { failurePhase = "before_stage" }
        val storage = HistoryAtomicFiles(io)
        assertEquals("old", storage.read(file))
        val absent = File(root, "missing/history.json")
        assertNull(storage.read(absent))
        assertFalse(requireNotNull(absent.parentFile).exists())
        assertTrue(io.events.isEmpty())
        assertEquals(1, io.opened)
        storage.close()
        assertEquals(0, io.opened)
    }

    @Test
    fun unsupportedSyncAfterSuccessfulExchangeCannotBecomeKnownRejection() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = DurabilityIo().apply { failurePhase = "published"; unsupported = true }
        val storage = HistoryAtomicFiles(io)
        storage.read(file)
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
        assertEquals("new", file.readText())
        assertTrue(root.listFiles().orEmpty().any { it.readText() == "old" })
        assertEquals(1, io.opened)
        storage.close()
        assertEquals(0, io.opened)
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    @Test
    fun postPublicationSyncFailureKeepsTheVisibleSnapshotAndMediaUntilRecovery() = runTest {
        val root = temporaryFolder.newFolder()
        val paths = HistoryPaths(root, File(root, "history.json"), File(root, "settings.json"), File(root, "media"), false)
        val io = DurabilityIo().apply { failurePhase = "published" }
        val storage = HistoryPersistence(paths, Json, ::historyTestFileIdentity, historyTestMediaWriter(), io)
        val repository = HistoryRepository(storage, StandardTestDispatcher(testScheduler))
        repository.saveText("result", "source")
        advanceUntilIdle()
        assertTrue(repository.state.value.items.isEmpty())
        assertTrue(repository.state.value.storageUnavailable)
        assertEquals(HistoryStorageError.STORAGE_CHANGED, repository.state.value.storageError)
        io.failurePhase = null
        val durable = storage.loadDatabase().items.single()
        assertEquals("result", durable.text)
        assertEquals("source", File(paths.mediaDir, durable.mediaPath).readText())
        repository.clearAll()
        repository.saveText("later", "later-source")
        advanceUntilIdle()
        assertEquals(1, io.publications)
        assertEquals(listOf(durable), storage.loadDatabase().items)
        assertEquals(1, paths.mediaDir.listFiles().orEmpty().size)
        assertEquals(1, io.opened)
        storage.close()
        assertEquals(0, io.opened)
    }

    private fun List<String>.indexOf(value: String, start: Int): Int =
        indices.firstOrNull { it >= start && this[it] == value } ?: -1

    private class DurabilityIo : FileTransactionIo {
        val delegate = NioTestFileTransactionIo()
        val events = mutableListOf<String>()
        var publications = 0
        var opened = 0
        var failurePhase: String? = null
        var failurePath: String? = null
        var unsupported = false
        private var stageSynced = false
        private inner class Directory(val inner: TransactionDirectory) : TransactionDirectory by inner {
            init { opened++ }
            override fun sync() {
                events += "sync:${currentPath()}"
                val phase = when { publications > 0 -> "published"; stageSynced -> "staged"; else -> "before_stage" }
                if (failurePhase == phase || failurePath == currentPath()) {
                    if (unsupported) throw AtomicPublicationUnavailable(95)
                    throw IOException("Directory sync unavailable")
                }
            }
            override fun close() { inner.close(); opened-- }
        }
        override fun directory(path: String): TransactionDirectory = Directory(delegate.directory(path))
        override fun directory(parent: TransactionDirectory, name: String): TransactionDirectory? =
            delegate.directory((parent as Directory).inner, name)?.let { Directory(it) }
        override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? {
            val file = delegate.file((parent as Directory).inner, name, create) ?: return null
            return wrap(file, name, create)
        }
        override fun writableFile(parent: TransactionDirectory, name: String): TransactionFile? =
            delegate.writableFile((parent as Directory).inner, name)?.let { wrap(it, name, false) }
        private fun wrap(file: TransactionFile, name: String, create: Boolean): TransactionFile {
            val payload = !name.endsWith(".receipt")
            if (create && payload) events += "create"
            opened++
            return object : TransactionFile by file {
                override fun writeSynced(bytes: ByteArray, cancelled: () -> Boolean) {
                    file.writeSynced(bytes, cancelled)
                    if (payload) { stageSynced = true; events += "file_sync" }
                }
                override fun close() { file.close(); opened-- }
            }
        }
        override fun makeDirectory(parent: TransactionDirectory, name: String) = delegate.makeDirectory((parent as Directory).inner, name)
        override fun publish(parent: TransactionDirectory, stage: String, target: String, exchange: Boolean) {
            delegate.publish((parent as Directory).inner, stage, target, exchange)
            publications++
            events += "publish"
        }
    }
}
