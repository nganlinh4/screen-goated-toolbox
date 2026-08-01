package dev.screengoated.toolbox.mobile.creation

import java.io.ByteArrayInputStream
import java.io.File
import kotlin.io.path.createTempDirectory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationFileCleanupContractTest {
    @Test
    fun `cleanup transaction protects original and quarantine paths durably`() {
        val files = createTempDirectory("creation-cleanup-index").toFile()
        val state = File(files, "creation/pending-cleanup.json")
        writeCreationIndexTextAtomically(
            state,
            """
                [{
                  "artifactPath":"C:/managed/original.bin",
                  "quarantinePath":"C:/managed/.original.bin.cleanup-1"
                }]
            """.trimIndent(),
            CREATION_CLEANUP_INDEX_MAX_BYTES,
        )

        assertEquals(
            setOf(
                "C:/managed/original.bin",
                "C:/managed/.original.bin.cleanup-1",
            ),
            creationDurableProtectedPaths(files),
        )

        state.delete()
        state.parentFile?.delete()
        files.delete()
    }

    @Test
    fun `cleanup without a precommitted proof never deletes isolated bytes`() {
        assertEquals(
            CreationCleanupDecision.RESTORE,
            decideCreationCleanup(
                null, null, 5L, "a".repeat(64), false, null, "replacement",
            ),
        )
        assertEquals(
            CreationCleanupDecision.RELINQUISH,
            decideCreationCleanup(
                null, null, 5L, "a".repeat(64), true, null, "replacement",
            ),
        )
    }

    @Test
    fun `cleanup deletes only exact precommitted bytes and stable identity`() {
        val digest = "b".repeat(64)
        assertEquals(
            CreationCleanupDecision.DELETE,
            decideCreationCleanup(5L, digest, 5L, digest, false, "inode-1", "inode-1"),
        )
        assertEquals(
            CreationCleanupDecision.RESTORE,
            decideCreationCleanup(5L, digest, 6L, digest, false, "inode-1", "inode-1"),
        )
        assertEquals(
            CreationCleanupDecision.RESTORE,
            decideCreationCleanup(
                5L, digest, 5L, "c".repeat(64), false, "inode-1", "inode-1",
            ),
        )
    }

    @Test
    fun `identical replacement before cleanup queue is not owned`() {
        assertFalse(creationCleanupIdentityMatches("inode-original", "inode-replacement"))
        assertTrue(creationCleanupIdentityMatches("inode-original", "inode-original"))
    }

    @Test
    fun `identical replacement before cleanup delete is restored instead of deleted`() {
        val digest = "b".repeat(64)
        assertEquals(
            CreationCleanupDecision.RESTORE,
            decideCreationCleanup(
                5L,
                digest,
                5L,
                digest,
                originalExists = false,
                expectedIdentity = "inode-original",
                actualIdentity = "inode-replacement",
            ),
        )
        assertEquals(
            CreationCleanupDecision.RELINQUISH,
            decideCreationCleanup(
                5L,
                digest,
                5L,
                digest,
                originalExists = true,
                expectedIdentity = "inode-original",
                actualIdentity = "inode-replacement",
            ),
        )
    }

    @Test
    fun `cleanup crash after resolved move replays only the journaled exact replacement`() {
        val record = CreationPendingCleanup(
            artifactPath = "creation/library/result.png",
            quarantinePath = "creation/library/.result.png.cleanup",
            expectedIdentity = "inode-original",
            resolution = "relinquish",
            replacementPath = "creation/relinquished/transaction-result.png",
            replacementIdentity = "inode-original",
        )

        assertTrue(creationCleanupResolutionCanFinish(record, "inode-original"))
        assertFalse(creationCleanupResolutionCanFinish(record, "inode-replacement"))
        assertFalse(
            creationCleanupResolutionCanFinish(
                record.copy(replacementPath = null),
                "inode-original",
            ),
        )
    }

    @Test
    fun `modified result cleanup reattaches the relinquished bytes without trusted proof`() {
        val original = CreationHistoryEntry(
            id = "history-1",
            tool = "image",
            sourcePath = "",
            outputPath = "creation/library/result.png",
            outputName = "result.png",
            createdAtMs = 1,
            committedSize = 4,
            committedSha256 = "a".repeat(64),
        )
        val recovered = reattachedCreationHistoryEntry(
            CreationPendingCleanup(
                artifactPath = original.outputPath,
                quarantinePath = "creation/library/.result.png.cleanup",
                resolution = CREATION_REATTACH_RESOLUTION,
                replacementPath = "creation/relinquished/result.png",
                retainedHistoryEntry = original,
            ),
        )

        assertEquals("creation/relinquished/result.png", recovered?.outputPath)
        assertEquals(null, recovered?.committedSize)
        assertEquals(null, recovered?.committedSha256)
        assertEquals(null, recovered?.committedIdentity)
    }

    @Test
    fun `presentation previews reuse content keys and prune within both caps`() {
        val first = creationPresentationPreviewKey {
            ByteArrayInputStream("same-source".encodeToByteArray())
        }
        val retry = creationPresentationPreviewKey {
            ByteArrayInputStream("same-source".encodeToByteArray())
        }
        assertEquals(first, retry)

        val artifacts = (0 until 530).map { index ->
            CreationPresentationArtifact(
                path = "preview-$index",
                lastModifiedMs = index.toLong(),
                sizeBytes = 1024L * 1024,
            )
        }
        val protected = (510 until 530).mapTo(mutableSetOf()) { "preview-$it" }
        val removed = planCreationPresentationPrune(
            artifacts,
            protected,
            nowMs = 1_000,
            maximumFiles = 512,
            maximumBytes = 256L * 1024 * 1024,
            retentionMs = Long.MAX_VALUE,
        )
        val retained = artifacts.filter { it.path !in removed }

        assertTrue(protected.none(removed::contains))
        assertTrue(retained.size <= 512)
        assertTrue(retained.sumOf(CreationPresentationArtifact::sizeBytes) <= 256L * 1024 * 1024)
    }

    @Test
    fun `pre-journal job input crash is reclaimed only after grace`() {
        val filesDir = createTempDirectory("creation-job-input-orphan").toFile()
        val root = File(filesDir, "creation/job-inputs").apply { mkdirs() }
        val orphan = File(root, "image_orphan").apply { mkdirs() }
        File(orphan, "0.img").writeBytes(byteArrayOf(1))
        orphan.setLastModified(1)

        assertTrue(
            reconcileCreationJobInputDirectories(
                filesDir,
                nowMs = JOB_INPUT_ORPHAN_GRACE_MS + 2,
            ).not(),
        )
        assertFalse(orphan.exists())
        filesDir.deleteRecursively()
    }

    @Test
    fun `concurrent job input cleanup accepts an already removed target`() {
        val filesDir = createTempDirectory("creation-job-input-race").toFile()
        val root = File(filesDir, "creation/job-inputs").apply { mkdirs() }
        val removed = File(root, "already-removed")

        assertTrue(deleteCreationJobInputOrConfirmAbsent(root, removed))
        filesDir.deleteRecursively()
    }

    @Test
    fun `job input cleanup removes a direct ordinary directory`() {
        val filesDir = createTempDirectory("creation-job-input-delete").toFile()
        val root = File(filesDir, "creation/job-inputs").apply { mkdirs() }
        val target = File(root, "orphan").apply { mkdirs() }
        File(target, "0.img").writeBytes(byteArrayOf(1, 2, 3))

        assertTrue(deleteCreationJobInputOrConfirmAbsent(root, target))
        assertFalse(target.exists())
        filesDir.deleteRecursively()
    }

    @Test
    fun `job input reconciliation remains bounded beyond four thousand directories`() {
        val directories = (0 until 4_105).map { index ->
            CreationJobInputDirectory("job-$index", index.toLong())
        }
        val owned = setOf("job-1", "job-4")
        val fresh = "job-4104"
        val removed = planCreationJobInputReconciliation(
            directories,
            owned,
            nowMs = 10_000,
            graceMs = 6_000,
            maximumDeletes = 4_096,
        )

        assertTrue(owned.none(removed::contains))
        assertFalse(fresh in removed)
        assertTrue(removed.size <= 4_096)
        assertTrue(removed.isNotEmpty())
    }

    @Test
    fun `generic pruning never infers ownership for managed result library bytes`() {
        val files = File("files")
        val cache = File("cache")
        val roots = creationManagedArtifactRoots(files, cache)
        val library = File(files, "creation/library")

        assertFalse(library in creationPrunableArtifactRoots(roots, library))
        assertTrue(File(files, "creation/staging") in creationPrunableArtifactRoots(roots, library))
    }
}
