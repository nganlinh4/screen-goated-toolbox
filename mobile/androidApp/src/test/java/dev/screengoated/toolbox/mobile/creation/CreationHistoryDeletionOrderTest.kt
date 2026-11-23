package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.io.IOException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationHistoryDeletionOrderTest {
    @Test
    fun clearAllKeepsGoingPastAFailureAndReportsIt() {
        val ioFailure = IOException("disk error")
        val stateFailure = IllegalStateException("Could not delete result")
        val batch = deleteCreationHistoryBatch(listOf("a", "b", "c", "d")) { id ->
            when (id) {
                "b" -> throw ioFailure
                "d" -> throw stateFailure
                else -> Unit
            }
        }
        assertEquals(listOf("a", "c"), batch.deleted)
        val thrown = assertThrows(IOException::class.java) { batch.throwFirstFailure() }
        assertSame(ioFailure, thrown)
        assertEquals(listOf<Throwable>(stateFailure), thrown.suppressed.toList())
    }

    @Test
    fun cleanBatchDoesNotThrow() {
        val batch = deleteCreationHistoryBatch(listOf(1, 2, 3)) {}
        assertEquals(listOf(1, 2, 3), batch.deleted)
        batch.throwFirstFailure()
    }

    @Test
    fun historyIsDeletedBeforeTheJournalDetachesOutputs() {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        val coordinator = File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation/" +
                "CreationManagerHistoryCoordinator.kt",
        ).readText()
        val delete = coordinator.substringAfter("fun delete(tool: CreationTool, id: String)")
            .substringBefore("\n    fun deleteAll(")
        assertTrue(delete.indexOf("history.delete(") < delete.indexOf("detachDeletedOutputs("))
        val deleteAll = coordinator.substringAfter("fun deleteAll(tool: CreationTool)")
            .substringBefore("\n    private fun detachDeletedOutputs(")
        assertEquals(1, Regex("""history\.list\(""").findAll(deleteAll).count())
        assertTrue(deleteAll.indexOf("history.delete(") < deleteAll.indexOf("detachDeletedOutputs("))
        assertTrue(!deleteAll.contains("delete(tool,"))
    }
}
