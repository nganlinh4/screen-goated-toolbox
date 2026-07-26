package dev.screengoated.toolbox.mobile.creation

internal sealed interface CreationWorkerFailureRoute {
    data object Fail : CreationWorkerFailureRoute
    data class Redispatch(val preferredWorker: String) : CreationWorkerFailureRoute
}

internal fun publicCreationText(value: String): String = value
    .replace("Meshy T2", "creation service", ignoreCase = true)
    .replace("Meshy", "creation service", ignoreCase = true)
    .replace("Tripo", "creation service", ignoreCase = true)

internal fun publicImageCreationStage(value: String): String = when (value) {
    "queued",
    "preparing",
    "uploading",
    "generating",
    "finalizing",
    "done",
    "failed",
    "cancelled",
    -> value
    else -> "preparing"
}

internal fun publicImageCreationText(
    stage: String,
    hasReferences: Boolean = true,
): String = when (stage) {
    "queued" -> "Queued"
    "uploading" -> if (hasReferences) "Adding reference image" else "Getting ready"
    "generating" -> "Creating image"
    "finalizing" -> "Finishing image"
    "done" -> "Image ready"
    "failed" -> "Could not create image"
    "cancelled" -> "Cancelled"
    else -> "Getting ready"
}

internal fun publicImageCreationFailure(): String =
    "Image creation could not finish. Try again."

internal fun routeCreationWorkerFailure(
    provider: String?,
    error: String,
): CreationWorkerFailureRoute {
    val prefix = when (provider) {
        CreationProvider.MESHY.wireName -> CreationContract.MESHY_RECOVERY_OWNER_PREFIX
        CreationProvider.TRIPO.wireName -> CreationContract.TRIPO_RECOVERY_OWNER_PREFIX
        else -> return CreationWorkerFailureRoute.Fail
    }
    if (!error.startsWith(prefix)) return CreationWorkerFailureRoute.Fail
    val ownerSlot = error.removePrefix(prefix).toIntOrNull()
    val workerCount = if (provider == CreationProvider.MESHY.wireName) {
        CreationContract.IMAGE_TO_3D_MESHY_WORKSPACES
    } else {
        CreationContract.IMAGE_TO_3D_WORKSPACES
    }
    require(ownerSlot != null && ownerSlot in 0 until workerCount) {
        "Creation runtime returned an invalid recovery owner"
    }
    return CreationWorkerFailureRoute.Redispatch("3d-$ownerSlot")
}
