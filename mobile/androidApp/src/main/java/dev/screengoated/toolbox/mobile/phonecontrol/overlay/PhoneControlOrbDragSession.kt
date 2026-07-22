package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import dev.screengoated.toolbox.mobile.service.DismissAction
import dev.screengoated.toolbox.mobile.service.DismissHit
import kotlin.math.abs

internal enum class PhoneControlOrbDragRelease {
    TAP,
    MOVED,
    DISMISS,
}

internal data class PhoneControlOrbDragUpdate(
    val windowX: Int,
    val windowY: Int,
    val started: Boolean,
)

internal class PhoneControlOrbDragSession(
    private val thresholdPx: Float,
) {
    private var downX = 0f
    private var downY = 0f
    private var startX = 0
    private var startY = 0

    var dragging = false
        private set

    fun begin(rawX: Float, rawY: Float, windowX: Int, windowY: Int) {
        downX = rawX
        downY = rawY
        startX = windowX
        startY = windowY
        dragging = false
    }

    fun move(rawX: Float, rawY: Float): PhoneControlOrbDragUpdate? {
        val dx = rawX - downX
        val dy = rawY - downY
        val started = !dragging && (abs(dx) > thresholdPx || abs(dy) > thresholdPx)
        if (started) dragging = true
        if (!dragging) return null
        return PhoneControlOrbDragUpdate(
            windowX = startX + dx.toInt(),
            windowY = startY + dy.toInt(),
            started = started,
        )
    }

    fun release(hit: DismissHit?): PhoneControlOrbDragRelease {
        val result = when {
            !dragging -> PhoneControlOrbDragRelease.TAP
            hit?.committedAction() == DismissAction.SINGLE ->
                PhoneControlOrbDragRelease.DISMISS
            else -> PhoneControlOrbDragRelease.MOVED
        }
        dragging = false
        return result
    }

    fun cancel() {
        dragging = false
    }
}
