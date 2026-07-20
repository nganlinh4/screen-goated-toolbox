package dev.screengoated.toolbox.mobile.phonecontrol.runtime

/** Failures that mean "try the next frame", not "screen capture is broken". */
internal fun isTransientScreenFrameFailure(code: String): Boolean = code in setOf(
    "surface_unavailable",
    "surface_unstable",
    "stale_frame",
    "screenshot_rate_limited",
)
