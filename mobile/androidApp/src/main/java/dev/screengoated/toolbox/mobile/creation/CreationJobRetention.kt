package dev.screengoated.toolbox.mobile.creation

internal data class CreationJobRetentionRecord(
    val id: String,
    val ownerId: String,
    val tool: String,
    val stage: String,
    val startedAtMs: Long,
)

internal fun retainedCreationJobIds(
    records: List<CreationJobRetentionRecord>,
    continuationIds: Set<String>,
    maximumTerminalJobs: Int,
): Set<String> {
    require(maximumTerminalJobs > 0)
    val busyRecords = records.filter { creationStageIsBusy(it.stage) }
    require(busyRecords.size <= maximumTerminalJobs) { "Active creation state exceeds capacity" }
    val busy = busyRecords
        .sortedByDescending(CreationJobRetentionRecord::startedAtMs)
        .mapTo(linkedSetOf(), CreationJobRetentionRecord::id)
    val continued = records.asSequence()
        .filter { it.id in continuationIds && it.id !in busy }
        .sortedByDescending(CreationJobRetentionRecord::startedAtMs)
        .take((maximumTerminalJobs - busy.size).coerceAtLeast(0))
        .mapTo(linkedSetOf(), CreationJobRetentionRecord::id)
    val recentTerminal = records.asSequence()
        .filterNot { creationStageIsBusy(it.stage) }
        .filterNot { it.id in continued }
        .sortedByDescending(CreationJobRetentionRecord::startedAtMs)
        .take((maximumTerminalJobs - busy.size - continued.size).coerceAtLeast(0))
        .map(CreationJobRetentionRecord::id)
    return busy + continued + recentTerminal
}

internal data class CreationMemoryRecord(
    val id: String,
    val status: CreationJobStatus?,
    val request: CreationWorkerRequest?,
    val startedAtMs: Long?,
    val continuation: CreationContinuation?,
    val engineId: String?,
    val ownerId: String?,
    val destination: String?,
)

internal data class CreationMemoryRetirement(
    val original: List<CreationMemoryRecord>,
    val retiredRequests: List<CreationWorkerRequest>,
    val retiredInputPaths: List<String>,
) {
    fun rollback(memory: CreationManagerMemory) = memory.restore(original)
}

internal class CreationManagerMemory {
    val jobs = linkedMapOf<String, CreationJobStatus>()
    val requests = mutableMapOf<String, CreationWorkerRequest>()
    val startedAt = mutableMapOf<String, Long>()
    val continuations = mutableMapOf<String, CreationContinuation>()
    val engineIds = mutableMapOf<String, String>()
    val owners = mutableMapOf<String, String>()
    val destinations = mutableMapOf<String, String?>()

    fun liveArtifactPaths(): Set<String> {
        val active = requests.values.asSequence()
            .filter { request -> jobs[request.jobId]?.stage?.let(::creationStageIsBusy) == true }
            .flatMap { request ->
                (listOf(request.imagePath, request.outputPath) + request.imagePaths).asSequence()
            }
        val continued = continuations.values.asSequence()
            .flatMap { continuation ->
                sequenceOf(continuation.sourcePath, continuation.outputPath)
            }
        return (active + continued).filter(String::isNotBlank).toSet()
    }

    fun pruneTerminal(
        nowMs: Long,
        continuationLifetimeMs: Long,
        maximumTerminalJobs: Int,
    ): CreationMemoryRetirement {
        val expired = continuations.filterValues {
            !creationContinuationIsLive(it.createdAtMs, nowMs, continuationLifetimeMs)
        }.keys
        val records = jobs.mapNotNull { (id, status) ->
            val request = requests[id] ?: return@mapNotNull null
            val owner = owners[id] ?: return@mapNotNull null
            CreationJobRetentionRecord(
                id,
                owner,
                request.tool,
                status.stage,
                startedAt[id] ?: 0L,
            )
        }
        val retained = retainedCreationJobIds(
            records,
            continuations.keys - expired,
            maximumTerminalJobs,
        )
        val removed = jobs.keys - retained
        val original = capture(expired + removed)
        expired.forEach { id ->
            continuations.remove(id)
            jobs[id]?.let { jobs[id] = it.copy(canSegment = false) }
        }
        removed.forEach(::remove)
        val stillOwned = liveArtifactPaths()
        val retiredInputs = (
            original.filter { it.id in removed }.flatMap { it.request?.imagePaths.orEmpty() } +
                original.filter { it.id in expired }.mapNotNull { it.continuation?.sourcePath }
            ).filterNot(stillOwned::contains).distinct()
        return CreationMemoryRetirement(
            original,
            original.filter { it.id in removed }.mapNotNull(CreationMemoryRecord::request),
            retiredInputs,
        )
    }

    fun retireOwner(ownerId: String, tool: CreationTool): CreationMemoryRetirement {
        val ids = owners.filterValues { it == ownerId }.keys.filter { id ->
            requests[id]?.tool == tool.wireName
        }.toSet()
        val original = capture(ids)
        ids.forEach(::remove)
        val stillOwned = liveArtifactPaths()
        return CreationMemoryRetirement(
            original,
            original.mapNotNull(CreationMemoryRecord::request),
            original.flatMap { it.request?.imagePaths.orEmpty() }
                .filterNot(stillOwned::contains)
                .distinct(),
        )
    }

    internal fun restore(records: List<CreationMemoryRecord>) {
        records.forEach { record ->
            remove(record.id)
            record.status?.let { jobs[record.id] = it }
            record.request?.let { requests[record.id] = it }
            record.startedAtMs?.let { startedAt[record.id] = it }
            record.continuation?.let { continuations[record.id] = it }
            record.engineId?.let { engineIds[record.id] = it }
            record.ownerId?.let { owners[record.id] = it }
            if (record.destination != null || record.id in recordsWithDestination(records)) {
                destinations[record.id] = record.destination
            }
        }
    }

    private fun capture(ids: Collection<String>): List<CreationMemoryRecord> = ids.map { id ->
        CreationMemoryRecord(
            id,
            jobs[id],
            requests[id],
            startedAt[id],
            continuations[id],
            engineIds[id],
            owners[id],
            destinations[id],
        )
    }

    private fun remove(id: String) {
        jobs.remove(id)
        requests.remove(id)
        startedAt.remove(id)
        continuations.remove(id)
        engineIds.remove(id)
        owners.remove(id)
        destinations.remove(id)
    }
}

private fun recordsWithDestination(records: List<CreationMemoryRecord>): Set<String> =
    records.mapTo(mutableSetOf(), CreationMemoryRecord::id)
