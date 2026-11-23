package dev.screengoated.toolbox.mobile.creation

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Source contracts for provider behavior a JVM test cannot drive: DocumentsContract
 * and MediaStore exist only on a device.
 */
class CreationDeliverySourceContractTest {
    @Test
    fun safReservationIsCreatedUntypedLikeTheProbe() {
        val store = source("CreationSafDeliveryStore.kt")
        val reserve = store.substringAfter("fun reserve(").substringBefore("\n    fun populate(")
        assertTrue(reserve.contains("CREATION_SAF_PENDING_MIME_TYPE"))
        assertFalse(reserve.contains("intent.mimeType"))
        val probe = source("CreationSafTree.kt").substringAfter("fun validateAtomicDestination(")
        assertTrue(probe.contains("CREATION_SAF_PENDING_MIME_TYPE"))
        assertEquals("application/octet-stream", CREATION_SAF_PENDING_MIME_TYPE)
    }

    @Test
    fun downloadsLookupIncludesThisAppsPendingRows() {
        val find = source("CreationDownloadsStore.kt")
            .substringAfter("fun find(name: String)")
            .substringBefore("\n    fun displayName(")
        assertTrue(find.contains("MediaStore.QUERY_ARG_MATCH_PENDING, MediaStore.MATCH_INCLUDE"))
        assertTrue(find.contains("MediaStore.setIncludePending("))
        assertTrue(find.contains("Build.VERSION_CODES.R"))
    }

    @Test
    fun supersededDeliveryHelpersStayDeleted() {
        val declarations = listOf(
            "fun decideCreationDeliveryRecovery(",
            "class CreationDeliveryRecoveryAction",
            "fun recoveredPublication(",
            "fun CreationFileStore.recoveredPublication(",
            "fun uploadUri(",
            "fun copyCreationFileDurably(",
            "fun uniqueCreationOutputFile(",
            "fun publishManagedCreationResult(",
            "fun String.creationContentUri(",
        )
        val offenders = creationDirectory().walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            .flatMap { file ->
                val text = file.readText()
                declarations.filter(text::contains).map { "${file.name}: $it" }
            }
            .toList()
        assertEquals(emptyList<String>(), offenders)
    }

    @Test
    fun deliveryFilesStayUnderTheLineLimit() {
        listOf(
            "CreationDeliveryTransaction.kt",
            "CreationDeliveryCoordinator.kt",
            "CreationOutputStore.kt",
            "CreationFileStore.kt",
        ).forEach { name ->
            val lines = source(name).lines().size
            assertTrue("$name has $lines lines", lines <= 600)
        }
    }

    private fun source(name: String): String = File(creationDirectory(), name).readText()

    private fun creationDirectory(): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation",
        )
    }
}
