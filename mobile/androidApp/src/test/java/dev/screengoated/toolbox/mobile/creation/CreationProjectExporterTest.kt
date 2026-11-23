package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.security.MessageDigest
import kotlin.io.path.createTempDirectory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class CreationProjectExporterTest {
    @Test
    fun `export accepts the committed artifact and rejects replacement even with identical bytes`() {
        val directory = createTempDirectory("creation-export-proof").toFile()
        try {
            val original = File(directory, "model.glb").apply { writeText("model bytes") }
            val replacement = File(directory, "replacement.glb").apply { writeText("model bytes") }
            // Windows JVM filesystems may not expose fileKey; Android's file store
            // supplies this token from its filesystem identity provider.
            fun identity(path: String): String? = when (path) {
                original.path -> "inode:10"
                replacement.path -> "inode:11"
                else -> null
            }
            val expectedIdentity = "inode:10"
            val digest = MessageDigest.getInstance("SHA-256")
                .digest(original.readBytes()).joinToString("") { "%02x".format(it) }
            assertEquals(original, creationProvenManagedExport(original.path, original.length(), digest, expectedIdentity, ::identity))
            assertThrows(IllegalArgumentException::class.java) {
                creationProvenManagedExport(replacement.path, replacement.length(), digest, expectedIdentity, ::identity)
            }
            original.writeText("other bytes")
            assertThrows(IllegalArgumentException::class.java) {
                creationProvenManagedExport(original.path, original.length(), digest, expectedIdentity, ::identity)
            }
        } finally {
            directory.deleteRecursively()
        }
    }
}
