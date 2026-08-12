package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import android.content.Context
import android.graphics.Rect
import android.os.Build
import android.view.WindowManager

internal fun Context.dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

internal fun WindowManager.LayoutParams.configureFullDisplayLayout(bounds: Rect) {
    width = bounds.width()
    height = bounds.height()
    x = bounds.left
    y = bounds.top
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) fitInsetsTypes = 0
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
        layoutInDisplayCutoutMode =
            WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
    }
}

internal fun needsOverlayLayoutUpdate(
    forceLayout: Boolean,
    windowSetChanged: Boolean,
): Boolean = forceLayout || windowSetChanged

internal fun farthestOverlayCorner(
    screen: OverlayBounds,
    overlayWidth: Int,
    overlayHeight: Int,
    margin: Int,
    avoid: OverlayBounds,
): Pair<Int, Int> {
    val left = screen.left + margin
    val top = screen.top + margin
    val right = (screen.right - overlayWidth - margin).coerceAtLeast(left)
    val bottom = (screen.bottom - overlayHeight - margin).coerceAtLeast(top)
    val avoidX = avoid.left.toLong() + (avoid.right - avoid.left) / 2L
    val avoidY = avoid.top.toLong() + (avoid.bottom - avoid.top) / 2L
    return listOf(left to top, right to top, left to bottom, right to bottom).maxBy { point ->
        val centerX = point.first.toLong() + overlayWidth / 2L
        val centerY = point.second.toLong() + overlayHeight / 2L
        val dx = centerX - avoidX
        val dy = centerY - avoidY
        dx * dx + dy * dy
    }
}
