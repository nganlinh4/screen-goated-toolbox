package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.graphics.Rect
import android.view.WindowManager

/**
 * Which dismiss target a drag gesture resolved to.
 */
internal enum class DismissAction {
    NONE,
    SINGLE,
    ALL,
}

/**
 * Proximity (0–1) of a drag point to each dismiss target.
 */
internal data class DismissHit(
    val singleProximity: Float,
    val allProximity: Float,
) {
    /** Nearest active target purely by proximity, ignoring any commit threshold. */
    val nearest: DismissAction
        get() = when {
            singleProximity <= 0f && allProximity <= 0f -> DismissAction.NONE
            allProximity > singleProximity -> DismissAction.ALL
            else -> DismissAction.SINGLE
        }

    /** The action to commit on release: nearest target whose proximity clears [threshold]. */
    fun committedAction(threshold: Float = DEFAULT_COMMIT_THRESHOLD): DismissAction = when {
        allProximity >= threshold && allProximity >= singleProximity -> DismissAction.ALL
        singleProximity >= threshold -> DismissAction.SINGLE
        else -> DismissAction.NONE
    }

    companion object {
        const val DEFAULT_COMMIT_THRESHOLD = 0.8f
    }
}

/** Localized label for the "dismiss all" target. */
internal fun dismissAllLabel(uiLanguage: String): String = when (uiLanguage) {
    "vi" -> "Tất cả"
    "ko" -> "전체"
    else -> "All"
}

/**
 * Shared controller for the morphing dismiss bubbles shown while dragging any
 * floating overlay (live translate panes, preset result/input windows, help
 * assistant). Wraps [MorphDismissZone] so every surface gets identical visuals,
 * motion, hit-testing, and swallow animation.
 *
 * @param showDismissAll when true, renders a second "all" target (bottom-left)
 *   alongside the centre "single" target; when false, only the centre target.
 * @param allLabel localized label for the "all" target (e.g. "All"/"Tất cả"/"전체").
 */
internal class DismissBubbleController(
    private val context: Context,
    private val windowManager: WindowManager,
    private val showDismissAll: Boolean = true,
    private val allLabel: () -> String = { "All" },
    coordinateScaleOverride: Float? = null,
) {
    private val density = context.resources.displayMetrics.density

    /**
     * Scale converting incoming drag coordinates to physical pixels. WebView
     * surfaces report CSS pixels (scale = density); raw `MotionEvent` surfaces
     * report physical pixels (scale = 1f).
     */
    private val coordinateScale: Float = coordinateScaleOverride ?: density
    private var targets: List<MorphDismissZone.DismissTargetDef>? = null
    private var zone: MorphDismissZone? = null
    private val lastDistanceSq = FloatArray(if (showDismissAll) 2 else 1) { Float.POSITIVE_INFINITY }

    val hasDismissAll: Boolean get() = showDismissAll

    val isShowing: Boolean get() = zone != null

    fun ensureShown() {
        if (zone != null) return
        zone = MorphDismissZone(
            context = context,
            windowManager = windowManager,
            targets = dismissTargets(),
        ).also { it.show() }
    }

    fun update(hit: DismissHit) {
        ensureShown()
        zone?.update(
            if (showDismissAll) {
                floatArrayOf(hit.singleProximity, hit.allProximity)
            } else {
                floatArrayOf(hit.singleProximity)
            },
        )
    }

    fun hide() {
        zone?.hide()
        zone = null
        targets = null
        resetTracking()
    }

    /** Swallow animation on the target matching [action], then call [onDone]. */
    fun swallow(action: DismissAction, onDone: () -> Unit) {
        val idx = when (action) {
            DismissAction.SINGLE -> 0
            DismissAction.ALL -> if (showDismissAll) 1 else { onDone(); return }
            DismissAction.NONE -> { onDone(); return }
        }
        zone?.swallow(idx, onDone) ?: onDone()
    }

    fun resetTracking() {
        lastDistanceSq.fill(Float.POSITIVE_INFINITY)
    }

    fun hit(rawXY: String, screenBounds: Rect): DismissHit {
        val parts = rawXY.split(",")
        if (parts.size != 2) return DismissHit(0f, 0f)
        val x = parts[0].toFloatOrNull() ?: return DismissHit(0f, 0f)
        val y = parts[1].toFloatOrNull() ?: return DismissHit(0f, 0f)
        return hit(x, y, screenBounds)
    }

    fun hit(x: Int, y: Int, screenBounds: Rect): DismissHit = hit(x.toFloat(), y.toFloat(), screenBounds)

    fun hit(x: Float, y: Float, screenBounds: Rect): DismissHit {
        val result = MorphDismissZone.hitTest(
            rawX = x,
            rawY = y,
            screenBounds = screenBounds,
            density = density,
            coordinateScale = coordinateScale,
            targets = dismissTargets(),
            previousDistanceSq = lastDistanceSq,
            layoutDirection = context.resources.configuration.layoutDirection,
        )
        result.distanceSq.copyInto(lastDistanceSq)
        return DismissHit(
            singleProximity = result.proximities.getOrElse(0) { 0f },
            allProximity = result.proximities.getOrElse(1) { 0f },
        )
    }

    /** Physical-pixel screen centre of the [action]'s bubble, or null if absent. */
    fun targetCenterPx(action: DismissAction, screenBounds: Rect): Pair<Float, Float>? {
        val idx = when (action) {
            DismissAction.SINGLE -> 0
            DismissAction.ALL -> if (showDismissAll) 1 else return null
            DismissAction.NONE -> return null
        }
        return MorphDismissZone.targetCentersPx(
            screenBounds = screenBounds,
            density = density,
            targets = dismissTargets(),
            layoutDirection = context.resources.configuration.layoutDirection,
        ).getOrNull(idx)
    }

    private fun dismissTargets(): List<MorphDismissZone.DismissTargetDef> {
        return targets ?: run {
            val defs = if (showDismissAll) {
                MorphDismissZone.singleAndAll(allLabel = allLabel())
            } else {
                MorphDismissZone.singleDismiss()
            }
            defs.also { targets = it }
        }
    }
}
