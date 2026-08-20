package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

@Serializable
internal data class CreationPublishIntent(
    val kind: String,
    val destination: String? = null,
    val finalName: String,
    val mimeType: String,
    val targetPath: String? = null,
    val pendingPath: String? = null,
    val pendingName: String? = null,
    val reservationToken: String? = null,
)

@Serializable
internal data class CreationCompanionDelivery(
    val sealedPath: String,
    val outputName: String,
    val artifactSize: Long,
    val artifactSha256: String,
    val intent: CreationPublishIntent,
    val pendingHandle: String? = null,
    val pendingIdentity: String? = null,
    val publicationPrepared: Boolean = false,
    val publishedPath: String? = null,
)

@Serializable
internal data class CreationDeliveryRecord(
    val dispatchId: String,
    val engineId: String,
    val ownerId: String,
    val request: CreationWorkerRequest,
    val current: CreationJobStatus,
    val event: CreationWorkerEvent,
    val sealedPath: String,
    val mimeType: String,
    val segmented: Boolean,
    val canSegment: Boolean,
    val imageWidth: Int? = null,
    val imageHeight: Int? = null,
    val faces: Long? = null,
    val vertices: Long? = null,
    val artifactSize: Long,
    val artifactSha256: String,
    val intent: CreationPublishIntent,
    val companion: CreationCompanionDelivery? = null,
    val transactionStage: String = "validated",
    val pendingHandle: String? = null,
    val pendingIdentity: String? = null,
    val publicationPrepared: Boolean = false,
    val publishedPath: String? = null,
    val historyCommitted: Boolean = false,
    val cleanupAttempts: Int = 0,
)

internal data class CreationReconciledTerminal(
    val request: CreationWorkerRequest,
    val status: CreationJobStatus,
    val continuation: CreationContinuation?,
)

