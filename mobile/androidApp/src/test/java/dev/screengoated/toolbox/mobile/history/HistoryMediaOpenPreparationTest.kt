package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionDirectory
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionFile
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileFingerprint
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileObjectId
import java.io.File
import java.io.IOException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class HistoryMediaOpenPreparationTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test fun legacyAndProvedItemsUseTheOpenedDescriptor() {
        val root = temporaryFolder.newFolder()
        File(root, "source.txt").writeText("source")
        val delegate = NioTestFileTransactionIo()
        var opens = 0
        val io = object : FileTransactionIo by delegate {
            override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? {
                val opened = delegate.file(parent, name, create) ?: return null
                opens++
                return object : TransactionFile by opened {
                    override fun close() { try { opened.close() } finally { opens-- } }
                }
            }
        }
        val legacy = HistoryItem(1, "time", HistoryType.TEXT, "result", "source.txt")
        val proof = requireNotNull(observeHistoryMedia(io, root.toPath(), legacy))
        assertEquals(0, opens)
        assertEquals(6L, proof.size)
        assertEquals(historyDigest("source".toByteArray()), proof.sha256)
        assertEquals(proof, observeHistoryMedia(io, root.toPath(), legacy.copy(mediaProof = proof)))
        delegate.replaceIdentity(File(root, "source.txt").path)
        assertNull(observeHistoryMedia(io, root.toPath(), legacy.copy(mediaProof = proof)))
        assertNull(observeHistoryMedia(io, root.toPath(), legacy.copy(mediaPath = "../source.txt")))
        assertNull(observeHistoryMedia(io, root.toPath(), legacy.copy(itemType = HistoryType.IMAGE)))
        assertEquals(0, opens)
    }

    @Test fun replacementAfterPreparationCannotAcquireTheDescriptorProof() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "source.txt").apply { writeText("source") }
        val delegate = NioTestFileTransactionIo()
        var swapped = false
        val io = object : FileTransactionIo by delegate {
            override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? {
                val opened = delegate.file(parent, name, create) ?: return null
                return object : TransactionFile by opened {
                    override fun fingerprint(maximum: Int): FileFingerprint {
                        val result = opened.fingerprint(maximum)
                        if (!swapped) {
                            swapped = true
                            file.writeText("foreign")
                            delegate.replaceIdentity(file.path)
                        }
                        return result
                    }
                }
            }
        }
        val item = HistoryItem(1, "time", HistoryType.TEXT, "result", file.name)
        val proof = requireNotNull(observeHistoryMedia(io, root.toPath(), item))
        val identity = proof.identity.split(':').map(String::toLong)
        HistoryDirectoryRoute(delegate, root.toPath()).use { route ->
            assertThrows(IOException::class.java) {
                openVerifiedHistoryMedia(delegate, route, file.name, FileObjectId(identity[0], identity[1]),
                    FileFingerprint(proof.size, proof.sha256), proof.size.toInt()).close()
            }
        }
        assertEquals("foreign", file.readText())
    }
}
