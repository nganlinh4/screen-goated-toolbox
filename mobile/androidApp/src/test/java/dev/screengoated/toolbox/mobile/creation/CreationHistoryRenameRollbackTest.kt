package dev.screengoated.toolbox.mobile.creation

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationHistoryRenameRollbackTest {
    @Test
    fun localFilesReturnToTheirOwnNameAndDocumentsToTheirDisplayName() {
        val local = File("creation/library/model-abc.glb").absolutePath
        assertEquals("model-abc.glb", creationRenameRollbackName(local, "My model.glb"))
        assertEquals(
            "My model.glb",
            creationRenameRollbackName("content://media/external/downloads/7", "My model.glb"),
        )
    }

    @Test
    fun restoreMustLandOnTheOriginalPath() {
        val old = File("creation/library/model.glb").absolutePath
        assertTrue(creationRenameRestoredTo(File("creation/library/./model.glb").path, old))
        assertFalse(creationRenameRestoredTo(File("creation/library/other.glb").absolutePath, old))
        val uri = "content://media/external/downloads/7"
        assertTrue(creationRenameRestoredTo(uri, uri))
        assertFalse(creationRenameRestoredTo("content://media/external/downloads/8", uri))
    }

    @Test
    fun aFailedIndexCommitRollsTheMoveBackBeforeRethrowing() {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        val store = File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation/" +
                "CreationHistoryRenameStore.kt",
        ).readText()
        val rename = store.substringAfter("    fun rename(").substringBefore("\n    fun recover(")
        val guarded = rename.substringAfter("commit(updated)").substringBefore("receipt = receipt.copy(committed = true)")
        assertTrue(guarded.contains("rollBack(records, receipt)"))
        assertTrue(guarded.contains("throw failure"))
        val rollBack = store.substringAfter("private fun rollBack(").substringBefore("\n    private fun restoreRenamed(")
        assertTrue(rollBack.indexOf("receipt.companion") < rollBack.indexOf("receipt.newPath, receipt.newIdentity"))
    }
}
