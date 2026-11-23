package dev.screengoated.toolbox.mobile.creation

import java.io.IOException
import dev.screengoated.toolbox.mobile.history.MAX_HISTORY_LIMIT
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationSvgEditTransactionTest {
    private fun receipt() = CreationSvgEditReceipt(
        entry = CreationHistoryEntry(
            id = "history-1", tool = "svg", sourcePath = "source", outputPath = "original",
            outputName = "result.svg", createdAtMs = 1,
            committedSize = 3, committedSha256 = "old-hash", committedIdentity = "old-id",
        ),
        originalCopyId = "original-copy",
        stagedPath = "staged", stagedIdentity = "stage-id", size = 5, sha256 = "new-hash",
        intent = CreationPublishIntent(kind = "downloads", finalName = "result-edited.svg", mimeType = "image/svg+xml"),
    )

    private class Fake(initial: CreationSvgEditReceipt) : CreationSvgEditOperations {
        var saved = initial
        var history = initial.entry
        var original = "old"
        var stageValid = true
        var failure: String? = null
        var published = false
        var publishCalls = 0
        var completions = 0
        var changeOriginalDuringPublish = false

        override fun originalMatches(entry: CreationHistoryEntry) = original == "old"
        override fun stagedMatches(receipt: CreationSvgEditReceipt) = stageValid
        override fun reserve(receipt: CreationSvgEditReceipt) = CreationPendingReservation("pending", "new-id")
        override fun populate(receipt: CreationSvgEditReceipt) {
            if (failure == "populate") throw IOException("populate")
        }
        override fun publish(receipt: CreationSvgEditReceipt): String {
            publishCalls++
            published = true
            if (changeOriginalDuringPublish) original = "external"
            if (failure == "publish-after") throw IOException("publish")
            return "published"
        }
        override fun publishedMatches(receipt: CreationSvgEditReceipt) =
            published && receipt.publishedPath == "published" && receipt.pendingIdentity == "new-id"
        override fun persist(receipt: CreationSvgEditReceipt) { saved = receipt }
        override fun complete(receipt: CreationSvgEditReceipt) { completions++ }
        fun commit(entry: CreationHistoryEntry, copy: CreationHistoryEntry) {
            if (failure == "history") throw IOException("history")
            assertEquals("original", copy.outputPath)
            assertEquals("original-copy", copy.id)
            history = entry
        }
    }

    @Test
    fun `failed or interrupted steps preserve original and replay one publication`() {
        for (cut in listOf("populate", "publish-after", "history")) {
            val initial = receipt()
            val fake = Fake(initial)
            fake.failure = cut
            assertThrows(IOException::class.java) {
                finishCreationSvgEdit(initial, fake.history, fake, fake::commit)
            }
            assertEquals("old", fake.original)
            assertEquals(initial.entry, fake.history)
            fake.failure = null
            val updated = finishCreationSvgEdit(fake.saved, fake.history, fake, fake::commit)
            assertEquals("published", updated.outputPath)
            assertEquals("new-id", updated.committedIdentity)
            assertEquals("new-hash", updated.committedSha256)
            assertEquals(5L, updated.committedSize)
            assertEquals("old", fake.original)
            assertEquals(1, fake.completions)
        }
    }

    @Test
    fun `changed source or staged bytes cannot commit history`() {
        val initial = receipt()
        val fake = Fake(initial)
        fake.stageValid = false
        assertThrows(IllegalArgumentException::class.java) {
            finishCreationSvgEdit(initial, fake.history, fake, fake::commit)
        }
        assertEquals(0, fake.publishCalls)
        fake.stageValid = true
        fake.changeOriginalDuringPublish = true
        assertThrows(IllegalArgumentException::class.java) {
            finishCreationSvgEdit(initial, fake.history, fake, fake::commit)
        }
        assertEquals(initial.entry, fake.history)
        assertEquals("external", fake.original)
        assertTrue(fake.published)
    }

    @Test
    fun `already committed history recovers without another publication`() {
        val initial = receipt()
        val fake = Fake(initial)
        val updated = finishCreationSvgEdit(initial, fake.history, fake, fake::commit)
        val calls = fake.publishCalls
        val recovered = finishCreationSvgEdit(fake.saved, updated, fake, fake::commit)
        assertEquals(updated, recovered)
        assertEquals(calls, fake.publishCalls)
    }

    @Test
    fun `editing a retained original does not mark the new copy as retained`() {
        val base = receipt()
        val original = retainedSvgOriginalEntry(CreationRetainedSvgOriginal(
            base.entry.outputPath, "old-id", 3, "old-hash", base.entry,
        ))
        val initial = base.copy(entry = original)
        val fake = Fake(initial)
        val updated = finishCreationSvgEdit(initial, fake.history, fake, fake::commit)
        assertTrue(!isRetainedSvgOriginal(updated))
    }

    @Test
    fun `retry resolves recovered copy by the original pending owner`() {
        val original = receipt()
        val recovered = original.entry.copy(
            outputPath = "published",
            outputName = "result-edited.svg",
            committedIdentity = "new-id",
            committedSize = 5,
            committedSha256 = "new-hash",
        )
        val backup = original.entry.copy(id = original.originalCopyId, dispatchId = null)
        assertEquals(recovered, resolveCreationSvgEditEntry(listOf(recovered, backup), "original", original.entry.id))
        assertThrows(IllegalArgumentException::class.java) {
            resolveCreationSvgEditEntry(listOf(recovered), "original", null)
        }
    }

    @Test
    fun `missing history owner fails closed during edit recovery`() {
        val pending = receipt()
        assertThrows(IllegalArgumentException::class.java) {
            requireCreationSvgEditOwner(emptyList(), pending)
        }
        assertEquals(pending.entry, requireCreationSvgEditOwner(listOf(pending.entry), pending))
    }

    @Test
    fun `unreadable edit journal fails closed`() {
        val directory = kotlin.io.path.createTempDirectory("svg-journal").toFile()
        try {
            val journal = java.io.File(directory, "edits.json")
            assertEquals(emptyList<CreationSvgEditReceipt>(), readCreationSvgEditReceipts(journal))
            journal.writeText("[broken")
            assertThrows(Exception::class.java) { readCreationSvgEditReceipts(journal) }
        } finally {
            directory.deleteRecursively()
        }
    }

    @Test
    fun `failed first journal write removes only proven unrecorded staging`() {
        val pending = receipt()
        assertTrue(canRemoveUnrecordedSvgStage(true, "staged", "stage-id", "stage-id") { emptyList() })
        assertTrue(!canRemoveUnrecordedSvgStage(true, "staged", "stage-id", "stage-id") { listOf(pending) })
        assertTrue(!canRemoveUnrecordedSvgStage(true, "staged", "stage-id", "stage-id") { throw IOException("unknown journal") })
        assertTrue(!canRemoveUnrecordedSvgStage(true, "staged", "stage-id", "replacement") { emptyList() })
        assertTrue(!canRemoveUnrecordedSvgStage(false, "staged", null, null) { emptyList() })
    }

    @Test
    fun `failed identity provider can clean an independently identified unrecorded stage`() {
        val staged = java.io.File.createTempFile("svg-stage-test", ".tmp")
        try {
            // Windows test hosts may not expose a file key; model a filesystem that does.
            val independent = creationStageFileKey(staged) ?: "simulated-file-key"
            val observed = creationStageFileKey(staged) ?: independent
            assertTrue(canRemoveUnrecordedSvgStage(false, staged.absolutePath, independent, observed) { emptyList() })
            assertTrue(staged.delete())
            assertTrue(!canRemoveUnrecordedSvgStage(false, staged.absolutePath, independent, creationStageFileKey(staged)) { emptyList() })
        } finally {
            staged.delete()
        }
    }

    @Test
    fun `saved copy remains newest at history and retention limits`() {
        val original = receipt().entry.copy(tool = CreationTool.IMAGE_TO_SVG.wireName)
        val entries = (0 until MAX_HISTORY_LIMIT).map { number ->
            original.copy(id = "entry-$number", createdAtMs = number.toLong() + 1)
        }
        val old = entries.first()
        val edited = old.copy(outputPath = "published", createdAtMs = creationSvgEditTimestamp(entries))
        val backup = old.copy(id = "backup", dispatchId = null)
        val revised = creationHistoryWithSvgCopy(entries, edited, backup)
        val bounded = boundedCreationHistory(revised)
        assertTrue(bounded.any { it.id == edited.id && it.outputPath == "published" })
        assertTrue(bounded.none { it.id == backup.id })
        val kept = planCreationHistoryRetention(
            bounded.map { CreationHistoryRetentionItem(it.id, it.tool, it.createdAtMs, emptySet()) },
            maximumPerTool = 1, budgetBytes = 0, protectedManagedPaths = emptySet(), sizeOf = { 0 },
        )
        assertEquals(setOf(edited.id), kept)
    }

    @Test
    fun `evicted original stays protected from history cleanup and managed pruning`() {
        val files = kotlin.io.path.createTempDirectory("svg-original-retention").toFile()
        try {
            val originalPath = java.io.File(files, "managed/original.svg").absolutePath
            val record = CreationRetainedSvgOriginal(originalPath, "inode", 5, "digest")
            val index = creationSvgOriginalsIndex(files)
            retainCreationSvgOriginal(index, record)
            retainCreationSvgOriginal(index, record)
            assertEquals(listOf(record), readRetainedSvgOriginals(index))
            assertTrue(originalPath in creationJournalProtectedPaths(files))
            assertTrue(originalPath in creationDurableProtectedPaths(files))
            assertTrue(creationDurableStateIsReadable(files))
            forgetRetainedSvgOriginal(index, record.copy(identity = "replacement"))
            assertEquals(listOf(record), readRetainedSvgOriginals(index))
            forgetRetainedSvgOriginal(index, record)
            assertEquals(emptyList<CreationRetainedSvgOriginal>(), readRetainedSvgOriginals(index))
            retainCreationSvgOriginal(index, record)
            index.writeText("[broken")
            assertTrue(!creationDurableStateIsReadable(files))
            assertThrows(CreationSvgOriginalsUnavailableException::class.java) { retainCreationSvgOriginal(index, record) }
        } finally {
            files.deleteRecursively()
        }
    }

    @Test
    fun `full original reservation index refuses another save without changing records`() {
        val directory = kotlin.io.path.createTempDirectory("svg-original-cap").toFile()
        try {
            val index = java.io.File(directory, "originals.json")
            index.writeText((0 until 4096).joinToString(prefix = "[", postfix = "]") {
                """{"outputPath":"original-$it","identity":"id-$it","size":1,"sha256":"digest"}"""
            })
            assertThrows(CreationSvgOriginalsUnavailableException::class.java) {
                retainCreationSvgOriginal(index, CreationRetainedSvgOriginal("new", "id", 1, "digest"))
            }
            assertEquals(4096, readRetainedSvgOriginals(index).size)
        } finally {
            directory.deleteRecursively()
        }
    }

    @Test
    fun `history switch preserves the original and is idempotent on recovery`() {
        val pending = receipt()
        val edited = pending.entry.copy(outputPath = "published")
        val backup = pending.entry.copy(id = pending.originalCopyId, dispatchId = null)
        val result = creationHistoryWithSvgCopy(listOf(pending.entry), edited, backup)
        assertEquals(listOf(edited, backup), result)
        assertEquals(result, creationHistoryWithSvgCopy(result, edited, backup))
    }

    @Test
    fun `offline and missing owners do not prevent other history recovery`() {
        val first = receipt()
        val missing = first.copy(entry = first.entry.copy(id = "missing"), originalCopyId = "missing-copy")
        val other = first.copy(
            entry = first.entry.copy(id = "other", outputPath = "other-original"),
            originalCopyId = "other-copy",
        )
        val initial = listOf(first.entry, other.entry)
        val attempted = mutableListOf<String>()
        val result = recoverCreationSvgEditEntries(initial, listOf(first, missing, other), {}) { pending, owner, commit ->
            attempted += owner.id
            if (owner.id == first.entry.id) throw IOException("offline")
            commit(owner.copy(outputPath = "other-edited"), owner.copy(id = pending.originalCopyId, dispatchId = null))
        }
        assertEquals(listOf("history-1", "other"), attempted)
        assertEquals("original", result.single { it.id == "history-1" }.outputPath)
        assertEquals("other-edited", result.single { it.id == "other" }.outputPath)
        assertEquals("other-original", result.single { it.id == "other-copy" }.outputPath)
    }

    @Test
    fun `committed edit updates its job and history before result delivery`() {
        val old = receipt().entry.copy(dispatchId = "dispatch")
        val updated = old.copy(outputPath = "published", outputName = "edited.svg")
        val item = CreationNativeItem(
            id = "item", batchId = "batch", sourcePath = "source", sourceName = "source",
            status = CreationJobStatus(stage = "done", progressText = "", dispatchId = "dispatch", outputPath = "original"),
        )
        val state = CreationNativeUiState(items = listOf(item), history = listOf(old))
        val rebound = state.withSavedSvgEdit("original", updated, null)
        assertEquals("published", rebound.items.single().status?.outputPath)
        assertEquals(updated, rebound.history.single())
    }
}
