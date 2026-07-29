package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationSurfaceActivityTest {
    @Test
    fun `idle drafts and terminal history do not require status polling`() {
        assertFalse(creationSurfaceHasActiveWork(emptyList()))
        assertFalse(
            creationSurfaceHasActiveWork(
                listOf(
                    item(CreationNativeStage.DRAFT, submitted = false),
                    item(CreationNativeStage.DONE, submitted = true),
                    item(CreationNativeStage.FAILED, submitted = true),
                ),
            ),
        )
    }

    @Test
    fun `accepted and recovered work require status polling`() {
        assertTrue(
            creationSurfaceHasActiveWork(
                listOf(item(CreationNativeStage.QUEUED, submitted = true)),
            ),
        )
        assertTrue(
            creationSurfaceHasActiveWork(
                listOf(item(CreationNativeStage.RUNNING, submitted = true)),
            ),
        )
    }

    private fun item(stage: CreationNativeStage, submitted: Boolean) = CreationNativeItem(
        id = "$stage-$submitted",
        batchId = "batch",
        sourcePath = "source.png",
        sourceName = "source.png",
        stage = stage,
        submitted = submitted,
    )
}
