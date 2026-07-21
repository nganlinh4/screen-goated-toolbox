package dev.screengoated.toolbox.mobile.creation

internal object CreationContract {
    const val DEFAULT_POLYCOUNT = 5_000
    const val MINIMUM_POLYCOUNT = 500
    const val MAXIMUM_POLYCOUNT = 20_000
    const val MAXIMUM_PARALLEL_JOBS = 2
    const val MAXIMUM_CONCURRENT_PREPARATIONS = 1
    const val MINIMUM_PREPARATION_INTERVAL_SECONDS = 60
    const val IMAGE_TO_3D_WORKSPACES = 4
    const val IMAGE_TO_SVG_WORKSPACES = 2
    const val SVG_MINIMUM_REUSABLE_CREDITS = 4

    fun svgRemoteModel(model: String): String = if (model == "detail") "Ultra" else "Classic"

    fun svgCreditCost(model: String): Int = if (model == "detail") 4 else 2
}