internal class CreationDeliveryStore(
    context: Context,
    private val files: CreationFileStore,
    private val cancellations: CreationCancellationStore,
) {
    private val target = File(context.filesDir, "creation/state/deliveries.json")
    private val lock = Any()
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }

    fun begin(prepared: PreparedCreation, destination: String?): CreationDeliveryRecord =
        cancellations.ifActive(prepared.request) { synchronized(lock) {
            require(creationRequestHasValidDeliveryIdentity(prepared.request)) {
                "Creation delivery identity is invalid"
            }
            val records = read().toMutableList()
            val size = files.size(prepared.stagingPath)
            require(size > 0L) { "Creation result is unavailable" }
            val digest = files.sha256(prepared.stagingPath)
            val companionArtifact = prepared.companionStagingPath?.let { path ->
                Triple(
                    path,
                    files.size(path).also { require(it > 0L) },
                    files.sha256(path),
                )
            }
            records.firstOrNull { it.dispatchId == prepared.request.dispatchId }?.let { saved ->
                require(
                    creationDeliveryMatchesPrepared(
                        saved,
                        prepared,
                        size,
                        digest,
                        companionArtifact,
                    ),
                ) {
                    "Creation delivery identity conflicts with saved state"
                }
                return@synchronized saved
            }
            require(records.size < CREATION_DELIVERY_MAXIMUM_RECORDS) {
                "Creation delivery state is full"
            }
            val intent = files.planPublishIntent(
                prepared.request.dispatchId,
                prepared.request.outputName,
                prepared.mimeType,
                destination,
                records.map(CreationDeliveryRecord::intent),
            )
            val companion = prepared.companionStagingPath?.let { path ->
                val name = requireNotNull(prepared.companionName)
                val companionIntent = files.planPublishIntent(
                    prepared.request.dispatchId,
                    name,
                    "application/octet-stream",
                    destination,
                    records.map(CreationDeliveryRecord::intent) + intent,
                )
                CreationCompanionDelivery(
                    sealedPath = path,
                    outputName = name,
                    artifactSize = requireNotNull(companionArtifact).second,
                    artifactSha256 = requireNotNull(companionArtifact).third,
                    intent = companionIntent,
                )
            }
            val record = CreationDeliveryRecord(
                dispatchId = prepared.request.dispatchId,
                engineId = prepared.engineId,
                ownerId = prepared.ownerId,
                request = prepared.request,
                current = prepared.current,
                event = prepared.event,
                sealedPath = prepared.stagingPath,
                mimeType = prepared.mimeType,
                segmented = prepared.segmented,
                canSegment = prepared.canSegment,
                imageWidth = prepared.imageDimensions?.width,
                imageHeight = prepared.imageDimensions?.height,
                faces = prepared.faces,
                vertices = prepared.vertices,
                artifactSize = size,
                artifactSha256 = digest,
                intent = intent,
                companion = companion,
            )
            records += record
            write(records)
            record
        } }

    fun publish(
        record: CreationDeliveryRecord,
        publicationAlreadyWon: Boolean = false,
    ): CreationDeliveryRecord = synchronized(lock) {
        val records = read().toMutableList()
        var current = requireNotNull(records.firstOrNull { it.dispatchId == record.dispatchId }) {
            "Creation delivery state is unavailable"
        }
        require(creationDeliveryIdentityMatches(current, record)) {
            "Creation delivery identity conflicts with saved state"
        }
        current.publishedPath?.let { published ->
            require(
                files.publishedArtifactMatches(
                    published,
                    requireNotNull(current.pendingIdentity),
                    current.artifactSize,
                    current.artifactSha256,
                ),
            ) { "Published creation result changed" }
            current.companion?.publishedPath?.let { companionPublished ->
                require(
                    files.publishedArtifactMatches(
                        companionPublished,
                        requireNotNull(current.companion.pendingIdentity),
                        current.companion.artifactSize,
                        current.companion.artifactSha256,
                    ),
                ) { "Published creation companion changed" }
            }
            if (current.companion == null || current.companion.publishedPath != null) {
                return@synchronized current
            }
        }
        val action = {
            if (current.pendingHandle == null) {
                val reservation = files.reservePublishIntent(current.intent)
                current = current.copy(
                    pendingHandle = reservation.handle,
                    pendingIdentity = reservation.identity,
                )
                replace(records, current)
                write(records)
            }
            val unreservedCompanion = current.companion?.takeIf { it.pendingHandle == null }
            if (unreservedCompanion != null) {
                val companion = unreservedCompanion
                val reservation = files.reservePublishIntent(companion.intent)
                current = current.copy(
                    companion = companion.copy(
                        pendingHandle = reservation.handle,
                        pendingIdentity = reservation.identity,
                    ),
                )
                replace(records, current)
                write(records)
            }
            val pending = requireNotNull(current.pendingHandle)
            val identity = requireNotNull(current.pendingIdentity)
            if (!current.publicationPrepared) {
                files.populatePublishIntent(
                    current.intent,
                    pending,
                    identity,
                    File(current.sealedPath),
                    current.artifactSize,
                    current.artifactSha256,
                )
                current = current.copy(
                    publicationPrepared = true,
                    transactionStage = "publication_prepared",
                )
                replace(records, current)
                write(records)
            }
            current.companion?.takeIf { !it.publicationPrepared }?.let { companion ->
                files.populatePublishIntent(
                    companion.intent,
                    requireNotNull(companion.pendingHandle),
                    requireNotNull(companion.pendingIdentity),
                    File(companion.sealedPath),
                    companion.artifactSize,
                    companion.artifactSha256,
                )
                current = current.copy(companion = companion.copy(publicationPrepared = true))
                replace(records, current)
                write(records)
            }
            if (current.publishedPath == null) {
                val published = files.commitPublishIntent(
                    current.intent,
                    pending,
                    identity,
                    current.artifactSize,
                    current.artifactSha256,
                )
                current = current.copy(publishedPath = published)
                replace(records, current)
                write(records)
            }
            current.companion?.takeIf { it.publishedPath == null }?.let { companion ->
                val published = files.commitPublishIntent(
                    companion.intent,
                    requireNotNull(companion.pendingHandle),
                    requireNotNull(companion.pendingIdentity),
                    companion.artifactSize,
                    companion.artifactSha256,
                )
                current = current.copy(companion = companion.copy(publishedPath = published))
                replace(records, current)
                write(records)
            }
            current = current.copy(transactionStage = "published")
            replace(records, current)
            write(records)
            current
        }
        if (publicationAlreadyWon) action() else cancellations.ifActive(current.request, action)
    }

    fun markHistoryCommitted(dispatchId: String) {
        synchronized(lock) {
            val records = read().toMutableList()
            val record = records.firstOrNull { it.dispatchId == dispatchId }
                ?: return@synchronized
            if (record.historyCommitted) return@synchronized
            replace(
                records,
                record.copy(
                    historyCommitted = true,
                    transactionStage = "history_committed",
                ),
            )
            write(records)
        }
    }

    fun complete(dispatchId: String): Boolean = synchronized(lock) {
        val records = read().toMutableList()
        val record = records.firstOrNull { it.dispatchId == dispatchId }
            ?: return@synchronized true
        require(record.historyCommitted) { "Creation history is not committed" }
        val primaryRemoved = !files.exists(record.sealedPath) ||
            files.deleteManagedPath(record.sealedPath)
        val companionRemoved = record.companion?.sealedPath?.let { path ->
            !files.exists(path) || files.deleteManagedPath(path)
        } ?: true
        val removed = primaryRemoved && companionRemoved
        if (removed) {
            records.removeAll { it.dispatchId == dispatchId }
        } else {
            replace(
                records,
                record.copy(cleanupAttempts = (record.cleanupAttempts + 1).coerceAtMost(
                    CREATION_DELIVERY_MAXIMUM_CLEANUP_ATTEMPTS,
                )),
            )
        }
        write(records)
        removed
    }

    fun reconcile(
        finisher: CreationJobFinisher,
        protectedPaths: Set<String>,
    ): Map<String, CreationReconciledTerminal> {
        return synchronized(lock) {
            val records = read().toMutableList()
            val completedJobs = mutableMapOf<String, CreationReconciledTerminal>()
            records.toList().forEach { saved ->
                val reconciled = runCatching {
                    val finished = if (cancellations.isCancelled(saved.request)) {
                        discardCancelled(saved)
                        CreationReconciledTerminal(
                            saved.request,
                            saved.current.copy(
                                stage = "cancelled",
                                progressText = "Cancelled.",
                                phase = "cancelled",
                            ),
                            null,
                        )
                    } else if (!saved.historyCommitted) {
                        val published = publish(saved)
                        val completed = finisher.completePublished(
                            published.prepared(),
                            requireNotNull(published.publishedPath),
                            published.intent.finalName,
                            published.companion?.publishedPath,
                            published.companion?.outputName,
                        )
                        finisher.recordHistory(completed, published.event, protectedPaths)
                        markHistoryCommitted(published.dispatchId)
                        CreationReconciledTerminal(
                            completed.request,
                            completed.status,
                            completed.continuation,
                        )
                    } else {
                        val completed = finisher.completePublished(
                            saved.prepared(),
                            requireNotNull(saved.publishedPath),
                            saved.intent.finalName,
                            saved.companion?.publishedPath,
                            saved.companion?.outputName,
                        )
                        CreationReconciledTerminal(
                            completed.request,
                            completed.status,
                            completed.continuation,
                        )
                    }
                    finished
                }.getOrNull()
                if (reconciled != null) completedJobs[saved.request.jobId] = reconciled
            }
            completedJobs
        }
    }

    fun finalizeJobs(jobIds: Set<String>) {
        val records = synchronized(lock) { read() }
        records.filter { it.request.jobId in jobIds }.forEach { record ->
            files.releaseJobInputs(
                creationJobInputPathsReleasedAfterCommit(
                    record.request,
                    retainedByContinuation = record.canSegment &&
                        !record.event.continuationToken.isNullOrBlank(),
                ),
            )
            complete(record.dispatchId)
        }
    }

    fun pendingJobIds(): Set<String> = synchronized(lock) {
        read().mapTo(mutableSetOf()) { it.request.jobId }
    }

    fun containsDispatch(dispatchId: String): Boolean = synchronized(lock) {
        read().any { it.dispatchId == dispatchId }
    }

    private fun discardCancelled(record: CreationDeliveryRecord) {
        val cleaned = record.pendingHandle?.let { pending ->
            val identity = requireNotNull(record.pendingIdentity)
            if (record.publicationPrepared) {
                files.abortPreparedPublishIntent(
                    record.intent,
                    pending,
                    identity,
                    record.artifactSize,
                    record.artifactSha256,
                )
            } else {
                files.abortPublishIntent(record.intent, pending, identity)
            }
        } ?: true
        require(cleaned) { "Creation cancellation cleanup is pending" }
        record.publishedPath?.let { published ->
            files.managedPathIdentity(published)?.let { managed ->
                files.pendingCleanupStore().isolateAndEnqueue(
                    listOf(
                        CreationCleanupCandidate(
                            path = managed,
                            expectedSize = record.artifactSize,
                            expectedSha256 = record.artifactSha256,
                            expectedIdentity = record.pendingIdentity,
                        ),
                    ),
                )
            }
        }
        record.companion?.let { companion ->
            val cleanedCompanion = companion.pendingHandle?.let { pending ->
                if (companion.publicationPrepared) {
                    files.abortPreparedPublishIntent(
                        companion.intent,
                        pending,
                        requireNotNull(companion.pendingIdentity),
                        companion.artifactSize,
                        companion.artifactSha256,
                    )
                } else {
                    files.abortPublishIntent(
                        companion.intent,
                        pending,
                        requireNotNull(companion.pendingIdentity),
                    )
                }
            } ?: true
            require(cleanedCompanion) { "Creation companion cancellation cleanup is pending" }
            companion.publishedPath?.let { published ->
                files.managedPathIdentity(published)?.let { managed ->
                    files.pendingCleanupStore().isolateAndEnqueue(
                        listOf(
                            CreationCleanupCandidate(
                                path = managed,
                                expectedSize = companion.artifactSize,
                                expectedSha256 = companion.artifactSha256,
                                expectedIdentity = companion.pendingIdentity,
                            ),
                        ),
                    )
                }
            }
        }
        markHistoryCommitted(record.dispatchId)
    }

    private fun CreationDeliveryRecord.prepared() = PreparedCreation(
        engineId,
        ownerId,
        request,
        current,
        event,
        sealedPath,
        mimeType,
        imageDimensions = if (imageWidth != null && imageHeight != null) {
            CreationImageDimensions(imageWidth, imageHeight)
        } else {
            null
        },
        segmented,
        canSegment,
        faces,
        vertices,
        companion?.sealedPath,
        companion?.outputName,
        event.polygons,
        event.quads,
    )

    private fun read(): List<CreationDeliveryRecord> {
        if (!target.exists()) return emptyList()
        val text = requireNotNull(
            readCreationIndexTextBounded(target, CREATION_DELIVERY_INDEX_MAX_BYTES),
        ) { "Creation delivery state is unavailable" }
        val decoded = json.decodeFromString<List<CreationDeliveryRecord>>(text)
        require(
            decoded.size <= CREATION_DELIVERY_MAXIMUM_RECORDS &&
                decoded.map(CreationDeliveryRecord::dispatchId).distinct().size == decoded.size &&
                decoded.all { validCreationDeliveryRecord(files.context.filesDir, it) },
        ) { "Creation delivery state exceeds capacity" }
        return decoded
    }

    private fun write(records: List<CreationDeliveryRecord>) {
        val retained = retainCreationDeliveryRecords(records, files::exists)
        require(retained.size <= CREATION_DELIVERY_MAXIMUM_RECORDS) {
            "Creation delivery state exceeds capacity"
        }
        writeCreationIndexTextAtomically(
            target,
            json.encodeToString(retained),
            CREATION_DELIVERY_INDEX_MAX_BYTES,
        )
    }

    private fun replace(
        records: MutableList<CreationDeliveryRecord>,
        updated: CreationDeliveryRecord,
    ) {
        records.removeAll { it.dispatchId == updated.dispatchId }
        records += updated
    }
}

