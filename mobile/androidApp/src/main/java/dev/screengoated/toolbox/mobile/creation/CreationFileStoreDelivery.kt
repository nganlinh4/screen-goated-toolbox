package dev.screengoated.toolbox.mobile.creation

import java.io.File

internal fun CreationFileStore.planPublishIntent(
    dispatchId: String,
    requestedName: String,
    mimeType: String,
    destination: String?,
    existingIntents: List<CreationPublishIntent>,
): CreationPublishIntent =
    outputs.plan(dispatchId, requestedName, mimeType, destination, existingIntents)

internal fun CreationFileStore.reservePublishIntent(
    intent: CreationPublishIntent,
): CreationPendingReservation = outputs.reserve(intent)

internal fun CreationFileStore.populatePublishIntent(
    intent: CreationPublishIntent,
    pendingHandle: String,
    pendingIdentity: String,
    source: File,
    expectedSize: Long,
    expectedSha256: String,
) = outputs.populate(
    intent, pendingHandle, pendingIdentity, source, expectedSize, expectedSha256,
)

internal fun CreationFileStore.commitPublishIntent(
    intent: CreationPublishIntent,
    pendingHandle: String,
    pendingIdentity: String,
    expectedSize: Long,
    expectedSha256: String,
): String = outputs.commit(
    intent, pendingHandle, pendingIdentity, expectedSize, expectedSha256,
)

internal fun CreationFileStore.recoveredPublication(
    intent: CreationPublishIntent,
    pendingHandle: String,
    pendingIdentity: String,
    expectedSize: Long,
    expectedSha256: String,
): String? = outputs.recoveredPublication(
    intent,
    pendingHandle,
    pendingIdentity,
    expectedSize,
    expectedSha256,
)

internal fun CreationFileStore.abortPublishIntent(
    intent: CreationPublishIntent,
    pendingHandle: String,
    pendingIdentity: String,
): Boolean = outputs.abort(intent, pendingHandle, pendingIdentity)

internal fun CreationFileStore.abortPreparedPublishIntent(
    intent: CreationPublishIntent,
    pendingHandle: String,
    pendingIdentity: String,
    expectedSize: Long,
    expectedSha256: String,
): Boolean = outputs.abortPrepared(
    intent,
    pendingHandle,
    pendingIdentity,
    expectedSize,
    expectedSha256,
)

internal fun CreationFileStore.publishedArtifactMatches(
    path: String,
    identity: String,
    expectedSize: Long,
    expectedSha256: String,
): Boolean = outputs.publishedArtifactMatches(
    path,
    identity,
    expectedSize,
    expectedSha256,
)

internal fun CreationFileStore.artifactIdentity(path: String): String? =
    outputs.identity(path)
