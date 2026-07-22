package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import android.content.Context
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

internal class OverlayCaptureGate {
    private val state = Mutex()

    @Volatile
    private var captureDepth = 0

    internal val depth: Int
        get() = captureDepth

    internal val isHidden: Boolean
        get() = captureDepth > 0

    internal suspend fun <T> withHidden(
        onHide: suspend (firstCapture: Boolean) -> Unit,
        onRestore: suspend (lastCapture: Boolean) -> Unit,
        block: suspend () -> T,
    ): T {
        var entered = false
        try {
            state.withLock {
                val firstCapture = captureDepth == 0
                captureDepth += 1
                entered = true
                onHide(firstCapture)
            }
            currentCoroutineContext().ensureActive()
            return block()
        } finally {
            if (entered) {
                withContext(NonCancellable) {
                    state.withLock {
                        check(captureDepth > 0) { "Overlay capture depth underflow" }
                        captureDepth -= 1
                        onRestore(captureDepth == 0)
                    }
                }
            }
        }
    }
}

internal fun Context.dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

internal fun needsOverlayLayoutUpdate(
    forceLayout: Boolean,
    windowSetChanged: Boolean,
    suppressionChanged: Boolean,
): Boolean = forceLayout || windowSetChanged || suppressionChanged

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
