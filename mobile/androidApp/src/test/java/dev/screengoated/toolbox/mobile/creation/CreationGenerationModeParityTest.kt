package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationGenerationModeParityTest {
    private val json = Json { encodeDefaults = true }
    private fun fixture(): kotlinx.serialization.json.JsonObject {
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) { it.parentFile }
            .first { File(it, "parity-fixtures").isDirectory }
        return json.parseToJsonElement(File(root, "parity-fixtures/image-to-3d/generation-mode-contract.json").readText()).jsonObject
    }

    @Test
    fun `new mode admission and limits match canonical Windows cases`() {
        val fixture = fixture()
        assertEquals(CreationContract.MINIMUM_POLYCOUNT, fixture.getValue("minimumPolycount").jsonPrimitive.int)
        assertEquals(CreationContract.MAXIMUM_POLYCOUNT, fixture.getValue("maximumPolycount").jsonPrimitive.int)
        for (entry in fixture.getValue("newRequests").jsonArray) {
            val row = entry.jsonObject
            val result = runCatching {
                newCreation3dRoute(buildJsonObject {
                    put("generationMode", row.getValue("mode"))
                    put("polycount", row.getValue("polycount"))
                    put("autoSegment", row.getValue("autoSegment"))
                })
            }
            val expected = row.getValue("normalizedPolycount")
            assertEquals(expected != kotlinx.serialization.json.JsonNull, result.isSuccess)
            if (result.isSuccess) assertEquals(expected.jsonPrimitive.int, result.getOrThrow().polycount)
        }
    }

    @Test
    fun `frozen mode validation preserves accepted requests without clamping`() {
        for (entry in fixture().getValue("frozenRequests").jsonArray) {
            val row = entry.jsonObject
            val result = runCatching {
                CreationContract.frozen3dMode(
                    CreationGenerationMode.fromWireName(row.getValue("mode").jsonPrimitive.content),
                    row.getValue("polycount").jsonPrimitive.int,
                    row.getValue("autoSegment").jsonPrimitive.boolean,
                )
            }
            assertEquals(row.getValue("valid").jsonPrimitive.boolean, result.isSuccess)
            if (result.isSuccess) assertEquals(row.getValue("polycount").jsonPrimitive.int, result.getOrThrow().polycount)
        }
    }

    @Test
    fun `accepted Fast request keeps its fingerprint and worker identity after serialization`() {
        val unsigned = CreationWorkerRequest(jobId = "3d_saved", dispatchId = "dispatch_3d_saved", tool = "3d", operation = "generate",
            generationMode = "fast", polycount = 100, autoSegment = false, topology = "triangle", imagePath = "source.png", outputPath = "result.glb", outputName = "result.glb")
        val request = unsigned.copy(requestFingerprint = creationRequestFingerprint(unsigned))
        val decoded = json.decodeFromString<CreationWorkerRequest>(json.encodeToString(request))
        assertEquals(request, decoded)
        assertEquals(request.requestFingerprint, creationRequestFingerprint(decoded))
        assertEquals("3d-0", dev.screengoated.toolbox.mobile.creation.worker.creationRequiredWorkerKey(CreationTool.IMAGE_TO_3D, decoded))
    }

    @Test
    fun `completed and restored continuations preserve generation settings through refinements`() {
        for (mode in listOf("quality", "fast")) {
            val request = CreationWorkerRequest(jobId = "saved", tool = "3d", operation = "generate",
                generationMode = mode, polycount = if (mode == "fast") 100 else 12000,
                autoSegment = mode == "quality", topology = "triangle", instruction = "Keep the outline",
                segmentationLevel = "simple", imagePath = "source.png", outputPath = "result.glb", outputName = "result.glb")
            val restored = json.decodeFromString<CreationWorkerRequest>(json.encodeToString(request))
            val status = CreationJobStatus(stage = "done", progressText = "Done", segmentationLevel = "simple")
            val continuation = CreationContinuation("owner", "engine", "token", "source.png", "result.glb", "result.glb", 1L)
                .withGeneration(restored, status)
            for (kind in listOf("materials", "separate_balanced")) {
                val refined = request.copy(operation = "refine").withFrozenGeneration(continuation, kind)
                assertEquals(request.generationMode, refined.generationMode)
                assertEquals(request.polycount, refined.polycount)
                assertEquals(request.autoSegment, refined.autoSegment)
                assertEquals(request.topology, refined.topology)
                assertEquals(request.instruction, refined.instruction)
                assertEquals(if (kind == "materials") "simple" else "balanced", refined.segmentationLevel)
                assertEquals("simple", continuation.segmentationLevel)
            }
        }
    }

    @Test
    fun `accepted queued work is active and keeps its frozen presentation`() {
        val status = CreationJobStatus(jobId = "saved", dispatchId = "dispatch_saved", stage = "queued", progressText = "Waiting",
            generationMode = "fast", polycount = 100, autoSegment = false, topology = "triangle", sourceImagePath = "source.png")
        val item = recoverCreationNativeItem(CreationTool.IMAGE_TO_3D, status) { false }
        assertEquals(CreationNativeStage.RUNNING, item.stage)
        assertEquals(item, routeCreationNativeItem(CreationTool.IMAGE_TO_3D, item) { false })
        val refreshed = applyCreationRuntimeCapabilities(CreationTool.IMAGE_TO_3D, item) { false }
        assertEquals("fast", refreshed.generationMode)
        assertEquals(100, refreshed.polycount)
        // The instruction echo is gone with the image tool: a recovered job
        // offers instructions only when its mode supports them at runtime.
        assertFalse(refreshed.allowsInstruction)
        assertFalse(refreshed.isConfigurable())
        val request = CreationWorkerRequest(jobId = "saved", dispatchId = "dispatch_saved", tool = "3d",
            operation = "generate", generationMode = "fast", polycount = 100, autoSegment = false,
            imagePath = "source.png", outputPath = "result.glb", outputName = "result.glb")
        val record = CreationJournalRecord("owner", request, status.copy(operation = "generate"), 1L)
        assertTrue(restoredCreationRecordIsBounded(record, 2L))
        assertEquals(emptyList<CreationJournalRecord>(), boundedRestorableCreationRecords(listOf(record), 2) { false })
        assertEquals(listOf(record), boundedRestorableCreationRecords(listOf(record), 2) { true })
    }
}
