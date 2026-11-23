package dev.screengoated.toolbox.mobile.creation

import java.io.FileNotFoundException

/**
 * Persisted publication attempts after which an unpublished delivery stops retrying.
 * With the manager's 250 ms doubling backoff capped at 30 s this is several minutes
 * of transient failure, and the count survives restarts.
 */
internal const val CREATION_DELIVERY_MAXIMUM_PUBLISH_ATTEMPTS = 12

internal enum class CreationDeliveryRetryDecision {
    RETRY,
    ABANDON,
}

/**
 * Decides what a failed delivery attempt does next. Once any file is published the
 * delivery keeps retrying, because a published result is retained for the user and
 * must still reach history. Before that, a permanent destination error abandons the
 * delivery at once and a transient one abandons it after the attempt budget.
 */
internal fun creationDeliveryRetryDecision(
    record: CreationDeliveryRecord,
    attempts: Int,
    error: Throwable,
): CreationDeliveryRetryDecision = when {
    record.historyCommitted ||
        record.publishedPath != null ||
        record.companion?.publishedPath != null -> CreationDeliveryRetryDecision.RETRY
    creationDeliveryErrorIsPermanent(error) -> CreationDeliveryRetryDecision.ABANDON
    attempts >= CREATION_DELIVERY_MAXIMUM_PUBLISH_ATTEMPTS -> CreationDeliveryRetryDecision.ABANDON
    else -> CreationDeliveryRetryDecision.RETRY
}

/** A revoked folder grant or a missing destination or source never heals by retrying. */
internal fun creationDeliveryErrorIsPermanent(error: Throwable): Boolean =
    generateSequence(error) { it.cause }
        .take(8)
        .any { it is SecurityException || it is FileNotFoundException }

/** Aborts one unpublished reservation; true when nothing of it remains. */
internal fun CreationFileStore.abortDeliveryReservation(
    intent: CreationPublishIntent,
    pendingHandle: String?,
    pendingIdentity: String?,
    publicationPrepared: Boolean,
    artifactSize: Long,
    artifactSha256: String,
): Boolean {
    val pending = pendingHandle ?: return true
    val identity = requireNotNull(pendingIdentity)
    return if (publicationPrepared) {
        abortPreparedPublishIntent(intent, pending, identity, artifactSize, artifactSha256)
    } else {
        abortPublishIntent(intent, pending, identity)
    }
}

/**
 * Releases everything an abandoned, unpublished delivery holds: its reservations,
 * its sealed staging outputs and its job inputs. Each step is best effort, because
 * the destination that failed may also refuse cleanup; a leftover hidden pending
 * document is harmless and the record is dropped either way.
 */
internal fun CreationFileStore.releaseAbandonedDelivery(record: CreationDeliveryRecord) {
    runCatching {
        abortDeliveryReservation(
            record.intent,
            record.pendingHandle,
            record.pendingIdentity,
            record.publicationPrepared,
            record.artifactSize,
            record.artifactSha256,
        )
    }
    record.companion?.let { companion ->
        runCatching {
            abortDeliveryReservation(
                companion.intent,
                companion.pendingHandle,
                companion.pendingIdentity,
                companion.publicationPrepared,
                companion.artifactSize,
                companion.artifactSha256,
            )
        }
    }
    CreationTool.fromWireName(record.request.tool)?.let { tool ->
        runCatching { deleteReservedStagingFile(tool, record.sealedPath) }
        record.companion?.let { runCatching { deleteReservedStagingFile(tool, it.sealedPath) } }
    }
    runCatching { releaseJobInputs(creationJobInputPathsReleasedAfterCommit(record.request)) }
}
