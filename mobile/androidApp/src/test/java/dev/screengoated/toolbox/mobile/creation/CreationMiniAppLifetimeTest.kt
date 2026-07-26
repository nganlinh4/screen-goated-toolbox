package dev.screengoated.toolbox.mobile.creation

import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationMiniAppLifetimeTest {
    @Test
    fun `close serializes after an accepted submission and blocks later submissions`() {
        val lifetime = CreationMiniAppLifetime()
        val submissionEntered = CountDownLatch(1)
        val releaseSubmission = CountDownLatch(1)
        val closeFinished = CountDownLatch(1)
        val events = CopyOnWriteArrayList<String>()

        val submitter = thread {
            lifetime.computeIfOpen {
                submissionEntered.countDown()
                assertTrue(releaseSubmission.await(2, TimeUnit.SECONDS))
                events += "submitted"
            }
        }
        assertTrue(submissionEntered.await(2, TimeUnit.SECONDS))
        val closer = thread {
            lifetime.close { events += "cancelled" }
            closeFinished.countDown()
        }

        releaseSubmission.countDown()
        submitter.join(2_000)
        closer.join(2_000)

        assertTrue(closeFinished.await(0, TimeUnit.SECONDS))
        assertEquals(listOf("submitted", "cancelled"), events)
        assertTrue(lifetime.isClosed)
        assertNull(lifetime.computeIfOpen { "late submission" })
        assertFalse(lifetime.close { events += "duplicate cancellation" })
    }

    @Test
    fun `closing native state cancels only queued and running items`() {
        fun item(id: String, stage: CreationNativeStage) = CreationNativeItem(
            id = id,
            batchId = "batch",
            sourcePath = "$id.png",
            sourceName = "$id.png",
            submitted = stage != CreationNativeStage.DRAFT,
            stage = stage,
        )
        val closed = CreationNativeUiState(
            items = listOf(
                item("draft", CreationNativeStage.DRAFT),
                item("queued", CreationNativeStage.QUEUED),
                item("running", CreationNativeStage.RUNNING),
                item("done", CreationNativeStage.DONE),
            ),
        ).cancelActiveItems()

        assertEquals(CreationNativeStage.DRAFT, closed.items[0].stage)
        assertEquals(CreationNativeStage.CANCELLED, closed.items[1].stage)
        assertEquals(CreationNativeStage.CANCELLED, closed.items[2].stage)
        assertEquals(CreationNativeStage.DONE, closed.items[3].stage)
    }
}
