package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.AtomicPublicationUnavailable
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
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
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** Namespace fault injection, not Android/JNI atomicity or power-loss evidence. */
class HistoryPublicationRaceTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test
    fun aReplacementAfterTheLastObservationCannotBeSilentlyDestroyed() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = NioTestFileTransactionIo()
        val storage = HistoryAtomicFiles(io)
        storage.read(file)
        io.beforePublish = {
            Files.move(file.toPath(), File(root, "original.json").toPath())
            file.writeText("foreign")
            io.replaceIdentity(file.path)
        }
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
        assertTrue(root.listFiles().orEmpty().any { it.readText() == "foreign" })
        assertEquals("old", File(root, "original.json").readText())
    }

    @Test
    fun sharedPublicationCasesAuditInstalledAndActualDisplacedVersions() {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("history-ui/publication.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        assertEquals(5, contract.getValue("cases").jsonArray.size)
        for (element in contract.getValue("cases").jsonArray) {
            val case = element.jsonObject
            val root = temporaryFolder.newFolder()
            val file = File(root, "history.json")
            val initial = case.getValue("initial").takeUnless { it == JsonNull }?.jsonPrimitive?.content
            val competing = case.getValue("competing").takeUnless { it == JsonNull }?.jsonPrimitive?.content
            if (initial != null) file.writeText(initial)
            val io = NioTestFileTransactionIo()
            val storage = HistoryAtomicFiles(io)
            assertEquals(initial, storage.read(file))
            if (competing != null) io.beforePublish = {
                file.writeText(competing)
                io.replaceIdentity(file.path)
            }
            val label = case.getValue("name").jsonPrimitive.content
            assertEquals(label, case.getValue("expected").jsonPrimitive.content, storage.write(file, "new").name)
            if (competing != null && initial == null) assertEquals(label, competing, file.readText())
            else assertEquals(label, "new", file.readText())
            val retained = case.getValue("retained").takeUnless { it == JsonNull }?.jsonPrimitive?.content
            if (retained != null) assertTrue(label, root.listFiles().orEmpty().any { it.readText() == retained })
            if (initial == null && competing == null) assertEquals(setOf(file.name, historyReceiptName(file.name)),
                root.listFiles().orEmpty().map { it.name }.toSet())
        }
    }

    @Test
    fun anExceptionAfterExchangeIsUncertainAndRetainsBothVersions() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = NioTestFileTransactionIo()
        val storage = HistoryAtomicFiles(io)
        storage.read(file)
        io.afterPublish = { throw IOException("Unknown syscall outcome") }
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
        assertEquals("new", file.readText())
        assertTrue(root.listFiles().orEmpty().any { it.readText() == "old" })
    }

    @Test
    fun unsupportedAtomicPublicationHasNoReplacementFallback() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = NioTestFileTransactionIo()
        val storage = HistoryAtomicFiles(io)
        storage.read(file)
        io.beforePublish = { throw AtomicPublicationUnavailable(95) }
        assertEquals(HistoryWriteResult.REJECTED, storage.write(file, "new"))
        assertEquals("old", file.readText())
        assertTrue(root.listFiles().orEmpty().any { it.readText() == "new" })
    }

    @Test
    fun aChangedStagingNameIsNeverMistakenForTheCreationDescriptor() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = NioTestFileTransactionIo()
        val storage = HistoryAtomicFiles(io)
        storage.read(file)
        io.beforePublish = {
            val stage = File(root, historyStageName(file.name))
            stage.writeText("foreign stage")
            io.replaceIdentity(stage.path)
        }
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
        assertTrue(root.listFiles().orEmpty().any { it.readText() == "foreign stage" })
        assertTrue(root.listFiles().orEmpty().any { it.readText() == "old" })
    }

    @Test
    fun ancestorChangesAreCheckedBeforeAndAfterNamespacePublication() {
        for (afterPublication in listOf(false, true)) {
            val root = temporaryFolder.newFolder()
            val file = File(root, "history.json").apply { writeText("old") }
            val io = NioTestFileTransactionIo()
            val storage = HistoryAtomicFiles(io)
            storage.read(file)
            var publications = 0
            io.beforePublish = { publications++ }
            if (afterPublication) io.afterPublish = { io.replaceIdentity(root.path) }
            else io.afterWrite = { io.replaceIdentity(root.path) }
            assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
            assertEquals(if (afterPublication) 1 else 0, publications)
            assertEquals(if (afterPublication) "new" else "old", file.readText())
            assertTrue(root.listFiles().orEmpty().any { it.readText() == "old" })
        }
    }

    @Test
    fun failedPostPublicationObservationCannotBecomeSuccessOrCleanup() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = NioTestFileTransactionIo()
        val storage = HistoryAtomicFiles(io)
        storage.read(file)
        io.afterPublish = { io.readFailure = { true } }
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
        assertEquals("new", file.readText())
        assertTrue(root.listFiles().orEmpty().any { it.readText() == "old" })
    }

    @Test
    fun snapshotBudgetRejectsOversizeReadsAndWritesWithoutChangingTheTarget() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("oversize") }
        val storage = HistoryAtomicFiles(NioTestFileTransactionIo(), maximumSnapshotBytes = 4)
        assertThrows(IOException::class.java) { storage.read(file) }
        assertEquals("oversize", file.readText())
        val absent = File(root, "absent.json")
        assertNull(storage.read(absent))
        assertEquals(HistoryWriteResult.REJECTED, storage.write(absent, "oversize"))
        assertFalse(absent.exists())
    }

    @Test
    fun absentStorageIsReadWithoutCreatingDirectories() {
        val root = File(temporaryFolder.newFolder(), "missing/nested")
        val file = File(root, "history.json")
        val storage = HistoryAtomicFiles(NioTestFileTransactionIo())
        assertNull(storage.read(file))
        assertFalse(root.exists())
        assertEquals(HistoryWriteResult.COMMITTED, storage.write(file, "new"))
        assertEquals("new", storage.read(file))
    }

    @Test
    fun anObservedAncestorReplacementInvalidatesTheLoadedVersion() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "history.json").apply { writeText("old") }
        val io = NioTestFileTransactionIo()
        val storage = HistoryAtomicFiles(io)
        storage.read(file)
        io.replaceIdentity(root.path)
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
        assertEquals("old", file.readText())
        assertEquals(1, root.listFiles().orEmpty().size)
    }

    @Test
    fun everyDescriptorClosesWhenStorageClosesAcrossPublicationOutcomes() {
        for (fault in listOf("none", "partial_write", "post_publication", "invalid_utf8", "changed_read")) {
            val root = temporaryFolder.newFolder()
            val file = File(root, "history.json").apply { writeText("old") }
            val io = CountingIo()
            val storage = HistoryAtomicFiles(io)
            if (fault == "invalid_utf8") file.writeBytes(byteArrayOf(0xC3.toByte(), 0x28))
            if (fault == "invalid_utf8") assertThrows(IOException::class.java) { storage.read(file) }
            else {
                assertEquals("old", storage.read(file))
                assertEquals(1, io.opened)
                if (fault == "partial_write") io.delegate.writeFailureAfterForPath = {
                    if (File(it).name == historyStageName(file.name)) 1 else null
                }
                if (fault == "post_publication") io.delegate.afterPublish = { throw IOException("Unknown result") }
                if (fault == "changed_read") io.afterRead = { io.delegate.replaceIdentity(file.path) }
                if (fault == "changed_read") assertThrows(IOException::class.java) { storage.read(file) }
                else assertEquals(when (fault) {
                    "none" -> HistoryWriteResult.COMMITTED
                    "partial_write" -> HistoryWriteResult.UNAVAILABLE
                    else -> HistoryWriteResult.UNAVAILABLE
                }, storage.write(file, "new"))
            }
            assertEquals(fault, if (fault == "invalid_utf8") 0 else 1, io.opened)
            storage.close()
            assertEquals(fault, 0, io.opened)
        }
    }

    @Test
    fun missingNativeBindingsAreStorageErrorsRatherThanEscapingTheWorkerBoundary() {
        val root = temporaryFolder.newFolder()
        val io = object : FileTransactionIo by NioTestFileTransactionIo() {
            override fun directory(path: String): TransactionDirectory = throw UnsatisfiedLinkError("Unavailable")
        }
        val file = File(root, "history.json").apply { writeText("old") }
        val storage = HistoryAtomicFiles(io)
        assertThrows(IOException::class.java) { storage.read(file) }
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(file, "new"))
        assertEquals("old", file.readText())
        val absent = File(root, "absent.json")
        assertNull(storage.read(absent))
        assertEquals(HistoryWriteResult.UNAVAILABLE, storage.write(absent, "new"))
        assertFalse(absent.exists())
    }

    private class CountingIo : FileTransactionIo {
        val delegate = NioTestFileTransactionIo()
        var opened = 0
        var afterRead: () -> Unit = {}
        private inner class Directory(val inner: TransactionDirectory) : TransactionDirectory by inner {
            init { opened++ }
            override fun close() { inner.close(); opened-- }
        }
        override fun directory(path: String): TransactionDirectory = Directory(delegate.directory(path))
        override fun directory(parent: TransactionDirectory, name: String): TransactionDirectory? =
            delegate.directory((parent as Directory).inner, name)?.let { Directory(it) }
        override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? {
            val file = delegate.file((parent as Directory).inner, name, create) ?: return null
            return wrap(file)
        }
        override fun writableFile(parent: TransactionDirectory, name: String): TransactionFile? =
            delegate.writableFile((parent as Directory).inner, name)?.let(::wrap)
        private fun wrap(file: TransactionFile): TransactionFile {
            opened++
            return object : TransactionFile by file {
                override fun readBounded(maximum: Int): ByteArray = file.readBounded(maximum).also { afterRead() }
                override fun close() { file.close(); opened-- }
            }
        }
        override fun makeDirectory(parent: TransactionDirectory, name: String) = delegate.makeDirectory((parent as Directory).inner, name)
        override fun publish(parent: TransactionDirectory, stage: String, target: String, exchange: Boolean) =
            delegate.publish((parent as Directory).inner, stage, target, exchange)
    }
}
