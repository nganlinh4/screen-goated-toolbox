package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.ui.i18n.CreationCommonLocale

internal fun publicCreationFailure(): String = "Creation could not finish. Try again."
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

internal fun CreationWorkerEvent.requiresSameDispatchRecovery(): Boolean =
    event == "execution_lost" || (event == "failure" && failureCode == "execution_lost")

internal fun publicCreationStage(
    tool: CreationTool,
    observed: String,
    current: String,
): String {
    val allowed = when (tool) {
        CreationTool.IMAGE_TO_3D ->
            setOf(
                "preparing",
                "waiting_for_user",
                "generating",
                "segmenting",
                "refining",
                "finalizing",
            )
        CreationTool.IMAGE_TO_SVG -> setOf("preparing", "generating", "finalizing")
    }
    return observed.takeIf(allowed::contains)
        ?: current.takeIf(allowed::contains)
        ?: "preparing"
}

internal fun publicCreationProgressText(stage: String): String = when (stage) {
    "waiting_for_user" -> "Complete the requested step in the secure window"
    "generating" -> "Creating result"
    "segmenting" -> "Separating model parts"
    "refining" -> "Creating a new version"
    "finalizing" -> "Finishing result"
    else -> "Getting ready"
}

internal fun publicCreationErrorText(
    value: String,
    common: CreationCommonLocale,
): String = publicCreationErrorText(
    value,
    common.storageUnavailable,
    common.sourceUnavailable,
    common.interrupted,
    common.svgOriginalsUnavailable,
)

internal fun publicCreationErrorText(
    value: String,
    storageUnavailable: String,
    sourceUnavailable: String,
    genericFailure: String,
    svgOriginalsUnavailable: String = genericFailure,
): String = when (value) {
    CREATION_STORAGE_UNAVAILABLE_ERROR_KEY -> storageUnavailable
    CREATION_SOURCE_UNAVAILABLE_ERROR_KEY -> sourceUnavailable
    CREATION_SVG_ORIGINALS_UNAVAILABLE -> svgOriginalsUnavailable
    else -> genericFailure
}

internal fun publicCreationThrowable(
    error: Throwable,
): String = when {
    error is CreationSvgOriginalsUnavailableException -> CREATION_SVG_ORIGINALS_UNAVAILABLE
    error is CreationStorageUnavailableException ||
        error.message == CREATION_STORAGE_UNAVAILABLE_ERROR_KEY ->
        CREATION_STORAGE_UNAVAILABLE_ERROR_KEY
    error is CreationSourceUnavailableException ||
        error.message == CREATION_SOURCE_UNAVAILABLE_ERROR_KEY ->
        CREATION_SOURCE_UNAVAILABLE_ERROR_KEY
    else -> publicCreationFailure()
}
