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

    @Test
    fun `surface cancellation cannot select another owner or tool`() {
        val memory = CreationManagerMemory()
        fun add(id: String, owner: String, tool: CreationTool) {
            memory.jobs[id] = CreationJobStatus(
                jobId = id,
                stage = "generating",
                progressText = "working",
            )
            memory.requests[id] = CreationWorkerRequest(
                jobId = id,
                tool = tool.wireName,
                operation = "generate",
                imagePath = "source.png",
                outputPath = "$id.out",
                outputName = "$id.out",
            )
            memory.owners[id] = owner
        }
        add("mine", "surface-a", CreationTool.IMAGE_TO_SVG)
        add("other-owner", "surface-b", CreationTool.IMAGE_TO_SVG)
        add("other-tool", "surface-a", CreationTool.IMAGE_TO_3D)

        assertEquals(
            listOf("mine"),
            creationCancellationJobIds(
                memory,
                "surface-a",
                CreationTool.IMAGE_TO_SVG,
                requestedJobId = null,
            ),
        )
        assertTrue(
            creationCancellationJobIds(
                memory,
                "surface-a",
                CreationTool.IMAGE_TO_SVG,
                requestedJobId = "other-owner",
            ).isEmpty(),
        )
    }

    @Test
    fun `worker leases retain concurrent surfaces and release the last owner`() {
        val leases = CreationWorkerLeaseRegistry()

        assertTrue(
            leases.acquire(
                CreationTool.IMAGE_TO_SVG,
                "surface-a",
                CreationWorkerLeaseKind.SURFACE,
            ),
        )
        assertFalse(
            leases.acquire(
                CreationTool.IMAGE_TO_SVG,
                "surface-b",
                CreationWorkerLeaseKind.SURFACE,
            ),
        )
        assertEquals(2, leases.requestedCapacity(CreationTool.IMAGE_TO_SVG, 2, 2))
        assertFalse(leases.release(CreationTool.IMAGE_TO_SVG, "surface-a"))
        assertTrue(leases.retained(CreationTool.IMAGE_TO_SVG))
        assertTrue(leases.release(CreationTool.IMAGE_TO_SVG, "surface-b"))
        assertFalse(leases.retained(CreationTool.IMAGE_TO_SVG))
        assertEquals(0, leases.requestedCapacity(CreationTool.IMAGE_TO_SVG, 2, 2))
    }

    @Test
    fun `recovery lease preserves both 3D mode lanes after the surface closes`() {
        val leases = CreationWorkerLeaseRegistry()
        leases.acquire(
            CreationTool.IMAGE_TO_3D,
            "surface-owner",
            CreationWorkerLeaseKind.SURFACE,
        )
        leases.acquire(
            CreationTool.IMAGE_TO_3D,
            "recovery-job",
            CreationWorkerLeaseKind.JOB,
        )

        assertFalse(leases.release(CreationTool.IMAGE_TO_3D, "surface-owner"))
        assertTrue(leases.retained(CreationTool.IMAGE_TO_3D))
        assertEquals(2, leases.requestedCapacity(CreationTool.IMAGE_TO_3D, 2, 2))
        assertTrue(leases.release(CreationTool.IMAGE_TO_3D, "recovery-job"))
        assertEquals(0, leases.requestedCapacity(CreationTool.IMAGE_TO_3D, 2, 2))
    }

    @Test
    fun `image demand expands only for two accepted jobs`() {
        val leases = CreationWorkerLeaseRegistry()
        leases.acquire(
            CreationTool.IMAGE_CREATOR,
            "surface-a",
            CreationWorkerLeaseKind.SURFACE,
        )
        assertEquals(1, leases.requestedCapacity(CreationTool.IMAGE_CREATOR, 1, 2))
        leases.acquire(
            CreationTool.IMAGE_CREATOR,
            "job-a",
            CreationWorkerLeaseKind.JOB,
        )
        assertEquals(1, leases.requestedCapacity(CreationTool.IMAGE_CREATOR, 1, 2))
        leases.acquire(
            CreationTool.IMAGE_CREATOR,
            "job-b",
            CreationWorkerLeaseKind.JOB,
        )
        assertEquals(2, leases.requestedCapacity(CreationTool.IMAGE_CREATOR, 1, 2))
        leases.release(CreationTool.IMAGE_CREATOR, "job-b")
        assertEquals(1, leases.requestedCapacity(CreationTool.IMAGE_CREATOR, 1, 2))
        leases.release(CreationTool.IMAGE_CREATOR, "surface-a")
        assertEquals(1, leases.requestedCapacity(CreationTool.IMAGE_CREATOR, 1, 2))
        leases.release(CreationTool.IMAGE_CREATOR, "job-a")
        assertEquals(0, leases.requestedCapacity(CreationTool.IMAGE_CREATOR, 1, 2))
    }

    @Test
    fun `accepted job owns preparation until its terminal release`() {
        val acquired = mutableListOf<Triple<CreationTool, String, String?>>()
        val required = mutableListOf<Triple<CreationTool, String, String>>()
        val released = mutableListOf<Pair<CreationTool, String>>()
        val leases = CreationRecoveryWorkerLeases(
            acquireWorker = { tool, owner, worker -> acquired += Triple(tool, owner, worker) },
            requireWorker = { tool, owner, worker ->
                required += Triple(tool, owner, worker)
            },
            releaseWorker = { tool, owner -> released += tool to owner },
        )

        leases.acquire("image-job", CreationTool.IMAGE_CREATOR)
        leases.acquire("image-job", CreationTool.IMAGE_CREATOR)
        assertEquals(
            listOf(Triple(CreationTool.IMAGE_CREATOR, "recovery:image-job", null)),
            acquired,
        )
        leases.assign("image-job", CreationTool.IMAGE_CREATOR, "image-1")
        leases.assign("image-job", CreationTool.IMAGE_CREATOR, "image-1")
        assertEquals(
            listOf(Triple(CreationTool.IMAGE_CREATOR, "recovery:image-job", "image-1")),
            required,
        )

        leases.release("image-job", CreationTool.IMAGE_CREATOR)
        leases.release("image-job", CreationTool.IMAGE_CREATOR)
        assertEquals(
            listOf(CreationTool.IMAGE_CREATOR to "recovery:image-job"),
            released,
        )
    }

    @Test
    fun `creation preparation starts only after a first use lease`() {
        assertNull(
            selectCreationPreparationTool(
                active = null,
                retained = emptySet(),
                ready = emptySet(),
                surfacePriority = listOf(CreationTool.IMAGE_CREATOR),
            ),
        )

        val selected = selectCreationPreparationTool(
            active = null,
            retained = setOf(CreationTool.IMAGE_TO_3D),
            ready = emptySet(),
            surfacePriority = listOf(CreationTool.IMAGE_TO_3D),
        )

        assertEquals(CreationTool.IMAGE_TO_3D, selected)
        assertEquals(2, CreationContract.maximumParallelJobs(requireNotNull(selected)))
        assertEquals(
            CreationTool.IMAGE_CREATOR,
            selectCreationPreparationTool(
                active = CreationTool.IMAGE_TO_3D,
                retained = CreationTool.entries.toSet(),
                ready = emptySet(),
                surfacePriority = listOf(CreationTool.IMAGE_CREATOR),
            ),
        )
    }
}