internal enum class CreationDeliveryFailureAction {
    RETRY,
    FAIL_JOB,
}

internal fun creationDeliveryFailureAction(hasDurableReceipt: Boolean) =
    if (hasDurableReceipt) CreationDeliveryFailureAction.RETRY
    else CreationDeliveryFailureAction.FAIL_JOB

internal class CreationDeliveryCoordinator(
    private val deliveries: CreationDeliveryStore,
    private val finisher: CreationJobFinisher,
    private val journal: CreationJobJournal,
    private val journalWriter: CreationManagerJournalWriter,
    private val memory: CreationManagerMemory,
    private val dispatchQueue: CreationDispatchQueue,
    private val mutationLock: Any,
    private val stateLock: Any,
) {
    fun deliver(prepared: PreparedCreation, destination: String?): FinishedCreation {
        val receipt = deliveries.publish(deliveries.begin(prepared, destination))
        return finisher.completePublished(
            prepared,
            requireNotNull(receipt.publishedPath),
            receipt.intent.finalName,
            receipt.companion?.publishedPath,
            receipt.companion?.outputName,
        )
    }

    fun reconcileAtStartup(): Map<String, CreationTool> {
        synchronized(mutationLock) {
            val completed = deliveries.reconcile(finisher, liveArtifactPaths())
            if (completed.isEmpty()) return emptyMap()
            synchronized(stateLock) {
                completed.forEach(::completeJobLocked)
            }
            journal.save(snapshotCreationManagerState(memory))
            deliveries.finalizeJobs(completed.keys)
            return completed.mapValues { (_, terminal) ->
                requireNotNull(CreationTool.fromWireName(terminal.request.tool))
            }
        }
    }

    fun reconcileInProcess(): Map<String, CreationTool> {
        synchronized(mutationLock) {
            val completed = deliveries.reconcile(finisher, liveArtifactPaths())
            if (completed.isEmpty()) return emptyMap()
            val snapshot = synchronized(stateLock) {
                completed.forEach(::completeJobLocked)
                journalWriter.snapshot(memory)
            }
            journalWriter.writeRequired(snapshot)
            deliveries.finalizeJobs(completed.keys)
            return completed.mapValues { (_, terminal) ->
                requireNotNull(CreationTool.fromWireName(terminal.request.tool))
            }
        }
    }

    private fun liveArtifactPaths(): Set<String> =
        synchronized(stateLock) { memory.liveArtifactPaths() }

    private fun completeJobLocked(jobId: String, terminal: CreationReconciledTerminal) {
        dispatchQueue.remove(jobId)
        memory.jobs[jobId] = terminal.status
        terminal.continuation?.let { memory.continuations[jobId] = it }
            ?: memory.continuations.remove(jobId)
        memory.engineIds.remove(jobId)
    }
}

