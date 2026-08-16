package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive

class CreationImageSessionsTest {
    @Test
    fun `thumbnail sampling rounds upward to stay within requested edge`() {
        val sample = creationThumbnailSampleSize(3_199, 1_800, 1_600)

        assertEquals(2, sample)
        assertTrue((3_199 + sample - 1) / sample <= 1_600)
        assertTrue((1_800 + sample - 1) / sample <= 1_600)
        assertTrue(
            ((3_199L + sample - 1) / sample) *
                ((1_800L + sample - 1) / sample) <= 1_600L * 1_600L,
        )
    }

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
        assertEquals(
            listOf("first.png", "second.png", "first.png"),
            state.selectedItem?.referencePaths,
        )
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

    @Test
    fun `wire image cardinality preserves plural order and uses legacy only as fallback`() {
        assertEquals(
            listOf("first.png", "second.png", "first.png"),
            normalizeCreationImagePaths(
                CreationTool.IMAGE_CREATOR,
                listOf(" first.png ", "second.png", "first.png"),
                "FIRST.PNG",
            ),
        )
        assertEquals(
            listOf("first.png"),
            normalizeCreationImagePaths(
                CreationTool.IMAGE_TO_3D,
                listOf("first.png"),
                "second.png",
            ),
        )
        assertTrue(
            runCatching {
                normalizeCreationImagePaths(CreationTool.IMAGE_TO_SVG, emptyList(), null)
            }.isFailure,
        )
        assertEquals(
            listOf("first.png"),
            normalizeCreationImagePaths(
                CreationTool.IMAGE_TO_SVG,
                emptyList(),
                "FIRST.PNG",
            ).map(String::lowercase),
        )
    }

    @Test
    fun `source-less image submission omits the reference field`() {
        val withoutReferences = creationSubmissionArgs(
            CreationTool.IMAGE_CREATOR,
            CreationImageSessions.new().copy(prompt = "Draw a quiet forest"),
        )
        val withReferences = creationSubmissionArgs(
            CreationTool.IMAGE_CREATOR,
            CreationImageSessions.new(listOf("first.png", "second.png"))
                .copy(prompt = "Edit these"),
        )

        assertFalse("imagePaths" in withoutReferences)
        assertEquals(
            listOf("first.png", "second.png"),
            withReferences.getValue("imagePaths").jsonArray.map { it.jsonPrimitive.content },
        )
    }

}
