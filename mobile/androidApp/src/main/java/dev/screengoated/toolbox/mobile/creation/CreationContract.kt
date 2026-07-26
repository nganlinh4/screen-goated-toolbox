package dev.screengoated.toolbox.mobile.creation

internal enum class CreationProvider(
    val wireName: String,
) {
    MESHY("meshy"),
    TRIPO("tripo");

    companion object {
        fun fromWireName(value: String?): CreationProvider? = entries.firstOrNull {
            it.wireName == value
        }
    }
}

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

internal data class CreationProviderRoute(
    val mode: CreationGenerationMode,
    val polycount: Int,
    val provider: CreationProvider,
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
    const val MAXIMUM_CONCURRENT_PREPARATIONS = 1
    const val MINIMUM_PREPARATION_INTERVAL_SECONDS = 60
    const val IMAGE_TO_3D_WORKSPACES = 4
    const val IMAGE_TO_3D_MESHY_WORKSPACES = 2
    const val MESHY_RECOVERY_OWNER_PREFIX = "meshy-recovery-owner:"
    const val TRIPO_RECOVERY_OWNER_PREFIX = "quality-recovery-owner:"
    const val IMAGE_TO_SVG_WORKSPACES = 2
    const val IMAGE_CREATOR_WORKSPACES = 4
    const val IMAGE_CREATOR_OPERATION = "create_image_from_reference"
    const val IMAGE_CREATOR_MAXIMUM_PROMPT_CHARACTERS = 4_000
    const val IMAGE_CREATOR_MAXIMUM_REFERENCE_IMAGES = 20

    fun maximumParallelJobs(tool: CreationTool): Int = when (tool) {
        CreationTool.IMAGE_CREATOR -> IMAGE_CREATOR_MAXIMUM_PARALLEL_JOBS
        CreationTool.IMAGE_TO_3D,
        CreationTool.IMAGE_TO_SVG,
        -> MAXIMUM_PARALLEL_JOBS
    }

    fun route3dProvider(
        mode: CreationGenerationMode,
        polycount: Int,
        requestedAutoSegment: Boolean,
    ): CreationProviderRoute {
        require(polycount in MINIMUM_POLYCOUNT..MAXIMUM_POLYCOUNT) {
            "Polycount must be between $MINIMUM_POLYCOUNT and $MAXIMUM_POLYCOUNT"
        }
        return when (mode) {
            CreationGenerationMode.FAST -> CreationProviderRoute(
                mode = mode,
                polycount = polycount.coerceAtMost(FAST_MAXIMUM_POLYCOUNT),
                provider = CreationProvider.MESHY,
                autoSegment = false,
                showAutoSegment = false,
            )
            CreationGenerationMode.QUALITY -> CreationProviderRoute(
                mode = mode,
                polycount = polycount.coerceAtLeast(QUALITY_MINIMUM_POLYCOUNT),
                provider = CreationProvider.TRIPO,
                autoSegment = requestedAutoSegment,
                showAutoSegment = true,
            )
        }
    }

    fun validate3dProvider(
        mode: CreationGenerationMode,
        polycount: Int,
        requestedAutoSegment: Boolean,
        requestedProvider: String?,
    ): CreationProviderRoute {
        val route = route3dProvider(mode, polycount, requestedAutoSegment)
        require(requestedProvider == null || requestedProvider == route.provider.wireName) {
            "Provider $requestedProvider conflicts with explicit mode ${mode.wireName}"
        }
        return route
    }

    fun canContinueSegmentation(
        provider: String?,
        isSegmented: Boolean,
        runtimeAllowsContinuation: Boolean,
    ): Boolean = provider == CreationProvider.TRIPO.wireName &&
        !isSegmented &&
        runtimeAllowsContinuation

    fun canUse3dWorker(provider: String?, slot: Int): Boolean =
        provider != CreationProvider.MESHY.wireName ||
            slot in 0 until IMAGE_TO_3D_MESHY_WORKSPACES
}
