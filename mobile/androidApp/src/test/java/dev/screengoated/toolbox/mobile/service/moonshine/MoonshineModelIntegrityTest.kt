package dev.screengoated.toolbox.mobile.service.moonshine

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files
import java.security.MessageDigest

class MoonshineModelIntegrityTest {
    private val contract = MoonshineModelFile(
        name = "model.ort",
        byteCount = 3,
        sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )

    @Test
    fun exactDigestIsRequiredBeyondFastSizeCheck() = withDirectory { directory ->
        val model = File(directory, contract.name)
        model.writeText("abc")
        assertTrue(MoonshineModelIntegrity.payloadPresent(directory, listOf(contract)))
        assertTrue(MoonshineModelIntegrity.verified(directory, listOf(contract)))

        model.writeText("abd")
        assertTrue(MoonshineModelIntegrity.payloadPresent(directory, listOf(contract)))
        assertFalse(MoonshineModelIntegrity.verified(directory, listOf(contract)))
    }

    @Test
    fun invalidPartCannotReplaceExistingPayload() = withDirectory { directory ->
        val target = File(directory, contract.name).apply { writeText("old") }
        val part = File(directory, "${contract.name}.part").apply { writeText("abd") }

        runCatching {
            MoonshineModelIntegrity.finalizeVerifiedPart(part, target, contract)
        }.onSuccess { error("invalid part unexpectedly finalized") }

        assertEquals("old", target.readText())
        assertTrue(part.exists())
    }

    @Test
    fun removalDeletesOnlyManagedPayloadAndParts() = withDirectory { directory ->
        val target = File(directory, contract.name).apply { writeText("abc") }
        val part = File(directory, "${contract.name}.part").apply { writeText("partial") }
        val unknown = File(directory, "notes.txt").apply { writeText("keep") }

        assertTrue(MoonshineModelIntegrity.removeManagedFiles(directory, listOf(contract)))
        assertFalse(target.exists())
        assertFalse(part.exists())
        assertEquals("keep", unknown.readText())
    }

    @Test
    fun catalogMatchesAuditedModelPayloads() {
        val expected = mapOf(
            MoonshineLanguage.ENGLISH_TINY to
                (51_131_795L to "7a73aa5f6c6062fa2e14c1452552b456ffb90816fc7bc3e9e514cbcb03293be9"),
            MoonshineLanguage.ENGLISH_SMALL to
                (164_689_974L to "7dc73384d15783b2f9d8c5f5b766fa8660d8d9ccfbf837cefa7ef265acfd8f11"),
            MoonshineLanguage.ENGLISH_MEDIUM to
                (303_329_727L to "78da1c2ce47f63fbaef6539d1b610a5eb68cd35eebc0720c73210e361804b924"),
        )
        MoonshineLanguage.entries.forEach { language ->
            val manifest = language.modelFileContracts.joinToString("\n") {
                "${it.name}:${it.byteCount}:${it.sha256}"
            }
            assertEquals(expected.getValue(language).first, language.expectedSizeBytes)
            assertEquals(expected.getValue(language).second, sha256(manifest))
            assertEquals(7, language.modelFileContracts.size)
            assertTrue(language.downloadBaseUrl.startsWith("https://download.moonshine.ai/model/"))
        }
    }

    private fun sha256(value: String): String = MessageDigest.getInstance("SHA-256")
        .digest(value.toByteArray(Charsets.UTF_8))
        .joinToString("") { byte -> "%02x".format(byte.toInt() and 0xff) }

    private fun withDirectory(test: (File) -> Unit) {
        val directory = Files.createTempDirectory("moonshine-integrity-").toFile()
        try {
            test(directory)
        } finally {
            directory.deleteRecursively()
        }
    }
}
