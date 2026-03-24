package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.Rect
import android.view.WindowManager
import dev.screengoated.toolbox.mobile.service.MorphDismissZone

internal enum class PresetOverlayDismissAction {
    NONE,
    SINGLE,
    ALL,
}

internal data class PresetOverlayDismissHit(
    val singleProximity: Float,
    val allProximity: Float,
) {
    val action: PresetOverlayDismissAction
        get() = when {
            singleProximity <= 0f && allProximity <= 0f -> PresetOverlayDismissAction.NONE
            allProximity > singleProximity -> PresetOverlayDismissAction.ALL
            else -> PresetOverlayDismissAction.SINGLE
        }
}

internal class PresetOverlayDismissTarget(
    private val context: Context,
    private val windowManager: WindowManager,
    private val uiLanguage: () -> String,
) {
    private val density = context.resources.displayMetrics.density
    private var targets: List<MorphDismissZone.DismissTargetDef>? = null
    private var zone: MorphDismissZone? = null
    private val lastDistanceSq = FloatArray(2) { Float.POSITIVE_INFINITY }

    fun ensureShown() {
        if (zone != null) return
        zone = MorphDismissZone(
            context = context,
            windowManager = windowManager,
            targets = dismissTargets(),
        ).also { it.show() }
    }

    fun update(hit: PresetOverlayDismissHit) {
        ensureShown()
        zone?.update(floatArrayOf(hit.singleProximity, hit.allProximity))
    }

    fun hide() {
        zone?.hide()
        zone = null
        targets = null
        resetTracking()
    }

    /** Swallow animation on the matching target. */
    fun swallow(action: PresetOverlayDismissAction, onDone: () -> Unit) {
        val idx = when (action) {
            PresetOverlayDismissAction.SINGLE -> 0
            PresetOverlayDismissAction.ALL -> 1
            PresetOverlayDismissAction.NONE -> { onDone(); return }
        }
        zone?.swallow(idx, onDone) ?: onDone()
    }

    fun resetTracking() {
        lastDistanceSq.fill(Float.POSITIVE_INFINITY)
    }

    fun hit(rawXY: String, screenBounds: Rect): PresetOverlayDismissHit {
        val parts = rawXY.split(",")
        if (parts.size != 2) return PresetOverlayDismissHit(0f, 0f)
        val x = parts[0].toIntOrNull() ?: return PresetOverlayDismissHit(0f, 0f)
        val y = parts[1].toIntOrNull() ?: return PresetOverlayDismissHit(0f, 0f)
        return hit(x, y, screenBounds)
    }

    fun hit(x: Int, y: Int, screenBounds: Rect): PresetOverlayDismissHit {
        val hit = MorphDismissZone.hitTest(
            rawX = x.toFloat(),
            rawY = y.toFloat(),
            screenBounds = screenBounds,
            density = density,
            coordinateScale = density,
            targets = dismissTargets(),
            previousDistanceSq = lastDistanceSq,
        )
        hit.distanceSq.copyInto(lastDistanceSq)
        return PresetOverlayDismissHit(
            singleProximity = hit.proximities.getOrElse(0) { 0f },
            allProximity = hit.proximities.getOrElse(1) { 0f },
        )
    }

    private fun allLabelText(): String = when (uiLanguage()) {
        "vi" -> "Tất cả"
        "ko" -> "전체"
        else -> "All"
    }

    private fun dismissTargets(): List<MorphDismissZone.DismissTargetDef> {
        return targets ?: MorphDismissZone.singleAndAll(allLabel = allLabelText()).also { targets = it }
    }
}