internal fun creationDeliveryMatchesPrepared(
    saved: CreationDeliveryRecord,
    prepared: PreparedCreation,
    artifactSize: Long,
    artifactSha256: String,
    companionArtifact: Triple<String, Long, String>? = null,
): Boolean =
    saved.dispatchId == prepared.request.dispatchId &&
        saved.engineId == prepared.engineId &&
        saved.ownerId == prepared.ownerId &&
        saved.request == prepared.request &&
        saved.sealedPath == prepared.stagingPath &&
        saved.mimeType == prepared.mimeType &&
        saved.artifactSize == artifactSize &&
        saved.artifactSha256.equals(artifactSha256, ignoreCase = true) &&
        if (companionArtifact == null) {
            saved.companion == null
        } else {
            saved.companion?.sealedPath == companionArtifact.first &&
                saved.companion.outputName == prepared.companionName &&
                saved.companion.artifactSize == companionArtifact.second &&
                saved.companion.artifactSha256.equals(
                    companionArtifact.third,
                    ignoreCase = true,
                )
        }

internal fun creationDeliveryIdentityMatches(
    saved: CreationDeliveryRecord,
    candidate: CreationDeliveryRecord,
): Boolean =
    saved.dispatchId == candidate.dispatchId &&
        saved.engineId == candidate.engineId &&
        saved.ownerId == candidate.ownerId &&
        saved.request == candidate.request &&
        saved.sealedPath == candidate.sealedPath &&
        saved.mimeType == candidate.mimeType &&
        saved.artifactSize == candidate.artifactSize &&
        saved.artifactSha256.equals(candidate.artifactSha256, ignoreCase = true) &&
        saved.intent == candidate.intent &&
        saved.companion == candidate.companion

internal enum class CreationDeliveryRecoveryAction {
    PUBLISH_SEALED,
    COMMIT_VERIFIED,
    WAIT_FOR_OWNED_BYTES,
}

internal fun decideCreationDeliveryRecovery(
    sealedMatchesReceipt: Boolean,
    publishedExists: Boolean,
    publishedMatchesReceipt: Boolean,
): CreationDeliveryRecoveryAction = when {
    publishedExists && publishedMatchesReceipt -> CreationDeliveryRecoveryAction.COMMIT_VERIFIED
    publishedExists -> CreationDeliveryRecoveryAction.WAIT_FOR_OWNED_BYTES
    sealedMatchesReceipt -> CreationDeliveryRecoveryAction.PUBLISH_SEALED
    else -> CreationDeliveryRecoveryAction.WAIT_FOR_OWNED_BYTES
}
