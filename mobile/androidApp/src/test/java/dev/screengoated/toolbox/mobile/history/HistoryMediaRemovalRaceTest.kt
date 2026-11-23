package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo
import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class HistoryMediaRemovalRaceTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test
    fun replacementAfterFinalIdentityCheckIsNotDeleted() {
        replaceDuringRetirement("foreign")
    }

    @Test
    fun pathnameLookupDoesNotAuthorizeAReplacementAfterFinalIdentityCheck() {
        val root = temporaryFolder.newFolder()
        val file = File(root, "source.txt").apply { writeText("owned") }
        val original = File(temporaryFolder.newFolder(), "original.txt")
        val proof = HistoryMediaProof(historyTestFileIdentity(file.toPath()), 5, historyDigest("owned".toByteArray()))
        val item = HistoryItem(1, "timestamp", HistoryType.TEXT, "result", file.name, proof)
        var observations = 0
        val result = historyMediaFile(root, item) { path ->
            val observed = historyTestFileIdentity(path)
            if (++observations == 2) {
                Files.move(path, original.toPath())
                file.writeText("foreign")
            }
            observed
        }
        assertNull("A replaced entry was returned as the owned media file", result)
        assertEquals("foreign", file.readText())
        assertEquals("owned", original.readText())
    }

    @Test
    fun identicalBytesDoNotMakeAReplacementOwnedForRemoval() {
        replaceDuringRetirement("owned")
    }

    @Test
    fun reoccupiedOriginalNamePreservesBothForeignEntries() {
        replaceDuringRetirement("foreign", reoccupy = true)
    }

    private fun replaceDuringRetirement(replacement: String, reoccupy: Boolean = false) {
        val root = temporaryFolder.newFolder()
        val media = File(root, "media").apply { mkdirs() }
        val file = File(media, "source.txt").apply { writeText("owned") }
        val original = File(root, "original.txt")
        val io = NioTestFileTransactionIo()
        val proof = HistoryMediaProof(historyDescriptorTestIdentity(io, file.toPath()), 5, historyDigest("owned".toByteArray()))
        val item = HistoryItem(1, "timestamp", HistoryType.TEXT, "result", file.name, proof)
        var moves = 0
        io.beforeMove = {
            if (++moves == 1) {
                Files.move(file.toPath(), original.toPath())
                file.writeText(replacement)
                io.replaceIdentity(file.path)
            }
        }
        io.afterMove = {
            if (reoccupy && moves == 1) {
                file.writeText("other")
                io.replaceIdentity(file.path)
            }
        }
        HistoryMediaStore(media, { historyDescriptorTestIdentity(io, it) }, fileIo = io).remove(item)
        assertEquals(2, moves)
        assertTrue("A replacement file was deleted", file.exists())
        assertEquals("owned", original.readText())
        val retired = File(root, ".history-retired").listFiles().orEmpty()
        if (reoccupy) {
            assertEquals("other", file.readText())
            assertEquals(replacement, retired.single().readText())
        } else {
            assertEquals(replacement, file.readText())
            assertTrue(retired.isEmpty())
        }
    }
}
