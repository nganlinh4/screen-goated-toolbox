package dev.screengoated.toolbox.mobile.history

import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.encodeToString
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileSnapshotLimitExceeded
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionDirectory
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionFile
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileFingerprint
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileObjectId
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class HistoryMediaCreationTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test
    fun descriptorVerificationClosesAReplacedEntryEvenWhenBytesMatch() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "source.txt").apply { writeText("source") }
        val io = NioTestFileTransactionIo()
        val original = io.directory(root.path).use { parent -> requireNotNull(io.file(parent, file.name)).use { it.identity } }
        var closed = 0
        val replacing = object : FileTransactionIo by io {
            override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? {
                val opened = io.file(parent, name, create) ?: return null
                if (!create && name == file.name) io.replaceIdentity(file.path)
                return object : TransactionFile by opened {
                    override fun close() { opened.close(); closed++ }
                }
            }
        }
        HistoryDirectoryRoute(replacing, root.toPath()).use { route ->
            assertThrows(IOException::class.java) {
                openVerifiedHistoryMedia(replacing, route, file.name, original,
                    FileFingerprint(6, historyDigest("source".toByteArray())), 6).close()
            }
        }
        assertEquals(1, closed)
        assertEquals("source", file.readText())
    }

    @Test
    fun replacementAfterWriteCannotAcquireCreatedMediaOwnership() {
        val root = temporaryFolder.newFolder()
        val bytes = "source".toByteArray()
        var replaced: File? = null
        val store = HistoryMediaStore(root, fileIdentity = { path ->
            if (replaced == null) {
                val modified = Files.getLastModifiedTime(path)
                Files.move(path, path.resolveSibling("original.txt"))
                Files.write(path, bytes)
                Files.setLastModifiedTime(path, modified)
                replaced = path.toFile()
            }
            "replacement-object"
        }, writer = historyTestMediaWriter())
        assertThrows(IOException::class.java) { store.create(HistoryType.TEXT, bytes).use {} }
        assertEquals("source", replaced?.readText())
        assertEquals("source", File(root, "original.txt").readText())
    }

    @Test
    fun sharedPayloadsUseTheCreatedDescriptorIdentityAndVerifiedFingerprint() {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("history-ui/media-creation.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        assertEquals("retained_creation_descriptor", contract.getValue("identity_source").jsonPrimitive.content)
        assertEquals(5, contract.getValue("cases").jsonArray.size)
        for (element in contract.getValue("cases").jsonArray) {
            val case = element.jsonObject
            val pattern = case.getValue("pattern").jsonArray.map { it.jsonPrimitive.int.toByte() }.toByteArray()
            val bytes = ByteArray(pattern.size * case.getValue("repeat").jsonPrimitive.int) { pattern[it % pattern.size] }
            val io = NioTestFileTransactionIo()
            val directory = File(temporaryFolder.newFolder(), "history/media")
            val store = HistoryMediaStore(directory, fileIdentity = { identity(io, it) },
                writer = HistoryDescriptorMediaWriter(io))
            val (name, proof) = store.create(HistoryType.TEXT, bytes).use { it.name to it.proof }
            val path = directory.resolve(name).toPath()
            assertEquals(identity(io, path), proof.identity)
            assertEquals(case.getValue("expected_size").jsonPrimitive.content.toLong(), proof.size)
            assertEquals(case.getValue("expected_sha256").jsonPrimitive.content, proof.sha256)
            assertEquals(bytes.toList(), Files.readAllBytes(path).toList())
        }
    }

    @Test
    fun descriptorWriterRejectsReplacedNamesChangedContentAndChangedAncestors() {
        for (change in listOf("entry", "content", "ancestor")) {
            val directory = temporaryFolder.newFolder(change)
            val io = NioTestFileTransactionIo()
            io.afterWrite = {
                val path = requireNotNull(directory.listFiles()).single().toPath()
                when (change) {
                    "entry" -> {
                        Files.move(path, path.resolveSibling("original.txt"))
                        Files.write(path, "source".toByteArray())
                        io.replaceIdentity(path.toString())
                    }
                    "content" -> Files.write(path, "edited".toByteArray())
                    "ancestor" -> io.replaceIdentity(directory.absolutePath)
                }
            }
            val writer = HistoryDescriptorMediaWriter(io)
            assertThrows(IOException::class.java) { writer.create(directory.toPath(), "created.txt", "source".toByteArray()).use {} }
            assertEquals(if (change == "content") "edited" else "source", File(directory, "created.txt").readText())
            if (change == "entry") assertEquals("source", File(directory, "original.txt").readText())
        }
    }

    @Test
    fun descriptorFingerprintEnforcesItsExplicitReadBudget() {
        val directory = temporaryFolder.newFolder()
        val io = NioTestFileTransactionIo()
        io.directory(directory.absolutePath).use { parent ->
            requireNotNull(io.file(parent, "created.bin", create = true)).use { file ->
                file.writeSynced(byteArrayOf(1, 2, 3)) { false }
                assertThrows(FileSnapshotLimitExceeded::class.java) { file.fingerprint(2) }
                assertEquals(3L, file.fingerprint(3).bytes)
            }
        }
    }

    @Test
    fun unavailableNativePrimitiveBecomesAnOrdinaryFailureAndReleasesItsRoute() {
        var closed = 0
        val writer = HistoryDescriptorMediaWriter(unavailableIo { closed++ })
        val failure = assertThrows(IOException::class.java) {
            writer.create(temporaryFolder.newFolder().toPath(), "source.txt", "source".toByteArray()).use {}
        }
        assertTrue(failure.cause is UnsatisfiedLinkError)
        assertEquals(1, closed)
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    @Test
    fun unavailableMediaPrimitiveReportsFailureWithoutStoppingHistoryWorker() = runTest {
        val root = temporaryFolder.newFolder()
        val paths = HistoryPaths(root, File(root, "history.json"), File(root, "settings.json"),
            File(root, "media").apply { mkdirs() }, false)
        val old = HistoryItem(1, "timestamp", HistoryType.TEXT, "old", "old.txt")
        File(paths.mediaDir, old.mediaPath).writeText("old-source")
        val original = Json.encodeToString(StoredHistoryDatabase(items = listOf(old)))
        paths.databaseFile.writeText(original)
        val persistence = HistoryPersistence(paths, Json, ::historyTestFileIdentity,
            HistoryDescriptorMediaWriter(unavailableIo()), NioTestFileTransactionIo())
        val repository = HistoryRepository(persistence, StandardTestDispatcher(testScheduler))
        repository.saveText("new", "new-source")
        advanceUntilIdle()
        assertEquals(HistoryStorageError.WRITE_FAILED, repository.state.value.storageError)
        assertEquals(listOf(old), repository.state.value.items)
        assertEquals(original, paths.databaseFile.readText())
        assertEquals("old-source", File(paths.mediaDir, old.mediaPath).readText())
        repository.updateMaxItems(10)
        advanceUntilIdle()
        assertEquals(10, repository.state.value.maxItems)
        assertEquals(10, persistence.loadSettings().maxItems)
        assertEquals(null, repository.state.value.storageError)
    }

    private fun unavailableIo(onClose: () -> Unit = {}): FileTransactionIo =
        object : FileTransactionIo by NioTestFileTransactionIo() {
            override fun directory(path: String): TransactionDirectory = object : TransactionDirectory {
                override val identity = FileObjectId(1, 1)
                override fun currentPath(): String = path
                override fun close() = onClose()
            }
            override fun directory(parent: TransactionDirectory, name: String): TransactionDirectory? =
                throw UnsatisfiedLinkError("Descriptor primitive unavailable")
        }

    private fun identity(io: NioTestFileTransactionIo, path: Path): String =
        io.directory(path.parent.toString()).use { parent ->
            requireNotNull(io.file(parent, path.fileName.toString())).use { file ->
                "${file.identity.device}:${file.identity.inode}"
            }
        }
}
