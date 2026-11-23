package dev.screengoated.toolbox.mobile.history

import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

@OptIn(ExperimentalCoroutinesApi::class)
class HistoryOrderingTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test
    fun openResolvesOnlyAnUnambiguousCurrentItem() = runTest {
        val delegate = historyTestPersistence(paths(), Json)
        val item = HistoryItem(7, "timestamp", HistoryType.TEXT, "current", "source.txt")
        var requests = 0
        val store = object : HistoryStorage by delegate {
            override fun loadDatabase() = StoredHistoryDatabase(items = listOf(item))
            override fun mediaFile(item: HistoryItem): File? { requests++; return File("owned") }
        }
        val repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
        assertEquals(null, repository.mediaFileFor(item.copy(text = "stale")))
        assertEquals(0, requests)
        assertEquals(File("owned"), repository.mediaFileFor(item))
        val ambiguous = object : HistoryStorage by store {
            override fun loadDatabase() = StoredHistoryDatabase(items = listOf(item, item.copy(text = "duplicate")))
        }
        val other = HistoryRepository(ambiguous, StandardTestDispatcher(testScheduler))
        assertEquals(null, other.mediaFileFor(item))
        assertEquals(1, requests)
    }

    @Test
    fun proofPreparationRejectsStaleAmbiguousAndChangedSnapshots() = runTest {
        val delegate = historyTestPersistence(paths(), Json)
        val item = HistoryItem(7, "timestamp", HistoryType.TEXT, "current", "source.txt")
        val proof = HistoryMediaProof("1:2", 3, "a".repeat(64))
        lateinit var repository: HistoryRepository
        var changeDuringLookup = false
        val store = object : HistoryStorage by delegate {
            override fun loadDatabase() = StoredHistoryDatabase(items = listOf(item))
            override fun saveSettings(settings: HistorySettings) = HistoryWriteResult.COMMITTED
            override fun mediaProofForOpen(item: HistoryItem): HistoryMediaProof {
                if (changeDuringLookup) { repository.updateMaxItems(20); testScheduler.runCurrent() }
                return proof
            }
        }
        repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
        assertEquals(proof, repository.mediaProofForOpen(item))
        assertEquals(null, repository.mediaProofForOpen(item.copy(text = "stale")))
        changeDuringLookup = true
        assertEquals(null, repository.mediaProofForOpen(item))
        val ambiguous = object : HistoryStorage by store {
            override fun loadDatabase() = StoredHistoryDatabase(items = listOf(item, item.copy(text = "duplicate")))
        }
        assertEquals(null, HistoryRepository(ambiguous, StandardTestDispatcher(testScheduler)).mediaProofForOpen(item))
    }

    @Test
    fun unavailableStorageCannotAuthorizeACapturedOpen() = runTest {
        val delegate = historyTestPersistence(paths(), Json)
        val item = HistoryItem(7, "timestamp", HistoryType.TEXT, "current", "source.txt")
        var requests = 0
        val store = object : HistoryStorage by delegate {
            override fun loadDatabase() = StoredHistoryDatabase(items = listOf(item))
            override fun saveSettings(settings: HistorySettings) = HistoryWriteResult.UNAVAILABLE
            override fun mediaFile(item: HistoryItem): File? { requests++; return File("owned") }
        }
        val repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
        repository.updateMaxItems(20)
        advanceUntilIdle()
        assertTrue(repository.state.value.storageUnavailable)
        assertEquals(listOf(item), repository.state.value.items)
        assertEquals(null, repository.mediaFileFor(item))
        assertEquals(0, requests)
    }

    @Test
    fun anOpenIsRejectedWhenTheCommittedStateChangesDuringLookup() = runTest {
        val delegate = historyTestPersistence(paths(), Json)
        val item = HistoryItem(7, "timestamp", HistoryType.TEXT, "current", "source.txt")
        lateinit var repository: HistoryRepository
        val store = object : HistoryStorage by delegate {
            override fun loadDatabase() = StoredHistoryDatabase(items = listOf(item))
            override fun saveSettings(settings: HistorySettings) = HistoryWriteResult.COMMITTED
            override fun mediaFile(item: HistoryItem): File? {
                repository.updateMaxItems(20)
                testScheduler.runCurrent()
                return File("owned")
            }
        }
        repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
        assertEquals(null, repository.mediaFileFor(item))
        assertEquals(20, repository.state.value.maxItems)
    }

    @Test
    fun descriptorAdmissionRejectsAnObsoleteSnapshotWithoutHandoff() = runTest {
        val delegate = historyTestPersistence(paths(), Json)
        val store = object : HistoryStorage by delegate {
            override fun saveSettings(settings: HistorySettings) = HistoryWriteResult.COMMITTED
        }
        val repository = HistoryRepository(store, StandardTestDispatcher(testScheduler))
        val snapshot = repository.state.value
        repository.updateMaxItems(20)
        advanceUntilIdle()
        var called = false
        assertEquals(null, repository.admitMediaOpen(snapshot) { called = true; "descriptor" })
        assertTrue(!called)
        assertEquals("descriptor", repository.admitMediaOpen(repository.state.value) { "descriptor" })
    }

    @Test
    fun descriptorHandoffSerializesStatePublication() {
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val attempted = CountDownLatch(1)
        val workers = Executors.newFixedThreadPool(2)
        try {
            val repository = HistoryRepository(historyTestPersistence(paths(), Json), deferStartupPrune = true)
            val snapshot = repository.state.value
            val handoff = workers.submit<String?> {
                repository.admitMediaOpen(snapshot) {
                    entered.countDown()
                    check(release.await(5, TimeUnit.SECONDS))
                    "descriptor"
                }
            }
            assertTrue(entered.await(5, TimeUnit.SECONDS))
            val mutation = workers.submit {
                attempted.countDown()
                repository.updateMaxItems(20)
            }
            assertTrue(attempted.await(5, TimeUnit.SECONDS))
            assertEquals(snapshot, repository.state.value)
            release.countDown()
            assertEquals("descriptor", handoff.get(5, TimeUnit.SECONDS))
            mutation.get(5, TimeUnit.SECONDS)
            repository.completeDeferredStartup()
            runBlocking { withTimeout(5_000) { repository.state.first { it.maxItems == 20 } } }
        } finally {
            release.countDown()
            workers.shutdownNow()
        }
    }

    @Test
    fun deferredStartupOrdersRetentionBeforeQueuedChangesWithoutWritingEarly() = runTest {
        val paths = paths()
        val delegate = historyTestPersistence(paths, Json)
        var writes = 0
        val store = object : HistoryStorage by delegate {
            override fun saveDatabase(database: StoredHistoryDatabase): HistoryWriteResult {
                writes++
                return delegate.saveDatabase(database)
            }
        }
        val repository = HistoryRepository(store, StandardTestDispatcher(testScheduler), deferStartupPrune = true)
        repository.saveText("before", "before-source")
        repository.clearAll()
        repository.saveText("after", "after-source")
        advanceUntilIdle()
        assertEquals(0, writes)
        assertTrue(!paths.databaseFile.exists())
        assertTrue(!paths.mediaDir.exists())
        repository.completeDeferredStartup()
        repository.completeDeferredStartup()
        advanceUntilIdle()
        assertEquals(2, writes)
        assertEquals(listOf("after"), repository.state.value.items.map { it.text })
        assertEquals("after-source", repository.mediaFileFor(repository.state.value.items.single())?.readText())
    }

    @Test
    fun byteBuffersAreCopiedAtAdmissionAndCommittedSnapshotsAreReadOnly() = runTest {
        val repository = HistoryRepository(historyTestPersistence(paths(), Json), StandardTestDispatcher(testScheduler))
        val bytes = byteArrayOf(1, 2, 3)
        repository.saveImage(bytes, "image")
        bytes.fill(9)
        advanceUntilIdle()
        val item = repository.state.value.items.single()
        assertEquals(listOf<Byte>(1, 2, 3), repository.mediaFileFor(item)?.readBytes()?.toList())
        assertThrows(UnsupportedOperationException::class.java) {
            (repository.state.value.items as MutableList<HistoryItem>).clear()
        }
        assertEquals(listOf(item), repository.state.value.items)
    }

    @Test
    fun admittedWriteCompletesBeforeClearWhileClearAdmissionAndCacheRemainAvailable() {
        blockedSave(clearDuringPreparation = false)
    }

    @Test
    fun maintenanceFailureKeepsCommittedPolicyAndDoesNotStopTheWorker() = runTest {
        val delegate = historyTestPersistence(paths(), Json)
        val repository = HistoryRepository(delegate, StandardTestDispatcher(testScheduler)) {
            throw java.io.IOException("Synthetic maintenance failure")
        }
        repository.updateMaxItems(10)
        advanceUntilIdle()
        assertEquals(10, repository.state.value.maxItems)
        assertEquals(10, delegate.loadSettings().maxItems)
        assertEquals(HistoryStorageError.WRITE_FAILED, repository.state.value.storageError)
        repository.saveText("new", "source")
        advanceUntilIdle()
        assertEquals(listOf("new"), repository.state.value.items.map { it.text })
        assertEquals(null, repository.state.value.storageError)
        assertEquals(repository.state.value.items, delegate.loadDatabase().items)
    }

    @Test
    fun clearRetiresMediaPreparedSaveBeforeDatabaseAdmission() {
        blockedSave(clearDuringPreparation = true)
    }

    private fun blockedSave(clearDuringPreparation: Boolean) {
        val paths = paths()
        val delegate = historyTestPersistence(paths, Json)
        val (name, proof) = delegate.createMedia(HistoryType.TEXT, "old".toByteArray()).use { it.name to it.proof }
        val old = HistoryItem(1, "timestamp", HistoryType.TEXT, "old", name, proof)
        paths.databaseFile.writeText(Json.encodeToString(StoredHistoryDatabase(items = listOf(old))))
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val candidates = mutableListOf<List<String>>()
        var blocked = false
        fun blockOnce() {
            if (!blocked) {
                blocked = true
                entered.countDown()
                check(release.await(5, TimeUnit.SECONDS)) { "Storage was not released" }
            }
        }
        val store = object : HistoryStorage by delegate {
            override fun createMedia(type: HistoryType, bytes: ByteArray): HistoryPreparedMedia {
                val prepared = delegate.createMedia(type, bytes)
                if (clearDuringPreparation) blockOnce()
                return prepared
            }
            override fun saveDatabase(database: StoredHistoryDatabase): HistoryWriteResult {
                if (!clearDuringPreparation) blockOnce()
                candidates += database.items.map { it.text }
                return delegate.saveDatabase(database)
            }
        }
        val worker = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
        val admission = Executors.newSingleThreadExecutor()
        try {
            val repository = HistoryRepository(store, worker)
            repository.saveText("new", "source")
            assertTrue(entered.await(5, TimeUnit.SECONDS))
            assertEquals(listOf(old), repository.state.value.items)
            admission.submit { repository.clearAll() }.get(2, TimeUnit.SECONDS)
            assertEquals(listOf(old), repository.state.value.items)
            release.countDown()
            runBlocking { withTimeout(5_000) { repository.state.first { it.items.isEmpty() } } }
            runBlocking {
                withContext(worker) {
                    assertEquals(if (clearDuringPreparation) listOf(emptyList<String>())
                        else listOf(listOf("new", "old"), emptyList()), candidates)
                    assertTrue(paths.mediaDir.listFiles().orEmpty().isEmpty())
                }
            }
        } finally {
            release.countDown()
            admission.shutdownNow()
            worker.close()
        }
    }

    private fun paths(): HistoryPaths {
        val root = temporaryFolder.newFolder()
        return HistoryPaths(root, File(root, "history.json"), File(root, "settings.json"), File(root, "media"), false)
    }
}
