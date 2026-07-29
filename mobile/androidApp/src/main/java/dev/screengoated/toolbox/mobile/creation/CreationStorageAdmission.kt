package dev.screengoated.toolbox.mobile.creation

import java.io.File
import org.json.JSONArray

internal const val CREATION_MANAGED_STORAGE_CAP_BYTES = 4L * 1024 * 1024 * 1024
internal const val CREATION_FREE_STORAGE_RESERVE_BYTES = 1L * 1024 * 1024 * 1024
internal const val CREATION_STORAGE_PRESSURE_TRIGGER_BYTES = 256L * 1024 * 1024
internal const val CREATION_STORAGE_PRESSURE_RECOVERY_BYTES = 512L * 1024 * 1024
internal const val CREATION_STORAGE_UNAVAILABLE_ERROR_KEY = "creation_storage_unavailable"
internal const val CREATION_SOURCE_UNAVAILABLE_ERROR_KEY = "creation_source_unavailable"

internal class CreationStorageUnavailableException :
    IllegalStateException(CREATION_STORAGE_UNAVAILABLE_ERROR_KEY)

internal class CreationSourceUnavailableException :
    IllegalStateException(CREATION_SOURCE_UNAVAILABLE_ERROR_KEY)

internal fun creationSubmissionFailure(error: Throwable): CreationSubmissionFailure = when {
    error is CreationStorageUnavailableException ||
        error.message == CREATION_STORAGE_UNAVAILABLE_ERROR_KEY ->
        CreationSubmissionFailure.STORAGE_UNAVAILABLE
    error is CreationSourceUnavailableException ||
        error.message == CREATION_SOURCE_UNAVAILABLE_ERROR_KEY ->
        CreationSubmissionFailure.SOURCE_UNAVAILABLE
    else -> CreationSubmissionFailure.INVALID_REQUEST
}

internal data class CreationStorageAdmissionPlan(
    val pruneBudgetBytes: Long,
    val requiredAvailableBytes: Long,
    val accepted: Boolean,
)

internal data class CreationStorageRequirements(
    val internalBytes: Long,
    val destinationBytes: Long,
)

internal data class CreationPendingStorageReservations(
    val internalBytes: Long = 0L,
    val destinationBytes: Map<String, Long> = emptyMap(),
)

internal fun creationStorageRequirements(
    sourceSnapshotBytes: Long,
    resultBytes: Long,
    managedDestination: Boolean,
    pendingInternalBytes: Long = 0L,
    pendingDestinationBytes: Long = 0L,
): CreationStorageRequirements {
    require(sourceSnapshotBytes >= 0L)
    require(resultBytes >= 0L)
    require(pendingInternalBytes >= 0L)
    require(pendingDestinationBytes >= 0L)
    val staging = creationSaturatingBytes(sourceSnapshotBytes, resultBytes)
    return CreationStorageRequirements(
        internalBytes = creationSaturatingBytes(
            pendingInternalBytes,
            if (managedDestination) creationSaturatingBytes(staging, resultBytes) else staging,
        ),
        destinationBytes = if (managedDestination) {
            pendingDestinationBytes
        } else {
            creationSaturatingBytes(pendingDestinationBytes, resultBytes)
        },
    )
}

internal fun creationPendingStorageReservations(
    filesDir: File,
    sizeOf: (String) -> Long,
): CreationPendingStorageReservations {
    val deliveries = readCreationStorageArray(
        File(filesDir, "creation/state/deliveries.json"),
        CREATION_DELIVERY_INDEX_MAX_BYTES,
    )
    val deliveryDispatches = mutableSetOf<String>()
    var internal = 0L
    val destinations = mutableMapOf<String, Long>()
    for (index in 0 until deliveries.length()) {
        val record = deliveries.getJSONObject(index)
        val dispatchId = record.getString("dispatchId")
        require(deliveryDispatches.add(dispatchId))
        val request = record.getJSONObject("request")
        val maximumResultBytes = maximumCreationResultBytes(
            requireNotNull(CreationTool.fromWireName(request.getString("tool"))),
        )
        val artifactBytes = record.getLong("artifactSize")
        require(artifactBytes in 1..maximumResultBytes)
        if (record.isNull("publishedPath")) {
            val missingBytes = creationMissingDeliveryBytes(
                artifactBytes,
                record.optString("pendingHandle").takeIf {
                    !record.isNull("pendingHandle") && it.isNotBlank()
                },
                record.optBoolean("publicationPrepared", false),
                sizeOf,
            )
            val intent = record.getJSONObject("intent")
            if (intent.getString("kind") == "managed") {
                internal = creationSaturatingBytes(internal, missingBytes)
            } else {
                val destination = intent.getString("destination")
                destinations[destination] = creationSaturatingBytes(
                    destinations.getOrDefault(destination, 0L),
                    missingBytes,
                )
            }
        }
    }
    val journal = readCreationStorageArray(
        File(filesDir, "creation/state/accepted-jobs.json"),
        CREATION_JOURNAL_INDEX_MAX_BYTES,
    )
    for (index in 0 until journal.length()) {
        val record = journal.getJSONObject(index)
        val status = record.getJSONObject("status")
        if (!creationStageIsBusy(status.getString("stage"))) continue
        val request = record.getJSONObject("request")
        if (request.getString("dispatchId") in deliveryDispatches) continue
        val resultBytes = maximumCreationResultBytes(
            requireNotNull(CreationTool.fromWireName(request.getString("tool"))),
        )
        val stagedBytes = sizeOf(request.getString("outputPath")).coerceAtLeast(0L)
        internal = creationSaturatingBytes(
            internal,
            (resultBytes - stagedBytes.coerceAtMost(resultBytes)).coerceAtLeast(0L),
        )
        if (record.isNull("destination")) {
            internal = creationSaturatingBytes(internal, resultBytes)
        } else {
            val destination = record.getString("destination")
            destinations[destination] = creationSaturatingBytes(
                destinations.getOrDefault(destination, 0L),
                resultBytes,
            )
        }
    }
    return CreationPendingStorageReservations(internal, destinations)
}

