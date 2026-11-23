package dev.screengoated.toolbox.mobile.preset

import kotlin.math.roundToInt

internal const val GEMINI_LIVE_STILL_FRAME_MAX_LONG_EDGE = 768

internal fun geminiLiveStillFrameSize(width: Int, height: Int): Pair<Int, Int> {
    require(width > 0 && height > 0)
    val longEdge = maxOf(width, height)
    if (longEdge <= GEMINI_LIVE_STILL_FRAME_MAX_LONG_EDGE) return width to height
    val scale = GEMINI_LIVE_STILL_FRAME_MAX_LONG_EDGE.toDouble() / longEdge
    return (width * scale).roundToInt().coerceAtLeast(1) to
        (height * scale).roundToInt().coerceAtLeast(1)
}
