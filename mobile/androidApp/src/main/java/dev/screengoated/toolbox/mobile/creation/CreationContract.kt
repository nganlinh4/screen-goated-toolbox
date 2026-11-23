package dev.screengoated.toolbox.mobile.creation

internal enum class CreationGenerationMode(
    val wireName: String,
) {
    FAST("fast"),
    QUALITY("quality");

    companion object {
        fun fromWireName(value: String?): CreationGenerationMode =
            if (value == null) QUALITY else requireNotNull(entries.firstOrNull { it.wireName == value }) {
                "Unknown generation mode"
            }
    }
}

internal data class CreationModeRoute(
    val mode: CreationGenerationMode,
    val polycount: Int,
    val autoSegment: Boolean,
    val showAutoSegment: Boolean,
)

internal object CreationContract {
    val REFINEMENT_ACTIONS = setOf(
        "separate_parts", "optimize_triangle", "optimize_quad",
        "add_materials", "generate_pbr", "rig", "animate",
    )

    fun refinementCapability(kind: String): String = when {
        kind.startsWith("separate_") -> "separate_parts"
        kind == "materials" -> "add_materials"
        kind == "pbr" -> "generate_pbr"
        kind.startsWith("animate_") -> "animate"
        else -> kind
    }
    const val DEFAULT_POLYCOUNT = 5_000
    const val MINIMUM_POLYCOUNT = 500
    const val MAXIMUM_POLYCOUNT = 20_000
    const val FAST_MAXIMUM_POLYCOUNT = 15_000
    const val FAST_MINIMUM_POLYCOUNT = 100
    const val QUALITY_MINIMUM_POLYCOUNT = MINIMUM_POLYCOUNT
    const val MAXIMUM_PARALLEL_JOBS = 2
    const val MINIMUM_PREPARED_CAPACITY = 1
    const val MAXIMUM_OPTIONAL_INSTRUCTION_CHARACTERS = 1_000
    const val MAXIMUM_PICKER_BATCH_IMAGES = 100
    const val MAXIMUM_PICKER_AGGREGATE_BYTES = 512L * 1024 * 1024
    const val MAXIMUM_SOURCE_IMAGE_BYTES = 25L * 1024 * 1024
    const val MAXIMUM_IMAGE_DIMENSION = 32_768
    const val MAXIMUM_DECODED_IMAGE_PIXELS = 64_000_000L
    const val MAXIMUM_IMAGE_ARTIFACT_BYTES = 64L * 1024 * 1024
    const val MAXIMUM_JOB_RUNTIME_MS = 7_200_000L
    const val TEMPORARY_CAPACITY_RECOVERY_MS = 16L * 60 * 1000
    const val MAXIMUM_GLB_ARTIFACT_BYTES = 100L * 1024 * 1024
    const val MAXIMUM_SVG_ARTIFACT_BYTES = 12L * 1024 * 1024
    const val MAXIMUM_SVG_ELEMENTS = 50_000
    const val MAXIMUM_SVG_ATTRIBUTES = 250_000
    const val MAXIMUM_SVG_EMBEDDED_RASTER_CHARACTERS = 2_800_000
    const val MAXIMUM_SVG_EMBEDDED_RASTER_PIXELS = 16_000_000L
    const val MAXIMUM_SVG_TOTAL_EMBEDDED_RASTER_PIXELS = 32_000_000L
    const val MAXIMUM_EDITABLE_SVG_BYTES = 2L * 1024 * 1024
    const val MAXIMUM_EDITABLE_SVG_GEOMETRY = 5_000

    fun maximumParallelJobs(tool: CreationTool): Int = when (tool) {
        CreationTool.IMAGE_TO_3D,
        CreationTool.IMAGE_TO_SVG,
        -> MAXIMUM_PARALLEL_JOBS
    }

    fun maximumConcurrentPreparations(tool: CreationTool): Int = when (tool) {
        CreationTool.IMAGE_TO_3D,
        CreationTool.IMAGE_TO_SVG,
        -> MAXIMUM_PARALLEL_JOBS
    }

    fun minimumPreparedCapacity(tool: CreationTool): Int = when (tool) {
        CreationTool.IMAGE_TO_3D,
        CreationTool.IMAGE_TO_SVG,
        -> MINIMUM_PREPARED_CAPACITY
    }

    fun route3dMode(
        mode: CreationGenerationMode,
        polycount: Int,
        requestedAutoSegment: Boolean,
    ): CreationModeRoute = route3dMode(mode, polycount.toLong(), requestedAutoSegment)

    fun route3dMode(
        mode: CreationGenerationMode,
        polycount: Long,
        requestedAutoSegment: Boolean,
    ): CreationModeRoute {
        require(mode == CreationGenerationMode.QUALITY) { "Choose Quality for a new model" }
        require(polycount in 0L..4_294_967_295L) { "Polycount is invalid" }
        return CreationModeRoute(mode, polycount.coerceIn(MINIMUM_POLYCOUNT.toLong(), MAXIMUM_POLYCOUNT.toLong()).toInt(), requestedAutoSegment, true)
    }

    fun newGenerationMode(value: String?): CreationGenerationMode =
        CreationGenerationMode.fromWireName(value).also {
            require(it == CreationGenerationMode.QUALITY) { "Choose Quality for a new model" }
        }

    fun frozen3dMode(mode: CreationGenerationMode, polycount: Int, autoSegment: Boolean): CreationModeRoute {
        require(when (mode) {
            CreationGenerationMode.FAST -> polycount in FAST_MINIMUM_POLYCOUNT..FAST_MAXIMUM_POLYCOUNT && !autoSegment
            CreationGenerationMode.QUALITY -> polycount in MINIMUM_POLYCOUNT..MAXIMUM_POLYCOUNT
        }) { "Saved generation settings are invalid" }
        return CreationModeRoute(mode, polycount, autoSegment, mode == CreationGenerationMode.QUALITY)
    }

    fun initialTopology(mode: CreationGenerationMode, autoSegment: Boolean, requested: String?): String =
        if (mode == CreationGenerationMode.FAST || autoSegment || requested == "triangle") "triangle" else "quad"

    fun canContinueSegmentation(
        isSegmented: Boolean,
        runtimeAllowsContinuation: Boolean,
    ): Boolean = !isSegmented && runtimeAllowsContinuation

}
