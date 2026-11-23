package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.AtomicPublicationUnavailable
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionDirectory
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionFile
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileWriteCancelled
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.Paths
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class HistoryReceiptRecoveryTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test
    fun loadedTargetRemainsPinnedUntilReplacement() {
        val file = File(temporaryFolder.newFolder(), "history.json").apply { writeText("old") }
        val delegate = NioTestFileTransactionIo()
        var targetHandles = 0
        val io = object : FileTransactionIo by delegate {
            override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? {
                val opened = delegate.file(parent, name, create) ?: return null
                if (name != file.name || create) return opened
                targetHandles++
                return object : TransactionFile by opened {
                    override fun close() { try { opened.close() } finally { targetHandles-- } }
                }
            }
        }
        val storage = HistoryAtomicFiles(io)
        assertEquals("old", storage.read(file))
        assertEquals("The loaded target must stay pinned while its identity authorizes a write", 1, targetHandles)
        delegate.readFailure = { File(it).name == file.name }
        assertThrows(Exception::class.java) { storage.read(file) }
        assertEquals("A failed read must preserve the prior pin", 1, targetHandles)
        delegate.readFailure = { false }
        assertEquals("old", storage.read(file))
        assertEquals("A repeated read must replace its old pin", 1, targetHandles)
        delegate.beforePublish = { throw AtomicPublicationUnavailable(95) }
        assertEquals(HistoryWriteResult.REJECTED, storage.write(file, "new"))
        assertEquals("A rejected publication must keep the original pin", 1, targetHandles)
        delegate.beforePublish = {}
        assertEquals(HistoryWriteResult.COMMITTED, storage.write(file, "new"))
        assertEquals("The committed target must replace the old pin", 1, targetHandles)
        storage.close()
        storage.close()
        assertEquals(0, targetHandles)
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "later"))
    }

    @Test
    fun readOnlySnapshotDoesNotRetainHandlesOrAuthorizeWrites() {
        val file = File(temporaryFolder.newFolder(), "settings.json").apply { writeText("old") }
        val delegate = NioTestFileTransactionIo()
        var handles = 0
        val io = object : FileTransactionIo by delegate {
            override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? {
                val opened = delegate.file(parent, name, create) ?: return null
                handles++
                return object : TransactionFile by opened {
                    override fun close() { try { opened.close() } finally { handles-- } }
                }
            }
        }
        HistoryAtomicFiles(io, readOnlySnapshots = true).use { storage ->
            assertEquals("old", storage.read(file))
            assertEquals(0, handles)
            assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
            assertEquals("old", file.readText())
        }
        assertEquals(0, handles)
    }

    @Test
    fun sharedLoadedIdentityCasesFollowDescriptorLifetime() {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("history-ui/loaded-identity.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        val cases = contract.getValue("cases").jsonArray
        assertEquals(4, cases.size)
        for (element in cases) {
            val case = element.jsonObject
            val event = case.getValue("event").jsonPrimitive.content
            val file = File(temporaryFolder.newFolder(), "history.json").apply { writeText("old") }
            val delegate = NioTestFileTransactionIo()
            var handles = 0
            val io = object : FileTransactionIo by delegate {
                override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? {
                    val opened = delegate.file(parent, name, create) ?: return null
                    if (name != file.name || create) return opened
                    handles++
                    return object : TransactionFile by opened {
                        override fun close() { try { opened.close() } finally { handles-- } }
                    }
                }
            }
            val storage = HistoryAtomicFiles(io, readOnlySnapshots = event == "snapshot")
            try {
                assertEquals("old", storage.read(file))
                when (event) {
                    "commit" -> assertEquals(HistoryWriteResult.COMMITTED, storage.write(file, "new"))
                    "close" -> storage.close()
                    "read", "snapshot" -> Unit
                    else -> error("Unknown loaded identity case")
                }
                val expected = case.getValue("expected").jsonPrimitive.content
                val expectedHandles = when (expected) {
                    "target_pinned", "successor_pinned" -> 1
                    "no_retained_target" -> 0
                    else -> error("Unknown loaded identity result")
                }
                assertEquals(case.getValue("name").jsonPrimitive.content, expectedHandles, handles)
            } finally { storage.close() }
            assertEquals(0, handles)
        }
    }

    @Test
    fun repeatedCommittedWritesHaveBoundedRecoveryFiles() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json")
        val storage = HistoryAtomicFiles(NioTestFileTransactionIo())
        repeat(20) { index ->
            assertEquals(HistoryWriteResult.COMMITTED, storage.write(file, "value-$index"))
            assertEquals("value-$index", storage.read(file))
        }
        assertTrue("Committed writes accumulated recovery files", root.listFiles().orEmpty().size <= 3)
    }

    @Test
    fun reopeningRejectsAnUncertainForeignDisplacedVersion() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = NioTestFileTransactionIo()
        val storage = HistoryAtomicFiles(io)
        storage.read(file)
        io.beforePublish = { file.writeText("foreign"); io.replaceIdentity(file.path) }
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
        assertTrue(root.listFiles().orEmpty().any { it.readText() == "foreign" })
        assertThrows(IOException::class.java) { HistoryAtomicFiles(io).read(file) }
    }

    @Test
    fun sharedRecoveryCasesRequireExactKnownVersionsBeforeResuming() {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("history-ui/publication-recovery.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        assertEquals(7, contract.getValue("cases").jsonArray.size)
        for (element in contract.getValue("cases").jsonArray) {
            val case = element.jsonObject
            val root = temporaryFolder.newFolder()
            val file = File(root, "history.json").apply { writeText("old") }
            val io = NioTestFileTransactionIo()
            val storage = HistoryAtomicFiles(io)
            storage.read(file)
            when (case.getValue("fault").jsonPrimitive.content) {
                "unsupported_publication" -> io.beforePublish = { throw AtomicPublicationUnavailable(95) }
                "after_publication" -> io.afterPublish = { throw IOException("Unknown publication outcome") }
                "foreign_displaced", "identical_displaced" -> io.beforePublish = {
                    file.writeText(if (case.getValue("fault").jsonPrimitive.content == "identical_displaced") "old" else "foreign")
                    io.replaceIdentity(file.path)
                }
                "foreign_installed" -> io.afterPublish = { file.writeText("foreign"); io.replaceIdentity(file.path) }
                "partial_stage" -> io.writeFailureAfterForPath = { if (File(it).name == historyStageName(file.name)) 1 else null }
                "partial_receipt" -> io.writeFailureAfterForPath = { if (File(it).name == historyReceiptName(file.name)) 1 else null }
                else -> error("Unknown recovery vector")
            }
            val label = case.getValue("name").jsonPrimitive.content
            assertEquals(label, case.getValue("expected_write").jsonPrimitive.content, storage.write(file, "new").name)
            io.beforePublish = {}; io.afterPublish = {}; io.writeFailureAfterForPath = { null }
            val reopened = HistoryAtomicFiles(NioTestFileTransactionIo())
            val expected = case.getValue("expected_read").takeUnless { it == JsonNull }?.jsonPrimitive?.content
            if (expected == null) assertThrows(label, IOException::class.java) { reopened.read(file) }
            else assertEquals(label, expected, reopened.read(file))
            val retained = root.listFiles().orEmpty().associate { it.name to it.readBytes().toList() }
            assertEquals(label, case.getValue("expected_resume").jsonPrimitive.content, reopened.write(file, "later").name)
            assertTrue(label, root.listFiles().orEmpty().size <= 3)
            if (expected == null) assertEquals(label, retained, root.listFiles().orEmpty().associate { it.name to it.readBytes().toList() })
        }
    }

    @Test
    fun aCreatedSlotWithoutItsIdentityReceiptIsPreservedAndUnavailable() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = NioTestFileTransactionIo()
        io.beforeFileWrite = { path, bytes ->
            if (File(path).name == historyReceiptName(file.name) && phase(bytes) == "WRITING") throw AbruptStop()
        }
        assertThrows(AbruptStop::class.java) { HistoryAtomicFiles(io).write(file, "new") }
        val slot = File(root, historyStageName(file.name))
        assertTrue(slot.exists())
        io.beforeFileWrite = { _, _ -> }
        assertThrows(IOException::class.java) { HistoryAtomicFiles(io).read(file) }
        assertEquals(HistoryWriteResult.UNAVAILABLE, HistoryAtomicFiles(io).write(file, "later"))
        assertTrue(slot.exists())
        assertEquals("old", file.readText())
        assertEquals(3, root.listFiles().orEmpty().size)
    }

    @Test
    fun anIntentWithoutACreatedSlotCanResumeWithoutDeletingAnything() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = NioTestFileTransactionIo()
        val interrupted = object : FileTransactionIo by io {
            override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? {
                if (create && name == historyStageName(file.name)) throw AbruptStop()
                return io.file(parent, name, create)
            }
        }
        assertThrows(AbruptStop::class.java) { HistoryAtomicFiles(interrupted).write(file, "new") }
        val reopened = HistoryAtomicFiles(NioTestFileTransactionIo())
        assertEquals("old", reopened.read(file))
        assertEquals(HistoryWriteResult.COMMITTED, reopened.write(file, "later"))
        assertEquals("later", file.readText())
        assertEquals(3, root.listFiles().orEmpty().size)
    }

    @Test
    fun completeStageBytesWithAnInterruptedPreparedReceiptRemainKnownNoEffect() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = NioTestFileTransactionIo()
        io.beforeFileWrite = { path, bytes ->
            if (File(path).name == historyReceiptName(file.name) && phase(bytes) == "PREPARED") throw AbruptStop()
        }
        assertThrows(AbruptStop::class.java) { HistoryAtomicFiles(io).write(file, "new") }
        assertEquals("new", File(root, historyStageName(file.name)).readText())
        io.beforeFileWrite = { _, _ -> }
        val reopened = HistoryAtomicFiles(NioTestFileTransactionIo())
        assertEquals("old", reopened.read(file))
        assertEquals(HistoryWriteResult.COMMITTED, reopened.write(file, "later"))
        assertEquals(3, root.listFiles().orEmpty().size)
    }

    @Test
    fun recoveryOfAnAppliedRenameSyncsWithoutRewritingTheReceipt() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = NioTestFileTransactionIo()
        io.afterPublish = { throw IOException("Unknown publication result") }
        assertEquals(HistoryWriteResult.UNAVAILABLE, HistoryAtomicFiles(io).write(file, "new"))
        io.afterPublish = {}
        val receipt = File(root, historyReceiptName(file.name))
        val retained = receipt.readBytes().toList()
        io.beforeDirectorySync = { throw IOException("Directory sync unavailable") }
        assertThrows(IOException::class.java) { HistoryAtomicFiles(io).read(file) }
        var syncs = 0
        io.beforeDirectorySync = { syncs++ }
        assertEquals("new", HistoryAtomicFiles(io).read(file))
        assertTrue(syncs > 0)
        assertEquals(retained, receipt.readBytes().toList())
    }

    @Test
    fun identicalReceiptBytesOnAReplacementObjectDoNotAcquireOwnership() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json")
        val io = NioTestFileTransactionIo()
        val storage = HistoryAtomicFiles(io)
        assertEquals(HistoryWriteResult.COMMITTED, storage.write(file, "old"))
        val receipt = File(root, historyReceiptName(file.name))
        val bytes = receipt.readBytes()
        Files.move(receipt.toPath(), File(root, "retained-receipt").toPath())
        receipt.writeBytes(bytes)
        io.replaceIdentity(receipt.path)
        assertThrows(IOException::class.java) { HistoryAtomicFiles(io).read(file) }
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
        assertEquals(bytes.toList(), receipt.readBytes().toList())
        assertEquals("old", file.readText())
    }

    @Test
    fun unknownSlotsAndUnreadableReceiptsNeverAuthorizeReplacementOrReset() {
        for (receiptBytes in listOf<ByteArray?>(null, byteArrayOf(), "{broken".toByteArray(), byteArrayOf(0xC3.toByte(), 0x28))) {
            val root = temporaryFolder.newFolder()
            val file = File(root, "history.json").apply { writeText("old") }
            File(root, historyStageName(file.name)).writeText("unknown")
            if (receiptBytes != null) File(root, historyReceiptName(file.name)).writeBytes(receiptBytes)
            val before = root.listFiles().orEmpty().associate { it.name to it.readBytes().toList() }
            val storage = HistoryAtomicFiles(NioTestFileTransactionIo())
            assertThrows(IOException::class.java) { storage.read(file) }
            assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
            assertEquals(before, root.listFiles().orEmpty().associate { it.name to it.readBytes().toList() })
        }
    }

    @Test
    fun independentOwnersCannotWriteOrReadThroughAnActiveReceiptLease() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = NioTestFileTransactionIo()
        val first = HistoryAtomicFiles(io)
        val second = HistoryAtomicFiles(NioTestFileTransactionIo())
        first.read(file); second.read(file)
        val admitted = CountDownLatch(1)
        val release = CountDownLatch(1)
        val executor = Executors.newSingleThreadExecutor()
        io.beforeFileWrite = { path, _ ->
            if (File(path).name == historyStageName(file.name)) {
                admitted.countDown()
                check(release.await(5, TimeUnit.SECONDS))
            }
        }
        try {
            val result = executor.submit<HistoryWriteResult> { first.write(file, "first") }
            assertTrue(admitted.await(5, TimeUnit.SECONDS))
            assertEquals(HistoryWriteResult.UNAVAILABLE, second.write(file, "second"))
            assertThrows(IOException::class.java) { second.read(file) }
            assertEquals("old", file.readText())
            release.countDown()
            assertEquals(HistoryWriteResult.COMMITTED, result.get(5, TimeUnit.SECONDS))
            assertEquals("first", second.read(file))
            assertEquals(HistoryWriteResult.COMMITTED, second.write(file, "second"))
            assertEquals(3, root.listFiles().orEmpty().size)
        } finally {
            release.countDown()
            executor.shutdownNow()
        }
    }

    @Test
    fun recyclingRefusesSlotsAndReceiptsWithAdditionalNamespaceLinks() {
        for (linkedReceipt in listOf(false, true)) {
            val root = temporaryFolder.newFolder()
            val file = File(root, "history.json")
            val io = NioTestFileTransactionIo()
            val storage = HistoryAtomicFiles(io)
            assertEquals(HistoryWriteResult.COMMITTED, storage.write(file, "first"))
            assertEquals(HistoryWriteResult.COMMITTED, storage.write(file, "second"))
            val linked = if (linkedReceipt) historyReceiptName(file.name) else historyStageName(file.name)
            io.fileLinkCount = { if (File(it).name == linked) 2 else 1 }
            val before = root.listFiles().orEmpty().associate { it.name to it.readBytes().toList() }
            assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "third"))
            assertEquals(before, root.listFiles().orEmpty().associate { it.name to it.readBytes().toList() })
        }
    }

    @Test
    fun writableDescriptorsReplaceWholeBuffersAndHonorCancellationBeforeTruncation() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "payload").apply { writeText("long original contents") }
        val io = NioTestFileTransactionIo()
        io.directory(root.path).use { parent ->
            requireNotNull(io.writableFile(parent, file.name)).use { opened ->
                assertThrows(FileWriteCancelled::class.java) { opened.writeSynced("x".toByteArray()) { true } }
                assertEquals("long original contents", file.readText())
                opened.writeSynced("short".toByteArray()) { false }
                assertEquals("short", file.readText())
                opened.writeSynced(byteArrayOf()) { false }
                assertEquals(0L, file.length())
            }
        }
    }

    private fun phase(bytes: ByteArray): String {
        val envelope = Json.parseToJsonElement(bytes.decodeToString()).jsonObject
        return Json.parseToJsonElement(envelope.getValue("payload").jsonPrimitive.content)
            .jsonObject.getValue("phase").jsonPrimitive.content
    }

    private class AbruptStop : Error("Interrupted storage operation")
}
