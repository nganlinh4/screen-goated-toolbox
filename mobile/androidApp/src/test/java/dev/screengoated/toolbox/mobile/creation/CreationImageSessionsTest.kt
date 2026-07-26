package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationImageSessionsTest {
    @Test
    fun `one image session accepts zero one or multiple references`() {
        val empty = CreationImageSessions.new()
        assertTrue(empty.referencePaths.isEmpty())
        assertTrue(empty.sourcePath.isEmpty())

        val state = CreationImageSessions.addReferences(
            CreationNativeUiState(items = listOf(empty), selectedItemId = empty.id),
            listOf("first.png", "second.png", "first.png"),
        )
        assertEquals(1, state.items.size)
        assertEquals(listOf("first.png", "second.png"), state.selectedItem?.referencePaths)
        assertEquals("first.png", state.selectedItem?.sourcePath)
    }

    @Test
    fun `reference collection is bounded and removal preserves order`() {
        val empty = CreationImageSessions.new()
        val paths = (0..CreationContract.IMAGE_CREATOR_MAXIMUM_REFERENCE_IMAGES)
            .map { "reference-$it.png" }
        val state = CreationImageSessions.addReferences(
            CreationNativeUiState(items = listOf(empty), selectedItemId = empty.id),
            paths,
        )
        val item = requireNotNull(state.selectedItem)
        assertEquals(
            CreationContract.IMAGE_CREATOR_MAXIMUM_REFERENCE_IMAGES,
            item.referencePaths.size,
        )
        assertTrue(state.transientError?.contains("20") == true)

        val removed = CreationImageSessions.removeReference(item, 0)
        assertEquals("reference-1.png", removed.sourcePath)
        assertEquals(item.referencePaths.drop(1), removed.referencePaths)
    }
}
