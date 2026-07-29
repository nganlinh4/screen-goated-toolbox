package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlin.io.path.createTempDirectory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class CreationUriGrantContractTest {
    @Test
    fun `uri grants survive shared owners and restart then release at the last owner`() {
        val filesDir = createTempDirectory("creation-uri-grants").toFile()
        val history = File(filesDir, "creation/history.json").apply {
            parentFile?.mkdirs()
            writeText(
                """[{"sourcePath":"creation/presentation/source.jpg",""" +
                    """"outputPath":"content://storage/tree/old/document/old%3Aresult"}]""",
            )
        }
        val journal = File(filesDir, "creation/state/accepted-jobs.json").apply {
            parentFile?.mkdirs()
            writeText("""[{"status":{"sourceImagePaths":["content://images/from-job"]}}]""")
        }
        val live = "content://images/shared"
        val currentOutput = "content://storage/tree/current"
        val requiredAfterRestart = creationRequiredPersistedUriGrants(
            filesDir,
            setOf(live),
            currentOutput,
        )

        assertEquals(
            setOf(live, "content://images/from-job"),
            requiredAfterRestart.source,
        )
        assertEquals(
            setOf(
                "content://storage/tree/current",
                "content://storage/tree/old/document/old%3Aresult",
            ),
            requiredAfterRestart.output,
        )

        history.delete()
        journal.delete()
        val owned = listOf(
            CreationOwnedUriGrant(live, setOf(CreationUriGrantLedger.SOURCE_KIND), 1),
            CreationOwnedUriGrant(
                "content://images/from-job",
                setOf(CreationUriGrantLedger.SOURCE_KIND),
                1,
            ),
            CreationOwnedUriGrant(
                "content://storage/tree/old",
                setOf(CreationUriGrantLedger.OUTPUT_KIND),
                3,
            ),
            CreationOwnedUriGrant(
                currentOutput,
                setOf(CreationUriGrantLedger.OUTPUT_KIND),
                3,
            ),
        )
        assertEquals(
            emptySet<String>(),
            creationRequiredGrantRoles(
                owned[0].uri,
                creationRequiredPersistedUriGrants(filesDir, emptySet(), currentOutput),
            ),
        )
        assertEquals(
            setOf(CreationUriGrantLedger.OUTPUT_KIND),
            creationRequiredGrantRoles(
                currentOutput,
                creationRequiredPersistedUriGrants(filesDir, emptySet(), currentOutput),
            ),
        )
        assertFalse(owned.any { it.uri == "content://foreign/not-owned" })
        filesDir.deleteRecursively()
    }
}