internal fun creationMissingDeliveryBytes(
    artifactBytes: Long,
    pendingHandle: String?,
    publicationPrepared: Boolean,
    sizeOf: (String) -> Long,
): Long {
    require(artifactBytes >= 0L)
    if (publicationPrepared) return 0L
    val materialized = pendingHandle?.let(sizeOf)?.coerceAtLeast(0L) ?: 0L
    return (artifactBytes - materialized.coerceAtMost(artifactBytes))
        .coerceAtLeast(0L)
}

private fun readCreationStorageArray(file: File, maximumBytes: Long): JSONArray {
    if (!file.exists()) return JSONArray()
    return runCatching {
        JSONArray(requireNotNull(readCreationIndexTextBounded(file, maximumBytes)))
    }.getOrElse { throw CreationStorageUnavailableException() }
}

internal val creationImportAdmissionLock = Any()

internal fun planCreationStorageAdmission(
    totalManagedBytes: Long,
    protectedManagedBytes: Long,
    availableBytes: Long,
    additionalBytes: Long,
    storageCapBytes: Long = CREATION_MANAGED_STORAGE_CAP_BYTES,
    freeReserveBytes: Long = CREATION_FREE_STORAGE_RESERVE_BYTES,
): CreationStorageAdmissionPlan {
    require(totalManagedBytes >= 0L)
    require(protectedManagedBytes in 0..totalManagedBytes)
    require(availableBytes >= 0L)
    require(additionalBytes >= 0L)
    require(storageCapBytes >= 0L)
    require(freeReserveBytes >= 0L)
    val pruneBudget = (storageCapBytes - additionalBytes).coerceAtLeast(0L)
    val requiredAvailable = creationSaturatingBytes(freeReserveBytes, additionalBytes)
    return CreationStorageAdmissionPlan(
        pruneBudgetBytes = pruneBudget,
        requiredAvailableBytes = requiredAvailable,
        accepted = additionalBytes <= storageCapBytes &&
            protectedManagedBytes <= pruneBudget &&
            totalManagedBytes <= pruneBudget &&
            availableBytes >= requiredAvailable,
    )
}

internal fun creationPressurePruneBudget(
    totalManagedBytes: Long,
    availableBytes: Long,
    requiredAvailableBytes: Long,
    capBudgetBytes: Long,
): Long {
    require(totalManagedBytes >= 0L)
    require(availableBytes >= 0L)
    require(requiredAvailableBytes >= 0L)
    require(capBudgetBytes >= 0L)
    val trigger = creationSaturatingBytes(
        requiredAvailableBytes,
        CREATION_STORAGE_PRESSURE_TRIGGER_BYTES,
    )
    if (availableBytes >= trigger) return capBudgetBytes
    val recovery = creationSaturatingBytes(
        requiredAvailableBytes,
        CREATION_STORAGE_PRESSURE_RECOVERY_BYTES,
    )
    val reclaim = (recovery - availableBytes).coerceAtLeast(0L)
    return minOf(capBudgetBytes, (totalManagedBytes - reclaim).coerceAtLeast(0L))
}

internal fun maximumCreationResultBytes(tool: CreationTool): Long = when (tool) {
    CreationTool.IMAGE_TO_3D -> CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES
    CreationTool.IMAGE_TO_SVG -> CreationContract.MAXIMUM_SVG_ARTIFACT_BYTES
    CreationTool.IMAGE_CREATOR -> CreationContract.MAXIMUM_IMAGE_ARTIFACT_BYTES
}

internal fun creationExternalStorageAccepted(
    availableBytes: Long?,
    destinationBytes: Long,
    freeReserveBytes: Long = CREATION_FREE_STORAGE_RESERVE_BYTES,
): Boolean {
    require(destinationBytes >= 0L)
    require(freeReserveBytes >= 0L)
    return availableBytes != null &&
        availableBytes >= creationSaturatingBytes(destinationBytes, freeReserveBytes)
}
