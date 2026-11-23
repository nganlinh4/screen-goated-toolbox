package dev.screengoated.toolbox.mobile.creation

import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationRetainedSvgOriginalTest {
    private fun original() = CreationHistoryEntry(
        id = "original", tool = CreationTool.IMAGE_TO_SVG.wireName,
        sourcePath = "source", outputPath = "original.svg", outputName = "original.svg",
        createdAtMs = 1, committedIdentity = "file-id", committedSize = 5, committedSha256 = "digest",
    )

    @Test
    fun retainedOriginalRemainsVisibleBeyondOrdinaryHistoryAndDoesNotDuplicate() {
        val original = original()
        val backup = original.copy(id = "backup")
        val record = CreationRetainedSvgOriginal("original.svg", "file-id", 5, "digest", backup)
        val edited = original.copy(id = "edited", outputPath = "edited.svg")
        val merged = mergeRetainedSvgOriginals(listOf(edited), listOf(record))
        assertEquals(listOf("edited", "backup"), merged.map { it.id })
        assertTrue(isRetainedSvgOriginal(merged.last()))
        assertEquals(merged, mergeRetainedSvgOriginals(merged, listOf(record)))
        assertEquals(listOf(original), mergeRetainedSvgOriginals(listOf(original), listOf(record)))
    }

    @Test
    fun retainedRenameUpdatesProofIdempotentlyAndRejectsUnverifiedDestinations() {
        val dir = Files.createTempDirectory("retained-rename").toFile()
        try {
            val index = java.io.File(dir, "index.json")
            val old = original()
            val record = CreationRetainedSvgOriginal("original.svg", "file-id", 5, "digest", old)
            retainCreationSvgOriginal(index, record)
            val renamed = retainedSvgOriginalEntry(record).copy(
                outputPath = "renamed.svg", outputName = "renamed.svg", committedIdentity = "new-id",
            )
            assertThrows(IllegalArgumentException::class.java) {
                reconcileRetainedSvgRenames(index, listOf(renamed)) { false }
            }
            assertEquals(listOf(record), readRetainedSvgOriginals(index))
            reconcileRetainedSvgRenames(index, listOf(renamed)) { true }
            reconcileRetainedSvgRenames(index, listOf(renamed)) { true }
            val updated = readRetainedSvgOriginals(index).single()
            assertEquals("renamed.svg", updated.outputPath)
            assertEquals("new-id", updated.identity)
            assertEquals(old.id, retainedSvgOriginalEntry(updated).id)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun renameReceiptAllowsHistoryRecoveryAfterRetainedIndexWasUpdatedFirst() {
        val old = original()
        val renamed = old.copy(outputPath = "renamed.svg", outputName = "renamed.svg", committedIdentity = "new-id")
        val record = CreationRetainedSvgOriginal("renamed.svg", "new-id", 5, "digest", renamed)
        val receipt = CreationHistoryRenameReceipt(
            transactionId = "rename", entryId = old.id, oldPath = old.outputPath, oldName = old.outputName,
            targetName = renamed.outputName, expectedSize = 5, expectedSha256 = "digest", oldIdentity = "file-id",
            newPath = renamed.outputPath, newIdentity = "new-id",
        )
        assertThrows(IllegalArgumentException::class.java) { mergeRetainedSvgOriginals(listOf(old), listOf(record)) }
        assertThrows(IllegalArgumentException::class.java) {
            mergeRetainedSvgOriginals(listOf(old), listOf(record), renames = listOf(receipt.copy(newIdentity = "other")))
        }
        val merged = mergeRetainedSvgOriginals(listOf(old), listOf(record), renames = listOf(receipt)).single()
        assertTrue(isRetainedSvgOriginal(merged))
        assertTrue(creationRenameRecoveryMustCommitHistory(receipt, merged))
        assertTrue(creationRenameArtifactIsVerified(receipt, "new-id", 5, "digest"))
        assertEquals(retainedSvgOriginalEntry(record), creationEntryWithCompletedRename(merged, receipt))
    }

    @Test
    fun legacyRetainedProofHasAStableVisibleIdentityAndCorruptionFailsClosed() {
        val dir = Files.createTempDirectory("retained-legacy").toFile()
        try {
            val index = creationSvgOriginalsIndex(dir)
            val record = CreationRetainedSvgOriginal("original.svg", "file-id", 5, "digest")
            retainCreationSvgOriginal(index, record)
            assertEquals(retainedSvgOriginalEntry(record).id, retainedSvgOriginalEntry(record).id)
            assertTrue(isRetainedSvgOriginal(retainedSvgOriginalEntry(record)))
            index.writeText("[broken")
            assertTrue(runCatching { readRetainedSvgOriginals(index) }.isFailure)
            assertFalse(creationDurableStateIsReadable(dir))
            index.delete()
            index.mkdir()
            assertTrue(runCatching { readRetainedSvgOriginals(index) }.isFailure)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun pendingOwnerKeepsExactReceiptFieldsUntilTheCopyCommits() {
        val old = original().copy(dispatchId = "dispatch")
        val record = CreationRetainedSvgOriginal(old.outputPath, "file-id", 5, "digest", old)
        assertEquals(old, mergeRetainedSvgOriginals(listOf(old), listOf(record), setOf(old.id)).single())
        assertEquals(old, mergeRetainedSvgOriginals(listOf(old), listOf(record), null).single())
        assertTrue(isRetainedSvgOriginal(mergeRetainedSvgOriginals(listOf(old), listOf(record)).single()))
    }

    @Test
    fun committingTransfersRetainedIdentityAndReplayIsIdempotent() {
        val dir = Files.createTempDirectory("retained-transfer").toFile()
        try {
            val index = java.io.File(dir, "index.json")
            val old = original().copy(outputPath = java.io.File(dir, "original.svg").canonicalPath)
            retainCreationSvgOriginal(index, CreationRetainedSvgOriginal(old.outputPath, "file-id", 5, "digest", old))
            val copy = old.copy(id = "backup", dispatchId = null)
            transferRetainedSvgOriginal(index, old, copy)
            transferRetainedSvgOriginal(index, old, copy)
            val retained = retainedSvgOriginalEntry(readRetainedSvgOriginals(index).single())
            assertEquals("backup", retained.id)
            val edited = old.copy(outputPath = java.io.File(dir, "edited.svg").path)
            assertEquals(listOf(edited, retained), creationHistoryWithSvgCopy(listOf(old, retained), edited, copy))
        } finally {
            dir.deleteRecursively()
        }
    }
}
