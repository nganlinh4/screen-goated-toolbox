package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.ui.i18n.CreationCommonLocale
internal fun publicCreationFailure(): String = "Creation could not finish. Try again."

internal fun publicCreationFailure(tool: CreationTool): String =
    if (tool == CreationTool.IMAGE_CREATOR) publicImageCreationFailure() else publicCreationFailure()

internal fun publicCreationFailureCategory(value: String?): String = value?.takeIf {
    it in setOf(
        "cancelled",
        "execution_lost",
        "input",
        "output",
        "runtime_unavailable",
        "timeout",
        "unsupported",
    )
} ?: "unexpected"

internal fun publicCreationStage(
    tool: CreationTool,
    observed: String,
    current: String,
    hasReferences: Boolean = true,
): String {
    if (tool == CreationTool.IMAGE_CREATOR) {
        val stage = publicImageCreationStage(observed)
        return if (stage == "uploading" && !hasReferences) "preparing" else stage
    }
    val allowed = when (tool) {
        CreationTool.IMAGE_TO_3D -> setOf("preparing", "generating", "segmenting", "finalizing")
        CreationTool.IMAGE_TO_SVG -> setOf("preparing", "generating", "finalizing")
        CreationTool.IMAGE_CREATOR -> error("Handled above")
    }
    return observed.takeIf(allowed::contains)
        ?: current.takeIf(allowed::contains)
        ?: "preparing"
}

internal fun publicCreationProgressText(stage: String): String = when (stage) {
    "uploading" -> "Adding reference image"
    "generating" -> "Creating result"
    "segmenting" -> "Separating model parts"
    "finalizing" -> "Finishing result"
    else -> "Getting ready"
}

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

internal fun publicCreationErrorText(
    value: String,
    common: CreationCommonLocale,
): String = publicCreationErrorText(
    value,
    common.storageUnavailable,
    common.sourceUnavailable,
    common.interrupted,
)

internal fun publicCreationErrorText(
    value: String,
    storageUnavailable: String,
    sourceUnavailable: String,
    genericFailure: String,
): String = when (value) {
    CREATION_STORAGE_UNAVAILABLE_ERROR_KEY -> storageUnavailable
    CREATION_SOURCE_UNAVAILABLE_ERROR_KEY -> sourceUnavailable
    else -> genericFailure
}

internal fun publicCreationThrowable(
    error: Throwable,
    tool: CreationTool,
): String = when {
    error is CreationStorageUnavailableException ||
        error.message == CREATION_STORAGE_UNAVAILABLE_ERROR_KEY ->
        CREATION_STORAGE_UNAVAILABLE_ERROR_KEY
    error is CreationSourceUnavailableException ||
        error.message == CREATION_SOURCE_UNAVAILABLE_ERROR_KEY ->
        CREATION_SOURCE_UNAVAILABLE_ERROR_KEY
    else -> publicCreationFailure(tool)
}
