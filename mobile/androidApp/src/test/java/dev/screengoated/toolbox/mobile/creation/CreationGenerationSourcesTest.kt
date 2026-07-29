package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationGenerationSourcesTest {
    @Test
    fun `only leased current-session originals can become generation input`() {
        val filesDir = Files.createTempDirectory("creation-generation-sources").toFile()
        try {
            val source = File(filesDir, "creation/sources/original.png").apply {
                requireNotNull(parentFile).mkdirs()
                writeText("original")
            }
            val presentation = File(filesDir, "creation/presentation/preview.png").apply {
                requireNotNull(parentFile).mkdirs()
                writeText("preview")
            }
            val jobInput = File(filesDir, "creation/job-inputs/old/0.img").apply {
                requireNotNull(parentFile).mkdirs()
                writeText("snapshot")
            }
            val leased = setOf(
                source.absolutePath,
                presentation.absolutePath,
                jobInput.absolutePath,
            )
            val exists = { path: String -> File(path).isFile }

            assertTrue(
                creationGenerationSourcesAreUsable(
                    filesDir,
                    leased,
                    listOf(source.absolutePath),
                    exists,
                ),
            )
            assertFalse(
                creationGenerationSourcesAreUsable(
                    filesDir,
                    leased,
                    listOf(presentation.absolutePath),
                    exists,
                ),
            )
            assertFalse(
                creationGenerationSourcesAreUsable(
                    filesDir,
                    leased,
                    listOf(jobInput.absolutePath),
                    exists,
                ),
            )
            assertFalse(
                creationGenerationSourcesAreUsable(
                    filesDir,
                    emptySet(),
                    listOf(source.absolutePath),
                    exists,
                ),
            )
        } finally {
            filesDir.deleteRecursively()
        }
    }

    @Test
    fun `persisted source handle must belong to the submitting surface`() {
        val handle = "content://images/original"

        assertTrue(
            creationGenerationSourcesAreUsable(
                File("."),
                setOf(handle),
                listOf(handle),
            ) { true },
        )
        assertFalse(
            creationGenerationSourcesAreUsable(
                File("."),
                emptySet(),
                listOf(handle),
            ) { true },
        )
        assertFalse(
            creationGenerationSourcesAreUsable(
                File("."),
                setOf(handle),
                listOf(handle),
            ) { false },
        )
    }

    @Test
    fun `text-only image retry never promotes its presentation preview to a reference`() {
        val args = creationSubmissionArgs(
            CreationTool.IMAGE_CREATOR,
            CreationNativeItem(
                id = "text-only",
                batchId = "batch",
                sourcePath = "creation/presentation/history-preview.png",
                sourceName = "history-preview.png",
                referencePaths = emptyList(),
                prompt = "A calm ocean",
            ),
        )

        assertFalse("imagePath" in args)
        assertFalse("imagePaths" in args)
        assertTrue(creationGenerationSourcesAreUsable(File("."), emptySet(), emptyList()) { false })
    }
}
