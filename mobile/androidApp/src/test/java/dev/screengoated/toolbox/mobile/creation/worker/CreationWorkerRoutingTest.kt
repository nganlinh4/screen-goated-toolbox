package dev.screengoated.toolbox.mobile.creation.worker

import dev.screengoated.toolbox.mobile.creation.CreationTool
import dev.screengoated.toolbox.mobile.creation.CreationGenerationMode
import dev.screengoated.toolbox.mobile.creation.CreationWorkerRequest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationWorkerRoutingTest {
    @Test
    fun `model generation routes to the prepared mode lane`() {
        assertTrue(
            creationWorkerStructurallySupports(
                CreationTool.IMAGE_TO_3D,
                0,
                request(operation = "generate", generationMode = "fast"),
            ),
        )
        assertTrue(
            creationWorkerStructurallySupports(
                CreationTool.IMAGE_TO_3D,
                1,
                request(operation = "generate", generationMode = "quality"),
            ),
        )
        assertFalse(
            creationWorkerStructurallySupports(
                CreationTool.IMAGE_TO_3D,
                0,
                request(operation = "generate", generationMode = "quality"),
            ),
        )
    }

    @Test
    fun `model follow-up remains on the quality lane`() {
        val refinement = request(operation = "refine", generationMode = "quality")
        assertFalse(
            creationWorkerStructurallySupports(CreationTool.IMAGE_TO_3D, 0, refinement),
        )
        assertTrue(
            creationWorkerStructurallySupports(CreationTool.IMAGE_TO_3D, 1, refinement),
        )
    }

    @Test
    fun `single model demand prepares the lane required by its mode`() {
        assertEquals(
            "3d-0",
            creationRequiredWorkerKey(
                CreationTool.IMAGE_TO_3D,
                request(operation = "generate", generationMode = "fast"),
            ),
        )
        assertEquals(
            "3d-1",
            creationRequiredWorkerKey(
                CreationTool.IMAGE_TO_3D,
                request(operation = "generate", generationMode = "quality"),
            ),
        )
        assertEquals(
            null,
            creationRequiredWorkerKey(
                CreationTool.IMAGE_TO_SVG,
                request(operation = "generate", generationMode = "quality").copy(tool = "svg"),
            ),
        )
        assertEquals(
            "3d-1",
            creationRequiredWorkerKeyForGeneration(
                CreationTool.IMAGE_TO_3D,
                CreationGenerationMode.QUALITY.wireName,
            ),
        )
    }

    private fun request(operation: String, generationMode: String) = CreationWorkerRequest(
        jobId = "job",
        tool = "3d",
        generationMode = generationMode,
        operation = operation,
        imagePath = "source.png",
        outputPath = "result.glb",
        outputName = "result.glb",
    )
}
