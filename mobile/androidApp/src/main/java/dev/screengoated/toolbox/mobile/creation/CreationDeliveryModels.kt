package dev.screengoated.toolbox.mobile.creation

import kotlinx.serialization.Serializable

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
    val publishAttempts: Int = 0,
)

internal data class CreationReconciledTerminal(
    val request: CreationWorkerRequest,
    val status: CreationJobStatus,
    val continuation: CreationContinuation?,
)

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
