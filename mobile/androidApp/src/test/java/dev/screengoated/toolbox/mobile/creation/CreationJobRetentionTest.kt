package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationJobRetentionTest {
    @Test
    fun `terminal retention keeps busy continuation and newest owner status`() {
        val records = (0 until 100).map { index ->
            CreationJobRetentionRecord(
                id = "terminal-$index",
                ownerId = "owner",
                tool = "svg",
                stage = "done",
                startedAtMs = index.toLong(),
            )
        } + listOf(
            CreationJobRetentionRecord("busy", "owner", "svg", "generating", 1),
            CreationJobRetentionRecord("continued", "owner", "3d", "done", 0),
        )

        val kept = retainedCreationJobIds(records, setOf("continued"), maximumTerminalJobs = 10)

        assertTrue("busy" in kept)
        assertTrue("continued" in kept)
        assertTrue("terminal-99" in kept)
        assertTrue("terminal-0" !in kept)
    }

    @Test
    fun `memory pruning removes every companion map atomically`() {
        val memory = CreationManagerMemory()
        repeat(5) { index ->
            val id = "job-$index"
            memory.jobs[id] = CreationJobStatus(jobId = id, stage = "done", progressText = "done")
            memory.requests[id] = CreationWorkerRequest(
                jobId = id,
                tool = "svg",
                operation = "generate",
                imagePath = "$id.png",
                outputPath = "$id.svg",
                outputName = "$id.svg",
            )
            memory.startedAt[id] = index.toLong()
            memory.owners[id] = "owner"
            memory.engineIds[id] = "engine"
            memory.destinations[id] = null
        }

        memory.pruneTerminal(nowMs = 10, continuationLifetimeMs = 10, maximumTerminalJobs = 1)

        val retained = memory.jobs.keys
        assertEquals(setOf("job-4"), retained)
        assertEquals(retained, memory.requests.keys)
        assertEquals(retained, memory.startedAt.keys)
        assertEquals(retained, memory.owners.keys)
        assertEquals(retained, memory.engineIds.keys)
        assertEquals(retained, memory.destinations.keys)
    }

    @Test
    fun `retired input ledger waits for journal ownership and failed deletion`() {
        val deleted = mutableListOf<String>()
        val first = retainedCreationJobInputCleanupPaths(
            listOf("protected", "retry", "gone"),
            protectedDirectories = setOf("protected"),
            exists = { it != "gone" },
            delete = {
                deleted += it
                false
            },
        )

        assertEquals(listOf("protected", "retry"), first)
        assertEquals(listOf("retry"), deleted)
        assertEquals(
            listOf("protected"),
            retainedCreationJobInputCleanupPaths(
                first,
                protectedDirectories = setOf("protected"),
                exists = { true },
                delete = { true },
            ),
        )
    }

    @Test
    fun `many closed owners leave a bounded journal and exact retired inputs`() {
        val memory = CreationManagerMemory()
        repeat(500) { index ->
            val id = "job-$index"
            memory.jobs[id] = CreationJobStatus(
                jobId = id,
                stage = "done",
                progressText = "done",
            )
            memory.requests[id] = CreationWorkerRequest(
                jobId = id,
                tool = "svg",
                operation = "generate",
                imagePath = "creation/job-inputs/$id/0.img",
                imagePaths = listOf("creation/job-inputs/$id/0.img"),
                outputPath = "creation/staging/svg/$id.svg",
                outputName = "$id.svg",
            )
            memory.startedAt[id] = index.toLong()
            memory.owners[id] = "owner-$index"
            memory.destinations[id] = null
        }

        val retired = memory.pruneTerminal(500, 10, 192)

        assertEquals(192, snapshotCreationManagerState(memory).size)
        assertEquals(308, retired.retiredInputPaths.size)
    }

    @Test
    fun `continuation expiry clears capability and retires its snapshot`() {
        val memory = CreationManagerMemory()
        val id = "continued"
        memory.jobs[id] = CreationJobStatus(
            jobId = id,
            stage = "done",
            progressText = "done",
            canSegment = true,
        )
        memory.requests[id] = CreationWorkerRequest(
            jobId = id,
            tool = "3d",
            operation = "generate",
            imagePath = "creation/job-inputs/$id/0.img",
            imagePaths = listOf("creation/job-inputs/$id/0.img"),
            outputPath = "creation/staging/3d/$id.glb",
            outputName = "$id.glb",
        )
        memory.startedAt[id] = 0
        memory.owners[id] = "owner"
        memory.destinations[id] = null
        memory.continuations[id] = CreationContinuation(
            ownerId = "owner",
            engineId = "engine",
            token = "token",
            sourcePath = "creation/job-inputs/$id/0.img",
            outputPath = "creation/library/$id.glb",
            outputName = "$id.glb",
            createdAtMs = 0,
        )

        val retired = memory.pruneTerminal(11, 10, 192)

        assertTrue(memory.continuations.isEmpty())
        assertEquals(false, memory.jobs.getValue(id).canSegment)
        assertEquals(listOf("creation/job-inputs/$id/0.img"), retired.retiredInputPaths)
    }

    @Test
    fun `suspended polling retains fifty terminal jobs for every creation tool`() {
        val records = CreationTool.entries.flatMap { tool ->
            (0 until 50).map { index ->
                CreationJobRetentionRecord(
                    id = "${tool.wireName}-$index",
                    ownerId = "owner-${tool.wireName}",
                    tool = tool.wireName,
                    stage = "done",
                    startedAtMs = index.toLong(),
                )
            }
        }

        val kept = retainedCreationJobIds(records, emptySet(), maximumTerminalJobs = 192)

        assertEquals(150, kept.size)
        assertEquals(records.map(CreationJobRetentionRecord::id).toSet(), kept)
    }

    @Test
    fun `journal selection never evicts an old active job for newer terminals`() {
        val active = journalRecord("active", "generating", 1L)
        val terminals = (0 until 500).map { index ->
            journalRecord("terminal-$index", "done", 2L + index)
        }

        val selected = selectCreationJournalRecords(listOf(active) + terminals, maximumRecords = 384)

        assertTrue(selected.any { it.request.jobId == "active" })
        assertEquals(384, selected.size)
    }

    @Test
    fun `restore bounds tampered active population without crashing`() {
        val records = (0 until 80).map { index ->
            journalRecord("active-$index", "generating", index.toLong())
        }

        val restored = boundedRestorableCreationRecords(
            records,
            maximumActivePerTool = 52,
            activeIsValid = { true },
        )

        assertEquals(52, restored.size)
        assertEquals(52, restored.map { it.request.jobId }.distinct().size)
    }

    private fun journalRecord(id: String, stage: String, startedAtMs: Long) =
        CreationJournalRecord(
            ownerId = "owner",
            request = CreationWorkerRequest(
                jobId = id,
                dispatchId = "dispatch-$id",
                tool = "svg",
                operation = "generate",
                imagePath = "$id.png",
                outputPath = "$id.svg",
                outputName = "$id.svg",
            ),
            status = CreationJobStatus(jobId = id, stage = stage, progressText = stage),
            startedAtMs = startedAtMs,
        )
}
