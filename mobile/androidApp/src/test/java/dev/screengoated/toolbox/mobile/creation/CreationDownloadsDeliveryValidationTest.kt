package dev.screengoated.toolbox.mobile.creation

import java.io.ByteArrayInputStream
import java.io.File
import kotlin.io.path.createTempDirectory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationDownloadsDeliveryValidationTest {
    @Test
    fun `published media keeps a readable proof path without becoming app managed`() {
        val downloads = "content://media/external/downloads/7"
        val document =
            "content://com.android.externalstorage.documents/document/primary%3ADownload"
        val managed = "/data/user/0/app/files/creation/result.png"
        assertTrue(isUserOwnedCreationOutputPath(downloads))
        assertTrue(isUserOwnedCreationOutputPath(document))
        assertFalse(isUserOwnedCreationOutputPath(managed))
        assertEquals(downloads, creationCommittedProofPath(downloads, null))
        assertEquals(managed, creationCommittedProofPath(managed, managed))
        assertEquals(null, creationCommittedProofPath("/outside/result.png", null))
    }

    @Test
    fun `downloads reservation preserves the final artifact extension`() {
        val token = "a".repeat(32)
        assertEquals("sgt-$token.pending.svg", creationDownloadsPendingName(token, "result.svg"))
        assertEquals("sgt-$token.pending.glb", creationDownloadsPendingName(token, "model.GLB"))
    }

    @Test
    fun `downloads final name does not rely on visibility of an old base name`() {
        val result = uniqueCreationDownloadsName("result.svg", { false }, "dispatch-123")
        assertEquals("result-dispatch123.svg", result)
    }

    @Test
    fun `stream proof verifies size when provider metadata is unavailable`() {
        val bytes = "verified output".encodeToByteArray()
        val digest = java.security.MessageDigest.getInstance("SHA-256")
            .digest(bytes)
            .joinToString("") { "%02x".format(it) }
        assertTrue(creationStreamMatchesProof(ByteArrayInputStream(bytes), bytes.size.toLong(), digest))
        assertFalse(creationStreamMatchesProof(ByteArrayInputStream(bytes), bytes.size + 1L, digest))
    }

    @Test
    fun `delivery receipt accepts only the system downloads destination`() {
        val filesDir = createTempDirectory("creation-downloads-record").toFile()
        val output = File(filesDir, "creation/staging/svg/result.svg").absolutePath
        val unsigned = CreationWorkerRequest(
            jobId = "svg-job",
            dispatchId = "svg-dispatch",
            sourceDescriptors = listOf(
                CreationSourceDescriptor("creation/job-inputs/source.png", 4, "b".repeat(64)),
            ),
            tool = CreationTool.IMAGE_TO_SVG.wireName,
            operation = "generate",
            imagePath = "creation/job-inputs/source.png",
            imagePaths = listOf("creation/job-inputs/source.png"),
            outputPath = output,
            outputName = "result.svg",
        )
        val request = unsigned.copy(requestFingerprint = creationRequestFingerprint(unsigned))
        val token = "a".repeat(32)
        val valid = CreationDeliveryRecord(
            dispatchId = request.dispatchId,
            engineId = "svg-0",
            ownerId = "owner",
            request = request,
            current = CreationJobStatus(
                jobId = request.jobId,
                stage = "finalizing",
                progressText = "working",
            ),
            event = CreationWorkerEvent(
                jobId = request.jobId,
                event = "success",
                outputPath = output,
            ),
            sealedPath = output,
            mimeType = "image/svg+xml",
            segmented = false,
            canSegment = false,
            artifactSize = 128,
            artifactSha256 = "b".repeat(64),
            intent = CreationPublishIntent(
                kind = "downloads",
                destination = CREATION_DOWNLOADS_DESTINATION,
                finalName = "result.svg",
                mimeType = "image/svg+xml",
                pendingName = creationDownloadsPendingName(token, "result.svg"),
                reservationToken = token,
            ),
        )

        assertTrue(validCreationDeliveryRecord(filesDir, valid))
        assertFalse(
            validCreationDeliveryRecord(
                filesDir,
                valid.copy(intent = valid.intent.copy(destination = "content://example/downloads")),
            ),
        )
        assertFalse(
            validCreationDeliveryRecord(
                filesDir,
                valid.copy(intent = valid.intent.copy(pendingName = "result.pending")),
            ),
        )
        assertFalse(
            validCreationDeliveryRecord(
                filesDir,
                valid.copy(intent = valid.intent.copy(targetPath = output)),
            ),
        )
        filesDir.deleteRecursively()
    }
}
