package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.security.MessageDigest
import kotlin.io.path.createTempDirectory
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationQueuedRecoveryTest {
    @Test
    fun restoredManagedRequestsKeepTheirFrozenSafDestinations() {
        fixture().getValue("availabilityCases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val root = createTempDirectory("creation-queued-recovery").toFile()
            try {
                val records = case.getValue("available").jsonArray.mapIndexed { index, available ->
                    record(root, index, "content://storage/tree/${available.jsonPrimitive.boolean}-$index")
                }
                val before = Json.encodeToString(records)
                val restored = restore(root, records, 2L)
                assertEquals(case.getValue("expectedRetained").jsonArray.map { it.jsonPrimitive.int },
                    records.indices.filter { index -> restored.any { it.request.jobId == records[index].request.jobId } })
                assertEquals(records.map { it.request }, restored.map { it.request })
                assertEquals(records.map { it.destination }, restored.map { it.destination })
                assertEquals(before, Json.encodeToString(records))
            } finally {
                root.deleteRecursively()
            }
        }
    }

    @Test
    fun expiredAcceptedRequestRemainsVisibleWithoutAnOutputArtifact() {
        val case = fixture().getValue("transitions").jsonArray.map { it.jsonObject }
            .single { it.getValue("deadlineExpired").jsonPrimitive.boolean }
        val root = createTempDirectory("creation-queued-expiry").toFile()
        try {
            val saved = record(root, 0, "content://storage/tree/unavailable")
            val restored = restore(root, listOf(saved), saved.request.deadlineAtMs + 1L)
            assertEquals(1, restored.size)
            assertEquals(case.getValue("expected").jsonPrimitive.content, restored.single().status.stage)
            assertEquals(saved.request, restored.single().request)
            assertTrue(restored.single().status.error != null)
        } finally {
            root.deleteRecursively()
        }
    }

    private fun restore(root: File, records: List<CreationJournalRecord>, nowMs: Long) =
        restoreCreationManagerRecords(records, nowMs, 10_000L, { request ->
            validateRestoredCreationRequest(root, request,
                sizeOf = { File(it).length() }, sha256Of = { sha256(File(it)) })
        }, { path ->
            check(!path.startsWith("content://")) { "Restoration must not probe the output provider" }
            File(path).isFile
        })

    private fun record(root: File, index: Int, destination: String): CreationJournalRecord {
        val jobId = "3d_saved_$index"
        val input = File(root, "creation/job-inputs/$jobId/source.png").apply {
            requireNotNull(parentFile).mkdirs()
            writeText("reference")
        }
        val output = reserveCreationStagingFile(File(root, "creation/staging/3d"), "result-$index.glb")
        val unsigned = CreationDeliveryTestFixture.request(jobId, "dispatch_$jobId").copy(
            imagePath = input.absolutePath,
            sourceDescriptors = listOf(CreationSourceDescriptor(input.absolutePath, input.length(), sha256(input))),
            outputPath = output.absolutePath, outputName = output.name,
            acceptedAtMs = 1L, deadlineAtMs = 1L + CreationContract.MAXIMUM_JOB_RUNTIME_MS,
        )
        val request = unsigned.copy(requestFingerprint = creationRequestFingerprint(unsigned))
        check(validateRestoredCreationRequest(root, request, { File(it).length() }, { sha256(File(it)) }))
        return CreationJournalRecord("owner", request, CreationJobStatus(
            jobId = jobId, dispatchId = request.dispatchId, operation = request.operation,
            stage = "queued", progressText = "Queued", generationMode = request.generationMode,
            sourceImagePath = input.absolutePath, sourceImagePaths = listOf(input.absolutePath),
        ), 1L, destination)
    }

    private fun sha256(file: File) = MessageDigest.getInstance("SHA-256").digest(file.readBytes())
        .joinToString("") { "%02x".format(it) }

    private fun fixture() = Json.parseToJsonElement(fixtureFile().readText()).jsonObject

    private fun fixtureFile(): File {
        var directory: File? = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (directory != null) {
            val candidate = File(directory, "parity-fixtures/creation/queued-recovery.json")
            if (candidate.isFile) return candidate
            directory = directory.parentFile
        }
        error("Queued recovery fixture not found")
    }
}
