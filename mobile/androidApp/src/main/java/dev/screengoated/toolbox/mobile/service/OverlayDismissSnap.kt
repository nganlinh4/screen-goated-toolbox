package dev.screengoated.toolbox.mobile.service

import android.animation.ValueAnimator
import android.graphics.Rect
import dev.screengoated.toolbox.mobile.service.overlay.OverlayPaneWindow
import kotlin.math.roundToInt

/**
 * Drives the "magnetic dismiss" motion for the live-translate panes: while a
 * drag is near a dismiss bubble, panes spring so their centre lands on the
 * bubble (single → the dragged pane only; all → every pane, fanned into a
 * stack to express the multiple overlays). Leaving the zone springs them back
 * to where the finger left them. A per-frame critically-damped follow keeps
 * the motion continuous and re-targetable as the finger and mode change.
 *
 * Coordinates are physical pixels. Targets come from [DismissBubbleController].
 */
internal class OverlayDismissSnap(
    private val density: Float,
    private val screenBounds: () -> Rect,
    private val targetCenter: (DismissAction) -> Pair<Float, Float>?,
) {
    private class PaneState(
        val window: OverlayPaneWindow,
        var freeBounds: OverlayBounds,
        var x: Float,
        var y: Float,
        var scale: Float = 1f,
        var rotation: Float = 0f,
        var alpha: Float = 1f,
    )

    private val panes = LinkedHashMap<OverlayPaneId, PaneState>()
    private var draggedPaneId: OverlayPaneId? = null
    private var mode: DismissAction = DismissAction.NONE
    private var ticker: ValueAnimator? = null

    // True once the dragged pane has been pulled into a bubble; while it is set
    // and the mode is NONE the pane eases back to the finger instead of snapping
    // there 1:1 (so leaving a bubble reads as a smooth fly-back, not a teleport).
    private var draggedDetached = false

    val currentMode: DismissAction get() = mode

    val isActive: Boolean get() = draggedPaneId != null

    fun begin(
        draggedPaneId: OverlayPaneId,
        windows: List<Pair<OverlayPaneId, OverlayPaneWindow>>,
    ) {
        if (isActive) return
        this.draggedPaneId = draggedPaneId
        mode = DismissAction.NONE
        panes.clear()
        windows.forEach { (id, window) ->
            val bounds = window.currentBounds()
            panes[id] = PaneState(
                window = window,
                freeBounds = bounds,
                x = bounds.x.toFloat(),
                y = bounds.y.toFloat(),
            )
        }
        startTicker()
    }

    /** Apply a finger delta to the dragged pane's free (un-snapped) position. */
    fun onDragDelta(dx: Int, dy: Int) {
        val id = draggedPaneId ?: return
        val pane = panes[id] ?: return
        val screen = screenBounds()
        val b = pane.freeBounds
        pane.freeBounds = b.copy(
            x = (b.x + dx).coerceIn(0, (screen.width() - b.width).coerceAtLeast(0)),
            y = (b.y + dy).coerceIn(0, (screen.height() - b.height).coerceAtLeast(0)),
        )
    }

    fun setMode(next: DismissAction) {
        if (mode == next) return
        if (next != DismissAction.NONE) draggedDetached = true
        mode = next
    }

    fun freeBoundsOf(id: OverlayPaneId): OverlayBounds? = panes[id]?.freeBounds

    /**
     * End the gesture. For SINGLE/ALL the snapped panes swallow into the bubble
     * then [onCommit] fires; for NONE the panes settle back and [onSettle] fires.
     */
    fun finish(
        committed: DismissAction,
        onCommit: () -> Unit,
        onSettle: () -> Unit,
    ) {
        stopTicker()
        when (committed) {
            DismissAction.ALL -> {
                var pending = panes.size.coerceAtLeast(1)
                if (panes.isEmpty()) onCommit()
                panes.values.forEach { pane ->
                    pane.window.animateSwallow {
                        if (--pending == 0) onCommit()
                    }
                }
            }
            DismissAction.SINGLE -> {
                val dragged = draggedPaneId?.let { panes[it] }
                if (dragged == null) {
                    onCommit()
                } else {
                    dragged.window.animateSwallow { onCommit() }
                }
            }
            DismissAction.NONE -> {
                panes.values.forEach { pane ->
                    pane.window.animateRestoreTo(pane.freeBounds)
                }
                onSettle()
            }
        }
        reset()
    }

    fun cancel() {
        stopTicker()
        panes.values.forEach { it.window.resetTransform() }
        reset()
    }

    private fun reset() {
        panes.clear()
        draggedPaneId = null
        mode = DismissAction.NONE
        draggedDetached = false
    }

    private fun startTicker() {
        stopTicker()
        ticker = ValueAnimator.ofFloat(0f, 1f).apply {
            duration = 1_000L
            repeatCount = ValueAnimator.INFINITE
            interpolator = null
            addUpdateListener { tick() }
            start()
        }
    }

    private fun stopTicker() {
        ticker?.cancel()
        ticker = null
    }

    private fun tick() {
        val draggedId = draggedPaneId ?: return
        val singleCenter = targetCenter(DismissAction.SINGLE)
        val allCenter = targetCenter(DismissAction.ALL)
        val stackDx = STACK_DX_DP * density
        val stackDy = STACK_DY_DP * density

        var index = 0
        for ((id, pane) in panes) {
            val b = pane.freeBounds
            val isDragged = id == draggedId

            // Resolve this pane's target visual state for the current mode.
            var tx = b.x.toFloat()
            var ty = b.y.toFloat()
            var tScale = 1f
            var tRot = 0f
            var tAlpha = 1f
            var snapTarget = false

            when (mode) {
                DismissAction.SINGLE -> if (isDragged && singleCenter != null) {
                    tx = singleCenter.first - b.width / 2f
                    ty = singleCenter.second - b.height / 2f
                    tScale = SNAP_SCALE
                    tAlpha = SNAP_ALPHA
                    snapTarget = true
                }
                DismissAction.ALL -> if (allCenter != null) {
                    // Fan the panes so they read as a stack of cards.
                    val fan = index - (panes.size - 1) / 2f
                    tx = allCenter.first - b.width / 2f + fan * stackDx
                    ty = allCenter.second - b.height / 2f - index * stackDy
                    tScale = SNAP_SCALE
                    tRot = fan * STACK_TILT_DEG
                    tAlpha = SNAP_ALPHA
                    snapTarget = true
                }
                DismissAction.NONE -> Unit
            }

            if (isDragged && !snapTarget) {
                if (draggedDetached) {
                    // Returning from a bubble: ease back to the finger, then lock 1:1.
                    pane.x += (tx - pane.x) * POS_STIFFNESS
                    pane.y += (ty - pane.y) * POS_STIFFNESS
                    if (kotlin.math.abs(tx - pane.x) < 1f && kotlin.math.abs(ty - pane.y) < 1f) {
                        pane.x = tx
                        pane.y = ty
                        draggedDetached = false
                    }
                } else {
                    // Crisp 1:1 dragging when not snapped — no spring lag on the finger.
                    pane.x = tx
                    pane.y = ty
                }
                pane.scale += (1f - pane.scale) * VISUAL_STIFFNESS
                pane.rotation += (0f - pane.rotation) * VISUAL_STIFFNESS
                pane.alpha += (1f - pane.alpha) * VISUAL_STIFFNESS
            } else {
                pane.x += (tx - pane.x) * POS_STIFFNESS
                pane.y += (ty - pane.y) * POS_STIFFNESS
                pane.scale += (tScale - pane.scale) * VISUAL_STIFFNESS
                pane.rotation += (tRot - pane.rotation) * VISUAL_STIFFNESS
                pane.alpha += (tAlpha - pane.alpha) * VISUAL_STIFFNESS
            }

            pane.window.setVisualState(
                x = pane.x.roundToInt(),
                y = pane.y.roundToInt(),
                scale = pane.scale,
                rotationDeg = pane.rotation,
                alpha = pane.alpha,
            )
            index++
        }
    }

    private companion object {
        const val SNAP_SCALE = 0.42f
        const val SNAP_ALPHA = 0.96f
        const val POS_STIFFNESS = 0.32f
        const val VISUAL_STIFFNESS = 0.30f
        const val STACK_DX_DP = 20f
        const val STACK_DY_DP = 16f
        const val STACK_TILT_DEG = 7f
    }
}
