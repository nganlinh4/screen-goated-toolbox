package dev.screengoated.toolbox.mobile.service.moonshine

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files

class ZipformerModelIntegrityTest {
    private val contract = ZipformerModelFile(
        name = "model.onnx",
        byteCount = 3,
        sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )

    @Test
    fun exactDigestIsRequiredBeyondMatchingSize() = withDirectory { directory ->
        val model = File(directory, contract.name)
        model.writeText("abc")
        assertTrue(ZipformerModelIntegrity.payloadPresent(directory, listOf(contract)))
        assertTrue(ZipformerModelIntegrity.verified(directory, listOf(contract)))

        model.writeText("abd")
        assertTrue(ZipformerModelIntegrity.payloadPresent(directory, listOf(contract)))
        assertFalse(ZipformerModelIntegrity.verified(directory, listOf(contract)))
    }

    @Test
    fun invalidPartCannotReplaceExistingPayload() = withDirectory { directory ->
        val target = File(directory, contract.name).apply { writeText("old") }
        val part = File(directory, "${contract.name}.part").apply { writeText("abd") }

        runCatching {
            ZipformerModelIntegrity.finalizeVerifiedPart(part, target, contract)
        }.onSuccess { error("invalid part unexpectedly finalized") }

        assertEquals("old", target.readText())
        assertTrue(part.exists())
    }

    @Test
    fun verifiedPartReplacesPayloadAndManagedRemovalPreservesUnknownFiles() =
        withDirectory { directory ->
            val target = File(directory, contract.name).apply { writeText("old") }
            val part = File(directory, "${contract.name}.part").apply { writeText("abc") }
            val unknown = File(directory, "notes.txt").apply { writeText("keep") }

            ZipformerModelIntegrity.finalizeVerifiedPart(part, target, contract)
            assertEquals("abc", target.readText())
            assertFalse(part.exists())

            assertTrue(ZipformerModelIntegrity.removeManagedFiles(directory, listOf(contract)))
            assertFalse(target.exists())
            assertEquals("keep", unknown.readText())
        }

    private fun withDirectory(test: (File) -> Unit) {
        val directory = Files.createTempDirectory("zipformer-integrity-").toFile()
        try {
            test(directory)
        } finally {
            directory.deleteRecursively()
        }
    }
}
