package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

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
            require(creationStagingDigestUnchanged(prepared.validatedSha256, digest)) {
                "Creation output changed after validation"
            }
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
            reconcileCreationDeliveriesIndependently(
                records.toList(),
                onFailure = ::recordFailedAttempt,
            ) { saved ->
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
            }
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
        val cleaned = files.abortDeliveryReservation(
            record.intent,
            record.pendingHandle,
            record.pendingIdentity,
            record.publicationPrepared,
            record.artifactSize,
            record.artifactSha256,
        )
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
            val cleanedCompanion = files.abortDeliveryReservation(
                companion.intent,
                companion.pendingHandle,
                companion.pendingIdentity,
                companion.publicationPrepared,
                companion.artifactSize,
                companion.artifactSha256,
            )
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

    /**
     * Counts one failed reconcile attempt durably. When the retry policy gives up on
     * an unpublished delivery, its reservations, staging outputs and inputs are
     * released, the record is dropped and the job settles as failed.
     */
    private fun recordFailedAttempt(
        saved: CreationDeliveryRecord,
        error: Throwable,
    ): CreationReconciledTerminal? {
        if (cancellations.isCancelled(saved.request)) return null
        val records = read().toMutableList()
        val current = records.firstOrNull { it.dispatchId == saved.dispatchId } ?: return null
        val attempts = (current.publishAttempts + 1)
            .coerceAtMost(CREATION_DELIVERY_MAXIMUM_PUBLISH_ATTEMPTS)
        if (creationDeliveryRetryDecision(current, attempts, error) ==
            CreationDeliveryRetryDecision.RETRY
        ) {
            replace(records, current.copy(publishAttempts = attempts))
            write(records)
            return null
        }
        files.releaseAbandonedDelivery(current)
        records.removeAll { it.dispatchId == current.dispatchId }
        write(records)
        return CreationReconciledTerminal(current.request, creationFailedStatus(current.current), null)
    }

    private fun CreationDeliveryRecord.prepared() = PreparedCreation(
        engineId,
        ownerId,
        request,
        current,
        event,
        sealedPath,
        mimeType,
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
