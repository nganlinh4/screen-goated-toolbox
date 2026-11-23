package dev.screengoated.toolbox.mobile.creation

import java.io.ByteArrayInputStream
import java.io.File
import kotlin.io.path.createTempDirectory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationViewerModelValidationTest {
    @Test
    fun freshMaterializationParsesTheArtifactOnce() {
        val directory = createTempDirectory("creation-viewer-once").toFile()
        try {
            var validations = 0
            val target = CreationPreviewCache().materialize(
                directory,
                "a".repeat(32),
                "glb",
                64,
                reusable = false,
                openInput = { ByteArrayInputStream("glTF-model-bytes".encodeToByteArray()) },
                validate = { validations += 1 },
            )
            assertEquals(1, validations)
            assertEquals("glTF-model-bytes", target.readText())
            assertFalse(directory.listFiles().orEmpty().any { ".tmp-" in it.name })
        } finally {
            directory.deleteRecursively()
        }
    }

    @Test
    fun originServesOnlyTheValidatedLength() {
        assertTrue(creationViewerModelLengthServable(1_024, 1_024))
        assertFalse(creationViewerModelLengthServable(1_025, 1_024))
        assertFalse(creationViewerModelLengthServable(19, 19))
        assertFalse(
            creationViewerModelLengthServable(
                CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES + 1,
                CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES + 1,
            ),
        )
    }

    @Test
    fun viewerValidatesItsPrivateCopyNotTheSourceOrEachRequest() {
        val viewer = source("CreationViewerModelFiles.kt")
        assertTrue(viewer.contains("materializePreview(path, \"glb\", validateLocal = false)"))
        assertTrue(viewer.contains("validate = CreationArtifactValidator::validateGlb"))
        val origin = source("CreationModelViewerOrigin.kt")
            .substringAfter("private fun modelResponse()")
            .substringBefore("\n    }\n")
        assertFalse(origin.contains("validateGlb"))
        assertTrue(origin.contains("session.modelBytes"))
    }

    private fun source(name: String): String {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation/$name",
        ).readText()
    }
}
