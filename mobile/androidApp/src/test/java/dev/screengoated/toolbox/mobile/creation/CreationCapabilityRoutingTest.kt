package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeProductDescriptor
import dev.screengoated.toolbox.mobile.creation.runtime.isCompatibleCreationRuntimeManifest
import dev.screengoated.toolbox.mobile.creation.runtime.runtimeSupportsOptionalInstruction
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationCapabilityRoutingTest {
    @Test
    fun `unknown failure text is replaced at the public boundary`() {
        assertEquals(
            "generic",
            publicCreationErrorText(
                "private backend detail",
                storageUnavailable = "storage",
                sourceUnavailable = "source",
                genericFailure = "generic",
            ),
        )
    }

    @Test
    fun `optional 3d instruction is capped at the host boundary`() {
        assertEquals("rounded handle", normalizedCreationInstruction(" rounded handle ", true))
        assertEquals(null, normalizedCreationInstruction("ignored", false))
        assertThrows(IllegalArgumentException::class.java) {
            normalizedCreationInstruction(
                "x".repeat(CreationContract.MAXIMUM_OPTIONAL_INSTRUCTION_CHARACTERS + 1),
                true,
            )
        }
    }

    @Test
    fun `private capacity failures collapse to the public unexpected category`() {
        assertEquals("unexpected", publicCreationFailureCategory("cooldown"))
        assertEquals("unexpected", publicCreationFailureCategory("rate_limit"))
        assertEquals("runtime_unavailable", publicCreationFailureCategory("runtime_unavailable"))
    }

    @Test
    fun `fast and quality modes preserve only product constraints`() {
        val fast = CreationContract.route3dMode(
            CreationGenerationMode.FAST,
            CreationContract.MAXIMUM_POLYCOUNT,
            requestedAutoSegment = true,
        )
        assertEquals(CreationContract.FAST_MAXIMUM_POLYCOUNT, fast.polycount)
        assertFalse(fast.autoSegment)
        assertFalse(fast.showAutoSegment)

        val quality = CreationContract.route3dMode(
            CreationGenerationMode.QUALITY,
            CreationContract.MINIMUM_POLYCOUNT,
            requestedAutoSegment = true,
        )
        assertEquals(CreationContract.QUALITY_MINIMUM_POLYCOUNT, quality.polycount)
        assertTrue(quality.autoSegment)
        assertTrue(quality.showAutoSegment)
    }

    @Test
    fun `worker request wire contains no backend selection field`() {
        val request = CreationWorkerRequest(
            jobId = "job-1",
            dispatchId = "dispatch-1",
            requestFingerprint = "a".repeat(64),
            sourceDescriptors = listOf(
                CreationSourceDescriptor("source.png", 123, "b".repeat(64)),
            ),
            tool = CreationTool.IMAGE_TO_3D.wireName,
            generationMode = CreationGenerationMode.FAST.wireName,
            operation = "generate",
            imagePath = "source.png",
            outputPath = "result.glb",
            outputName = "result.glb",
        )
        val codec = Json { encodeDefaults = true; explicitNulls = false }
        val wire = codec.encodeToString(CreationWorkerRequest.serializer(), request)
        val fields = Json.parseToJsonElement(wire).jsonObject.keys

        assertEquals(
            setOf(
                "jobId",
                "acceptedAtMs",
                "deadlineAtMs",
                "dispatchId",
                "requestFingerprint",
                "sourceDescriptors",
                "tool",
                "generationMode",
                "operation",
                "imagePath",
                "imagePaths",
                "outputPath",
                "outputName",
                "polycount",
                "autoSegment",
                "model",
                "backgroundMode",
            ),
            fields,
        )
        assertTrue(request.jobId != request.dispatchId)
        val instructed = codec.encodeToString(
            CreationWorkerRequest.serializer(),
            request.copy(instruction = "Keep the handle rounded"),
        )
        assertTrue("instruction" in Json.parseToJsonElement(instructed).jsonObject)
    }

    @Test
    fun `source descriptors preserve exact reference order and duplicates`() {
        val descriptors = listOf(
            CreationSourceDescriptor("first.png", 10, "a".repeat(64)),
            CreationSourceDescriptor("second.png", 20, "b".repeat(64)),
            CreationSourceDescriptor("first.png", 10, "a".repeat(64)),
        )
        val request = CreationWorkerRequest(
            jobId = "image-job",
            dispatchId = "image-dispatch",
            requestFingerprint = "c".repeat(64),
            sourceDescriptors = descriptors,
            tool = CreationTool.IMAGE_CREATOR.wireName,
            operation = CreationContract.IMAGE_CREATOR_OPERATION,
            imagePath = "first.png",
            imagePaths = descriptors.map(CreationSourceDescriptor::path),
            outputPath = "result.png",
            outputName = "result.png",
        )
        val decoded = Json.decodeFromString(
            CreationWorkerRequest.serializer(),
            Json.encodeToString(CreationWorkerRequest.serializer(), request),
        )

        assertEquals(listOf("first.png", "second.png", "first.png"), decoded.imagePaths)
        assertEquals(descriptors, decoded.sourceDescriptors)
        assertEquals("create_image", decoded.operation)
    }

    @Test
    fun `segmentation continuation follows result capability only`() {
        assertTrue(CreationContract.canContinueSegmentation(false, true))
        assertFalse(CreationContract.canContinueSegmentation(true, true))
        assertFalse(CreationContract.canContinueSegmentation(false, false))
    }

    @Test
    fun `runtime handshake requires version and product capability manifest`() {
        val expected = CreationRuntimeProductDescriptor(
            runtimeVersion = "1",
            features = setOf("image_to_3d", "image_to_svg", "image_creator"),
        )
        val manifest = """
            {
              "contractVersion":1,
              "runtimeVersion":"1",
              "features":["image_to_3d","image_to_svg","image_creator"],
              "tools":{
                "image_to_3d":{
                  "generationModes":{
                    "fast":{"optionalInstruction":true},
                    "quality":{"optionalInstruction":false}
                  }
                }
              }
            }
        """.trimIndent()
        assertTrue(
            isCompatibleCreationRuntimeManifest(manifest, expected),
        )
        assertFalse(isCompatibleCreationRuntimeManifest(manifest.replace("\"1\"", "\"2\""), expected))
        assertFalse(
            isCompatibleCreationRuntimeManifest(
                manifest.replace(",\"image_creator\"", ""),
                expected,
            ),
        )
        assertFalse(
            isCompatibleCreationRuntimeManifest(
                """{"contractVersion":2,"runtimeVersion":"1","features":["image"],"tools":{}}""",
            ),
        )
        assertFalse(isCompatibleCreationRuntimeManifest("""{"contractVersion":1,"tools":{}}"""))
        assertFalse(isCompatibleCreationRuntimeManifest("not-json"))
        assertFalse(
            isCompatibleCreationRuntimeManifest(
                manifest.replace("\"contractVersion\":1", "\"contractVersion\":\"1\""),
                expected,
            ),
        )
        assertFalse(
            isCompatibleCreationRuntimeManifest(
                manifest.replace("\"contractVersion\":1", "\"contractVersion\":1.0"),
                expected,
            ),
        )
        assertFalse(
            isCompatibleCreationRuntimeManifest(
                manifest.replace(
                    "\"image_to_3d\",\"image_to_svg\",\"image_creator\"",
                    "\"image_to_3d\",\"image_to_svg\",\"image_creator\",\"image_creator\"",
                ),
                expected,
            ),
        )
    }

    @Test
    fun `optional instruction capability is exact and fails closed`() {
        val manifest = """
            {
              "contractVersion": 1,
              "runtimeVersion": "1",
              "features": ["image_to_3d","image_to_svg","image_creator"],
              "tools": {
                "image_to_3d": {
                  "generationModes": {
                    "fast": {"optionalInstruction": true},
                    "quality": {"optionalInstruction": false}
                  }
                }
              }
            }
        """.trimIndent()
        assertTrue(runtimeSupportsOptionalInstruction(manifest, "fast"))
        assertFalse(runtimeSupportsOptionalInstruction(manifest, "quality"))
        assertFalse(runtimeSupportsOptionalInstruction(manifest, "unknown"))
        assertFalse(
            runtimeSupportsOptionalInstruction(
                manifest.replace("true", "\"true\""),
                "fast",
            ),
        )
        assertFalse(
            isCompatibleCreationRuntimeManifest(
                manifest.replace(
                    "\"quality\": {\"optionalInstruction\": false}",
                    "\"quality\": {\"optionalInstruction\": false},\"future\":{}",
                ),
            ),
        )
        assertFalse(
            isCompatibleCreationRuntimeManifest(
                manifest.dropLast(1) + ""","unknown":true}""",
            ),
        )
        assertFalse(
            isCompatibleCreationRuntimeManifest(
                manifest.replace(
                    "\"optionalInstruction\": true",
                    "\"optionalInstruction\": true,\"future\":false",
                ),
            ),
        )
        assertFalse(
            isCompatibleCreationRuntimeManifest(
                manifest.replace(
                    "\"image_to_3d\"",
                    "\"unknown_tool\"",
                ),
            ),
        )
    }
}
