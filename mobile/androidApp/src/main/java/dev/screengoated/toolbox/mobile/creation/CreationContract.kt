package dev.screengoated.toolbox.mobile.creation

internal enum class CreationGenerationMode(
    val wireName: String,
) {
    FAST("fast"),
    QUALITY("quality");

    companion object {
        fun fromWireName(value: String?): CreationGenerationMode =
            entries.firstOrNull { it.wireName == value } ?: QUALITY
    }
}

internal data class CreationModeRoute(
    val mode: CreationGenerationMode,
    val polycount: Int,
    val autoSegment: Boolean,
    val showAutoSegment: Boolean,
)

internal object CreationContract {
    const val DEFAULT_POLYCOUNT = 5_000
    const val MINIMUM_POLYCOUNT = 100
    const val MAXIMUM_POLYCOUNT = 20_000
    const val FAST_MAXIMUM_POLYCOUNT = 15_000
    const val QUALITY_MINIMUM_POLYCOUNT = 500
    const val MAXIMUM_PARALLEL_JOBS = 2
    const val IMAGE_CREATOR_MAXIMUM_PARALLEL_JOBS = 2
    const val IMAGE_CREATOR_OPERATION = "create_image"
    const val IMAGE_CREATOR_MAXIMUM_PROMPT_CHARACTERS = 4_000
    const val MAXIMUM_OPTIONAL_INSTRUCTION_CHARACTERS = 1_000
    const val IMAGE_CREATOR_MAXIMUM_REFERENCE_IMAGES = 20
    const val MAXIMUM_PICKER_BATCH_IMAGES = 100
    const val MAXIMUM_PICKER_AGGREGATE_BYTES = 512L * 1024 * 1024
    const val MAXIMUM_SOURCE_IMAGE_BYTES = 25L * 1024 * 1024
    const val MAXIMUM_IMAGE_REFERENCE_AGGREGATE_BYTES = 100L * 1024 * 1024
    const val MAXIMUM_IMAGE_DIMENSION = 32_768
    const val MAXIMUM_DECODED_IMAGE_PIXELS = 64_000_000L
    const val MAXIMUM_IMAGE_ARTIFACT_BYTES = 64L * 1024 * 1024
    const val MAXIMUM_JOB_RUNTIME_MS = 7_200_000L
    const val MAXIMUM_GLB_ARTIFACT_BYTES = 100L * 1024 * 1024
    const val MAXIMUM_SVG_ARTIFACT_BYTES = 12L * 1024 * 1024
    const val MAXIMUM_SVG_ELEMENTS = 50_000
    const val MAXIMUM_SVG_ATTRIBUTES = 250_000
    const val MAXIMUM_SVG_EMBEDDED_RASTER_CHARACTERS = 2_800_000
    const val MAXIMUM_SVG_EMBEDDED_RASTER_PIXELS = 16_000_000L
    const val MAXIMUM_SVG_TOTAL_EMBEDDED_RASTER_PIXELS = 32_000_000L
    const val MAXIMUM_EDITABLE_SVG_BYTES = 2L * 1024 * 1024
    const val MAXIMUM_EDITABLE_SVG_GEOMETRY = 5_000

    fun normalizedOperation(tool: CreationTool, value: String?): String? =
        if (tool == CreationTool.IMAGE_CREATOR && value == LEGACY_IMAGE_CREATOR_OPERATION) {
            IMAGE_CREATOR_OPERATION
        } else {
            value
        }

    fun maximumParallelJobs(tool: CreationTool): Int = when (tool) {
        CreationTool.IMAGE_CREATOR -> IMAGE_CREATOR_MAXIMUM_PARALLEL_JOBS
        CreationTool.IMAGE_TO_3D,
        CreationTool.IMAGE_TO_SVG,
        -> MAXIMUM_PARALLEL_JOBS
    }

    fun route3dMode(
        mode: CreationGenerationMode,
        polycount: Int,
        requestedAutoSegment: Boolean,
    ): CreationModeRoute {
        require(polycount in MINIMUM_POLYCOUNT..MAXIMUM_POLYCOUNT) {
            "Polycount must be between $MINIMUM_POLYCOUNT and $MAXIMUM_POLYCOUNT"
        }
        return when (mode) {
            CreationGenerationMode.FAST -> CreationModeRoute(
                mode = mode,
                polycount = polycount.coerceAtMost(FAST_MAXIMUM_POLYCOUNT),
                autoSegment = false,
                showAutoSegment = false,
            )
            CreationGenerationMode.QUALITY -> CreationModeRoute(
                mode = mode,
                polycount = polycount.coerceAtLeast(QUALITY_MINIMUM_POLYCOUNT),
                autoSegment = requestedAutoSegment,
                showAutoSegment = true,
            )
        }
    }

    fun canContinueSegmentation(
        isSegmented: Boolean,
        runtimeAllowsContinuation: Boolean,
    ): Boolean = !isSegmented && runtimeAllowsContinuation

    private const val LEGACY_IMAGE_CREATOR_OPERATION = "create_image_from_reference"
}
