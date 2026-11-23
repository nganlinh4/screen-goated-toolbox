package dev.screengoated.toolbox.mobile.service

/**
 * Corner-handle resize shared by every floating overlay window. The upper
 * bound never drops below the minimum, so a window that sits past the screen
 * edge (for example after rotation) clamps to its minimum instead of throwing.
 *
 * Returns null for an unknown corner.
 */
internal fun resizeOverlayFromCorner(
    current: OverlayBounds,
    corner: String,
    dx: Int,
    dy: Int,
    screenWidth: Int,
    screenHeight: Int,
    minWidth: Int,
    minHeight: Int,
): OverlayBounds? {
    val nextHeight = clampOverlaySpan(current.height + dy, minHeight, screenHeight - current.y)
    return when (corner) {
        "br" -> current.copy(
            width = clampOverlaySpan(current.width + dx, minWidth, screenWidth - current.x),
            height = nextHeight,
        )
        "bl" -> {
            val nextWidth = clampOverlaySpan(current.width - dx, minWidth, current.x + current.width)
            current.copy(
                x = (current.x + current.width - nextWidth).coerceAtLeast(0),
                width = nextWidth,
                height = nextHeight,
            )
        }
        else -> null
    }
}

private fun clampOverlaySpan(value: Int, min: Int, max: Int): Int =
    value.coerceIn(min, max.coerceAtLeast(min))
