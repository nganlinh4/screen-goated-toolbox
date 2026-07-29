package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationDispatchQueueTest {
    @Test
    fun `more than two explicit jobs remain in one bounded fair queue`() {
        val queue = CreationDispatchQueue(maximumQueuedPerTool = 50)

        repeat(10) { index ->
            assertTrue(
                queue.offer(
                    CreationPendingDispatch("job-$index", CreationTool.IMAGE_CREATOR),
                ),
            )
        }

        assertEquals(10, queue.count(CreationTool.IMAGE_CREATOR))
        assertEquals((0 until 10).map { "job-$it" }, queue.snapshot().map { it.jobId })
    }

    @Test
    fun `queue capacity is enforced per product tool`() {
        val queue = CreationDispatchQueue(maximumQueuedPerTool = 2)
        assertTrue(queue.offer(CreationPendingDispatch("image-1", CreationTool.IMAGE_CREATOR)))
        assertTrue(queue.offer(CreationPendingDispatch("image-2", CreationTool.IMAGE_CREATOR)))
        assertFalse(queue.offer(CreationPendingDispatch("image-3", CreationTool.IMAGE_CREATOR)))
        assertTrue(queue.offer(CreationPendingDispatch("svg-1", CreationTool.IMAGE_TO_SVG)))
    }

    @Test
    fun `two executing jobs leave fifty additional sessions durably queueable`() {
        val executing = List(CreationContract.MAXIMUM_PARALLEL_JOBS) { "active-$it" }
        val queue = CreationDispatchQueue(maximumQueuedPerTool = 50)
        repeat(50) { index ->
            assertTrue(
                queue.offer(CreationPendingDispatch("queued-$index", CreationTool.IMAGE_TO_3D)),
            )
        }

        assertEquals(2, executing.size)
        assertEquals(50, queue.count(CreationTool.IMAGE_TO_3D))
        assertFalse(
            queue.offer(CreationPendingDispatch("queued-overflow", CreationTool.IMAGE_TO_3D)),
        )
    }

    @Test
    fun `progress floods coalesce while terminal delivery remains lossless`() {
        val buffer = CreationWorkerEventBuffer()
        repeat(100_000) { index ->
            buffer.offer(
                CreationWorkerEnvelope(
                    "engine",
                    CreationWorkerEvent(
                        jobId = "job",
                        event = "progress",
                        progressRatio = index / 100_000.0,
                    ),
                ),
            )
        }
        assertEquals(1, buffer.size())

        buffer.offer(
            CreationWorkerEnvelope(
                "engine",
                CreationWorkerEvent(jobId = "job", event = "success"),
            ),
        )
        buffer.offer(
            CreationWorkerEnvelope(
                "engine",
                CreationWorkerEvent(jobId = "other-job", event = "failure"),
            ),
        )
        assertEquals(2, buffer.size())
        assertEquals(
            setOf("success", "failure"),
            setOf(
                requireNotNull(buffer.poll()).event.event,
                requireNotNull(buffer.poll()).event.event,
            ),
        )
        assertEquals(0, buffer.size())
    }

    @Test
    fun `terminal rerun receives fresh job and dispatch identities`() {
        val firstJob = newCreationJobId(CreationTool.IMAGE_CREATOR, 1_000, 1)
        val firstDispatch = newCreationDispatchId(CreationTool.IMAGE_CREATOR, 1_000, 2)
        val rerunJob = newCreationJobId(CreationTool.IMAGE_CREATOR, 1_000, 3)
        val rerunDispatch = newCreationDispatchId(CreationTool.IMAGE_CREATOR, 1_000, 4)

        assertTrue(firstJob != rerunJob)
        assertTrue(firstDispatch != rerunDispatch)
        assertTrue(firstJob != firstDispatch)
        assertTrue(rerunJob != rerunDispatch)
    }

    @Test
    fun `failed durable segmentation commit restores continuation and queue`() {
        val memory = CreationManagerMemory()
        val queue = CreationDispatchQueue(maximumQueuedPerTool = 50)
        val continuation = CreationContinuation(
            ownerId = "owner",
            engineId = "3d-0",
            token = "token",
            sourcePath = "source.png",
            outputPath = "result.glb",
            outputName = "result.glb",
            createdAtMs = 1L,
        )
        memory.continuations["original"] = continuation
        memory.jobs["original"] = CreationJobStatus(
            jobId = "original",
            stage = "done",
            progressText = "done",
            canSegment = true,
        )
        val request = request("segmentation", "segment")
        val draft = CreationJobDraft(
            request,
            CreationJobStatus(
                jobId = request.jobId,
                stage = "segmenting",
                progressText = "working",
            ),
        )

        val rollback = applyCreationSegmentationSubmission(
            memory,
            queue,
            CreationSegmentationSnapshot("original", continuation),
            draft,
            "owner",
            null,
            2L,
            50,
        )
        rollback()

        assertEquals(continuation, memory.continuations["original"])
        assertTrue(memory.jobs.getValue("original").canSegment)
        assertNull(memory.jobs[request.jobId])
        assertEquals(0, queue.count(CreationTool.IMAGE_TO_3D))
    }

    @Test
    fun `failed durable worker claim restores engine continuation`() {
        val memory = CreationManagerMemory()
        val continuation = CreationContinuation(
            ownerId = "owner",
            engineId = "image-0",
            token = "token",
            sourcePath = "source.png",
            outputPath = "result.png",
            outputName = "result.png",
            createdAtMs = 1L,
        )
        memory.continuations["previous"] = continuation
        memory.jobs["previous"] = CreationJobStatus(
            jobId = "previous",
            stage = "done",
            progressText = "done",
            canSegment = true,
        )
        val request = request("new-job", "generate")

        val change = applyCreationWorkerAssignment(memory, request, "image-0")
        assertEquals(listOf("source.png"), change.retiredInputPaths)
        change.rollback()

        assertNull(memory.engineIds[request.jobId])
        assertEquals(continuation, memory.continuations["previous"])
        assertTrue(memory.jobs.getValue("previous").canSegment)
    }

    @Test
    fun `failed durable submission removes queue and every companion map`() {
        val memory = CreationManagerMemory()
        val queue = CreationDispatchQueue(maximumQueuedPerTool = 50)
        val request = request("new-job", "generate")
        val draft = CreationJobDraft(
            request,
            CreationJobFactory.initialStatus(CreationTool.IMAGE_TO_3D, request),
        )

        val rollback = requireNotNull(
            applyCreationJobSubmission(
                memory,
                queue,
                draft,
                ownerId = "owner",
                destination = null,
                startedAtMs = 1L,
                maximumQueuedJobs = 50,
            ),
        )
        rollback()

        assertEquals(0, queue.count(CreationTool.IMAGE_TO_3D))
        assertTrue(memory.jobs.isEmpty())
        assertTrue(memory.requests.isEmpty())
        assertTrue(memory.startedAt.isEmpty())
        assertTrue(memory.owners.isEmpty())
        assertTrue(memory.destinations.isEmpty())
    }

    @Test
    fun `malicious progress data cannot mutate result metadata or expose private stage`() {
        val memory = CreationManagerMemory()
        memory.requests["job"] = request("job", "generate")
        val original = CreationJobStatus(
            jobId = "job",
            generationMode = CreationGenerationMode.QUALITY.wireName,
            stage = "generating",
            progressText = "Creating result",
            phase = "generating",
            progressRatio = 0.25,
            estimatedTotalMs = 10_000,
            timingSampleCount = 2,
            outputPath = "published.glb",
            outputName = "published.glb",
            mimeType = "model/gltf-binary",
            faces = 12,
            vertices = 8,
        )
        memory.jobs["job"] = original

        val update = requireNotNull(
            applyCreationProgressUpdate(
                memory,
                "job",
                CreationWorkerEvent(
                    jobId = "job",
                    generationMode = CreationGenerationMode.FAST.wireName,
                    event = "progress",
                    stage = "private-capacity-check",
                    progressRatio = Double.NaN,
                    estimatedTotalMs = Long.MAX_VALUE,
                    timingSampleCount = Long.MAX_VALUE,
                    outputPath = "private/staging/result.glb",
                    outputName = "secret.glb",
                    mimeType = "private/type",
                    width = 99,
                    height = 88,
                    isSegmented = true,
                    canSegment = true,
                    faces = 999,
                    vertices = 888,
                ),
            ),
        )

        assertNull(update.diagnosticStage)
        assertEquals(original, memory.jobs["job"])
    }

    @Test
    fun `public timing is capped at the exact whole-job watchdog`() {
        val status = CreationJobStatus(
            stage = "generating",
            progressText = "working",
            progressRatio = Double.POSITIVE_INFINITY,
            estimatedTotalMs = Long.MAX_VALUE,
            timingSampleCount = Long.MAX_VALUE,
        ).withCreationElapsed(
            startedAtMs = 1L,
            nowMs = Long.MAX_VALUE,
        )

        assertEquals(CreationContract.MAXIMUM_JOB_RUNTIME_MS, status.elapsedMs)
        assertEquals(CreationContract.MAXIMUM_JOB_RUNTIME_MS, status.estimatedTotalMs)
        assertEquals(null, status.progressRatio)
        assertEquals(100_000L, status.timingSampleCount)
    }

    @Test
    fun `product segmentation state cannot be weakened by a private event`() {
        val fast = request("fast", "generate").copy(
            generationMode = CreationGenerationMode.FAST.wireName,
        )
        val automatic = request("automatic", "generate").copy(
            generationMode = CreationGenerationMode.QUALITY.wireName,
            autoSegment = true,
        )
        val continuation = request("continuation", "segment").copy(
            generationMode = CreationGenerationMode.QUALITY.wireName,
        )

        assertTrue(validatedCreationSegmentation(fast, event(isSegmented = null)))
        assertTrue(validatedCreationSegmentation(automatic, event(isSegmented = true)))
        assertTrue(validatedCreationSegmentation(continuation, event(isSegmented = null)))
        assertThrows(IllegalArgumentException::class.java) {
            validatedCreationSegmentation(fast, event(isSegmented = false))
        }
        assertThrows(IllegalArgumentException::class.java) {
            validatedCreationSegmentation(automatic, event(isSegmented = false))
        }
    }

    @Test
    fun `quality result without automatic separation follows validated event state`() {
        val request = request("quality", "generate").copy(
            generationMode = CreationGenerationMode.QUALITY.wireName,
            autoSegment = false,
        )

        assertFalse(validatedCreationSegmentation(request, event(isSegmented = null)))
        assertFalse(validatedCreationSegmentation(request, event(isSegmented = false)))
        assertTrue(validatedCreationSegmentation(request, event(isSegmented = true)))
    }

    private fun event(isSegmented: Boolean?) = CreationWorkerEvent(
        event = "success",
        isSegmented = isSegmented,
    )

    private fun request(jobId: String, operation: String) = CreationWorkerRequest(
        jobId = jobId,
        dispatchId = "dispatch-$jobId",
        requestFingerprint = "0".repeat(64),
        tool = CreationTool.IMAGE_TO_3D.wireName,
        operation = operation,
        imagePath = "source.png",
        outputPath = "$jobId.glb",
        outputName = "$jobId.glb",
    )
}
