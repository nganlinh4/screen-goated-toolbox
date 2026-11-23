package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.creation.CreationDeliveryTestFixture.prepared
import dev.screengoated.toolbox.mobile.creation.CreationDeliveryTestFixture.record
import dev.screengoated.toolbox.mobile.creation.CreationDeliveryTestFixture.request
import java.io.File
import kotlin.io.path.createTempDirectory
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationDeliveryContractTest {
    @Test
    fun `storage reserves internal staging separately from the selected destination`() {
        val mib = 1024L * 1024
        val saf = creationStorageRequirements(
            sourceSnapshotBytes = 100 * mib,
            resultBytes = 64 * mib,
            pendingInternalBytes = 12 * mib,
            pendingDestinationBytes = 8 * mib,
        )
        val sourceLess = creationStorageRequirements(
            sourceSnapshotBytes = 0,
            resultBytes = 64 * mib,
        )

        assertEquals(176 * mib, saf.internalBytes)
        assertEquals(72 * mib, saf.destinationBytes)
        assertEquals(64 * mib, sourceLess.internalBytes)
        assertEquals(64 * mib, sourceLess.destinationBytes)
    }

    @Test
    fun `external storage fails closed and preserves the free reserve boundary`() {
        val gib = 1024L * 1024 * 1024
        assertFalse(creationExternalStorageAccepted(null, 1))
        assertFalse(creationExternalStorageAccepted(gib, 1))
        assertTrue(creationExternalStorageAccepted(gib + 1, 1))
    }

    @Test
    fun `durable pending jobs reserve both internal and external delivery domains`() {
        val filesDir = createTempDirectory("creation-storage-reservations").toFile()
        val stateDir = File(filesDir, "creation/state").apply { mkdirs() }
        val activeRequest = request()
        val active = CreationJournalRecord(
            ownerId = "owner",
            request = activeRequest,
            status = CreationJobStatus(
                jobId = activeRequest.jobId,
                stage = "generating",
                progressText = "working",
            ),
            startedAtMs = 1,
            destination = null,
        )
        val safPrepared = prepared(request("image_job", "dispatch_image_job")).copy(
            request = request("image_job", "dispatch_image_job").copy(
                tool = CreationTool.IMAGE_TO_3D.wireName,
            ),
            mimeType = "image/png",
        )
        val safDelivery = record(safPrepared).copy(
            intent = CreationPublishIntent(
                kind = "saf",
                destination = "content://storage/tree/root",
                finalName = "result.png",
                mimeType = "image/png",
                pendingName = ".pending",
            ),
        )
        File(stateDir, "accepted-jobs.json").writeText(Json.encodeToString(listOf(active)))
        File(stateDir, "deliveries.json").writeText(Json.encodeToString(listOf(safDelivery)))

        val reservations = creationPendingStorageReservations(filesDir) { path ->
            if (path == activeRequest.outputPath) 10L else 0L
        }

        assertEquals(
            CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES * 2 - 10,
            reservations.internalBytes,
        )
        assertEquals(
            safDelivery.artifactSize,
            reservations.destinationBytes.getValue("content://storage/tree/root"),
        )
        filesDir.deleteRecursively()
    }

    @Test
    fun `prepared delivery reserves only bytes not already materialized`() {
        val artifactBytes = 1_024L
        assertEquals(
            artifactBytes,
            creationMissingDeliveryBytes(
                artifactBytes, null, publicationPrepared = false,
            ) { error("must not query") },
        )
        assertEquals(
            artifactBytes - 12,
            creationMissingDeliveryBytes(
                artifactBytes, "content://pending", publicationPrepared = false,
            ) { 12L },
        )
        assertEquals(
            0L,
            creationMissingDeliveryBytes(
                artifactBytes, "content://pending", publicationPrepared = true,
            ) { error("prepared bytes must not be reserved twice") },
        )
        assertEquals(
            0L,
            creationMissingDeliveryBytes(
                artifactBytes, "content://pending", publicationPrepared = false,
            ) { artifactBytes + 1 },
        )
    }

    @Test
    fun `same dispatch cannot substitute a different request or owner`() {
        val prepared = prepared()
        val saved = record(prepared)

        assertTrue(
            creationDeliveryMatchesPrepared(
                saved,
                prepared,
                saved.artifactSize,
                saved.artifactSha256,
            ),
        )
        assertFalse(
            creationDeliveryMatchesPrepared(
                saved,
                prepared.copy(ownerId = "another-owner"),
                saved.artifactSize,
                saved.artifactSha256,
            ),
        )
        assertFalse(
            creationDeliveryMatchesPrepared(
                saved,
                prepared.copy(
                    request = request(jobId = "3d_another"),
                ),
                saved.artifactSize,
                saved.artifactSha256,
            ),
        )
    }

    @Test
    fun `same size staging mutation fails the sealed artifact proof`() {
        val directory = createTempDirectory("creation-delivery-proof").toFile()
        val staging = File(directory, "result.glb").apply { writeBytes(byteArrayOf(1, 2, 3, 4)) }
        val digest = creationFileSha256(staging)

        assertTrue(creationFileMatchesProof(staging, 4, digest))
        staging.writeBytes(byteArrayOf(4, 3, 2, 1))
        assertFalse(creationFileMatchesProof(staging, 4, digest))

        staging.delete()
        directory.delete()
    }

    @Test
    fun `interrupted sealed cleanup remains durable until replay confirms deletion`() {
        val prepared = prepared()
        val committed = record(prepared).copy(historyCommitted = true)
        val uncommitted = record(
            prepared.copy(request = request("3d_second", "dispatch_3d_second")),
        )

        assertEquals(
            listOf(committed, uncommitted),
            retainCreationDeliveryRecords(listOf(committed, uncommitted)) { true },
        )
        assertEquals(
            listOf(uncommitted),
            retainCreationDeliveryRecords(listOf(committed, uncommitted)) { false },
        )
    }

    @Test
    fun `cancellation fence survives the full seven day recovery boundary`() {
        val now = CREATION_CANCELLATION_RETENTION_MS + 10
        val boundary = CreationCancellationFence(
            "job",
            "dispatch",
            "a".repeat(64),
            now - CREATION_CANCELLATION_RETENTION_MS,
        )
        val expired = boundary.copy(
            dispatchId = "expired",
            createdAtMs = boundary.createdAtMs - 1,
        )

        assertEquals(
            listOf(boundary),
            retainCreationCancellationFences(listOf(boundary, expired), now),
        )
    }

    @Test
    fun `cancellation compaction never drops unresolved work`() {
        val protected = CreationCancellationFence(
            "job",
            "protected",
            "a".repeat(64),
            0,
        )
        val records = (0 until 16_384).map { index ->
            protected.copy(
                jobId = "job-$index",
                dispatchId = "dispatch-$index",
                createdAtMs = index.toLong(),
            )
        } + protected

        val compacted = compactCreationCancellationFences(records, setOf("protected"))

        assertEquals(16_384, compacted.size)
        assertTrue(compacted.any { it.dispatchId == "protected" })
        assertFalse(compacted.any { it.dispatchId == "dispatch-0" })
    }

    @Test
    fun `cancellation retention rejects far future clock skew unless unresolved`() {
        val now = 1_000_000L
        val future = CreationCancellationFence(
            "job",
            "future",
            "a".repeat(64),
            now + 10L * 60 * 1_000,
        )
        assertTrue(retainCreationCancellationFences(listOf(future), now).isEmpty())
        assertEquals(
            listOf(future),
            retainCreationCancellationFences(listOf(future), now, setOf("future")),
        )
    }

    @Test
    fun `rename recovery commits history before old byte cleanup at every cut`() {
        val entry = CreationHistoryEntry(
            id = "history",
            tool = "3d",
            sourcePath = "source.png",
            outputPath = "creation/library/old.glb",
            outputName = "old.glb",
            createdAtMs = 1,
        )
        val receipt = CreationHistoryRenameReceipt(
            transactionId = "rename",
            entryId = entry.id,
            oldPath = entry.outputPath,
            oldName = entry.outputName,
            targetName = "new.glb",
            expectedSize = 4,
            expectedSha256 = "a".repeat(64),
            newPath = "creation/library/new.glb",
            newIdentity = "inode-new",
            committed = true,
        )

        assertTrue(creationRenameRecoveryMustCommitHistory(receipt, entry))
        assertFalse(
            creationRenameRecoveryMustCommitHistory(
                receipt,
                entry.copy(
                    outputPath = requireNotNull(receipt.newPath),
                    outputName = receipt.targetName,
                ),
            ),
        )
        assertTrue(
            creationRenameArtifactIsVerified(
                receipt,
                actualIdentity = "inode-new",
                actualSize = 4,
                actualSha256 = "a".repeat(64),
            ),
        )
        assertFalse(
            creationRenameArtifactIsVerified(
                receipt,
                actualIdentity = "inode-replacement",
                actualSize = 4,
                actualSha256 = "a".repeat(64),
            ),
        )
        assertFalse(
            creationRenameArtifactIsVerified(
                receipt,
                actualIdentity = "inode-new",
                actualSize = 4,
                actualSha256 = "b".repeat(64),
            ),
        )
        val paired = receipt.copy(
            companion = CreationHistoryCompanionRenameReceipt(
                oldPath = "creation/library/old.fbx",
                oldName = "old.fbx",
                targetName = "new.fbx",
                expectedSize = 8,
                expectedSha256 = "c".repeat(64),
                oldIdentity = "fbx-old",
                newPath = "creation/library/new.fbx",
                newIdentity = "fbx-new",
            ),
        )
        assertTrue(creationRenameRecoveryMustCommitHistory(paired, entry))
        val primaryOnly = entry.copy(
            outputPath = requireNotNull(paired.newPath),
            outputName = paired.targetName,
        )
        assertTrue(creationRenameRecoveryMustCommitHistory(paired, primaryOnly))
        val fullyRenamed = primaryOnly.copy(
            metadata = JsonObject(
                mapOf(
                    "download" to JsonObject(
                        mapOf(
                            "path" to JsonPrimitive("creation/library/new.fbx"),
                            "name" to JsonPrimitive("new.fbx"),
                        ),
                    ),
                ),
            ),
        )
        assertFalse(creationRenameRecoveryMustCommitHistory(paired, fullyRenamed))
        val committed = creationEntryWithCompletedRename(entry, paired)
        assertEquals("creation/library/new.glb", committed.outputPath)
        assertEquals("new.glb", committed.outputName)
        assertEquals("creation/library/new.fbx", committed.companionOutputPath())
        assertEquals("new.fbx", committed.companionOutputName())
        assertEquals("inode-new", committed.committedIdentity)
        assertEquals("fbx-new", committed.companionCommittedIdentity)
    }

    @Test
    fun `delivery failure retries only after a durable receipt exists`() {
        assertEquals(
            CreationDeliveryFailureAction.FAIL_JOB,
            creationDeliveryFailureAction(hasDurableReceipt = false),
        )
        assertEquals(
            CreationDeliveryFailureAction.RETRY,
            creationDeliveryFailureAction(hasDurableReceipt = true),
        )
    }

    @Test
    fun `stable id SAF rename replay accepts its still resolvable old handle only`() {
        assertTrue(
            creationSafRenameRecoveryMatches(
                "provider:doc-1",
                oldHandleExists = true,
                oldHandleIdentity = "provider:doc-1",
                targetIdentity = "provider:doc-1",
            ),
        )
        assertFalse(
            creationSafRenameRecoveryMatches(
                "provider:doc-1",
                oldHandleExists = true,
                oldHandleIdentity = "provider:replacement",
                targetIdentity = "provider:doc-1",
            ),
        )
    }

    @Test
    fun `SAF probe cleanup never claims a same-name concurrent document`() {
        assertTrue(creationSafProbeOwns("probe-created", "probe-created"))
        assertFalse(creationSafProbeOwns("probe-created", "concurrent-document"))
        assertFalse(creationSafProbeOwns(null, "concurrent-document"))
    }

    @Test
    fun `segmentation completion releases job inputs but never the prior artifact`() {
        val request = request().copy(
            operation = "segment",
            previousOutputPath = "creation/library/original.glb",
        )

        assertEquals(
            listOf("creation/job-inputs/source.png"),
            creationJobInputPathsReleasedAfterCommit(request),
        )
        assertFalse(
            requireNotNull(request.previousOutputPath) in
                creationJobInputPathsReleasedAfterCommit(request),
        )
    }

    @Test
    fun `completed 3d history keeps every frozen generation setting`() {
        val request = request().copy(
            operation = "generate",
            generationMode = "quality",
            polycount = 9_000,
            autoSegment = true,
            instruction = "Keep the silhouette",
        )
        val metadata = creationHistoryMetadata(
            FinishedCreation(
                request = request,
                status = CreationJobStatus(
                    jobId = request.jobId,
                    stage = "done",
                    progressText = "ready",
                    faces = 40,
                    vertices = 20,
                ),
                segmented = true,
                publishedPath = "creation/library/result.glb",
                continuation = null,
            ),
        )

        assertEquals("generate", metadata.getValue("operation").jsonPrimitive.content)
        assertEquals("quality", metadata.getValue("generationMode").jsonPrimitive.content)
        assertEquals(9_000, metadata.getValue("polycount").jsonPrimitive.int)
        assertTrue(metadata.getValue("autoSegment").jsonPrimitive.boolean)
        assertEquals(
            "Keep the silhouette",
            metadata.getValue("instruction").jsonPrimitive.content,
        )
        assertEquals(40, metadata.getValue("faces").jsonPrimitive.int)
        assertEquals(20, metadata.getValue("vertices").jsonPrimitive.int)
    }

    @Test
    fun `delivery receipt validation rejects identity path and stage tampering`() {
        val filesDir = createTempDirectory("creation-delivery-record").toFile()
        val output = File(filesDir, "creation/staging/3d/result.glb").absolutePath
        val unsigned = request().copy(outputPath = output)
        val request = unsigned.copy(requestFingerprint = creationRequestFingerprint(unsigned))
        val token = "a".repeat(32)
        val valid = record(prepared(request)).copy(
            sealedPath = output,
            artifactSize = 128,
            artifactSha256 = "b".repeat(64),
            intent = CreationPublishIntent(
                kind = "managed",
                finalName = "result.glb",
                mimeType = "model/gltf-binary",
                targetPath = File(filesDir, "creation/library/result.glb").absolutePath,
                pendingPath = File(filesDir, "creation/library/.sgt-$token.delivery").absolutePath,
                reservationToken = token,
            ),
        )

        assertTrue(validCreationDeliveryRecord(filesDir, valid))
        assertFalse(validCreationDeliveryRecord(filesDir, valid.copy(dispatchId = "other")))
        assertFalse(
            validCreationDeliveryRecord(
                filesDir,
                valid.copy(sealedPath = File(filesDir, "result.glb").absolutePath),
            ),
        )
        assertFalse(
            validCreationDeliveryRecord(
                filesDir,
                valid.copy(transactionStage = "published", publicationPrepared = false),
            ),
        )
        filesDir.deleteRecursively()
    }


}
