package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.Rect
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.TextView
import kotlin.math.roundToInt
import kotlin.math.sqrt

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
    private var dismissBubbleView: FrameLayout? = null
    private var lastSingleDistSq = Int.MAX_VALUE
    private var lastAllDistSq = Int.MAX_VALUE

    fun ensureShown() {
        if (dismissBubbleView != null) return
        val bubbleSize = dp(56)
        val allCircle = dismissCircleView(baseColor = android.graphics.Color.argb(196, 56, 56, 68))
        val allLabel = TextView(context).apply {
            text = allLabelText()
            textSize = 14f
            setTextColor(android.graphics.Color.WHITE)
            gravity = Gravity.CENTER
            setTypeface(typeface, android.graphics.Typeface.BOLD)
            includeFontPadding = false
            alpha = 0f
            scaleX = 0.4f
            scaleY = 0.4f
        }
        val singleCircle = dismissCircleView(baseColor = android.graphics.Color.argb(200, 60, 60, 60))
        val singleIcon = TextView(context).apply {
            text = "\u00D7"
            textSize = 24f
            setTextColor(android.graphics.Color.WHITE)
            gravity = Gravity.CENTER
            alpha = 0f
            scaleX = 0.4f
            scaleY = 0.4f
        }
        val container = FrameLayout(context).apply {
            addView(
                allCircle,
                FrameLayout.LayoutParams(bubbleSize, bubbleSize).apply {
                    gravity = Gravity.BOTTOM or Gravity.START
                    leftMargin = dp(18)
                    bottomMargin = dp(24)
                },
            )
            addView(
                allLabel,
                FrameLayout.LayoutParams(bubbleSize, bubbleSize).apply {
                    gravity = Gravity.BOTTOM or Gravity.START
                    leftMargin = dp(18)
                    bottomMargin = dp(24)
                },
            )
            addView(
                singleCircle,
                FrameLayout.LayoutParams(bubbleSize, bubbleSize).apply {
                    gravity = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL
                    bottomMargin = dp(24)
                },
            )
            addView(
                singleIcon,
                FrameLayout.LayoutParams(bubbleSize, bubbleSize).apply {
                    gravity = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL
                    bottomMargin = dp(24)
                },
            )
        }
        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT,
            bubbleSize * 2,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
            android.graphics.PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.BOTTOM or Gravity.START
        }
        dismissBubbleView = container
        runCatching { windowManager.addView(container, params) }
        listOf(allCircle, allLabel, singleCircle, singleIcon).forEach { target ->
            target.animate()
                .alpha(1f)
                .scaleX(1f)
                .scaleY(1f)
                .setDuration(250)
                .setInterpolator(android.view.animation.OvershootInterpolator(1.5f))
                .start()
        }
    }

    fun update(hit: PresetOverlayDismissHit) {
        ensureShown()
        val view = dismissBubbleView ?: return
        updateTargetVisual(
            circle = view.getChildAt(0),
            label = view.getChildAt(1),
            proximity = hit.allProximity,
            baseColor = android.graphics.Color.argb(196, 56, 56, 68),
        )
        updateTargetVisual(
            circle = view.getChildAt(2),
            label = view.getChildAt(3),
            proximity = hit.singleProximity,
            baseColor = android.graphics.Color.argb(200, 60, 60, 60),
        )
    }

    fun hide() {
        val view = dismissBubbleView ?: return
        val children = (0 until view.childCount).map(view::getChildAt)
        children.firstOrNull()?.animate()
            ?.alpha(0f)
            ?.setDuration(200)
            ?.withEndAction {
                runCatching { windowManager.removeView(view) }
                dismissBubbleView = null
            }
            ?.start()
        children.drop(1).forEach { child ->
            child.animate()
                .alpha(0f)
                .scaleX(0.3f)
                .scaleY(0.3f)
                .setDuration(200)
                .start()
        }
    }

    fun resetTracking() {
        lastSingleDistSq = Int.MAX_VALUE
        lastAllDistSq = Int.MAX_VALUE
    }

    fun hit(rawXY: String, screenBounds: Rect): PresetOverlayDismissHit {
        val parts = rawXY.split(",")
        if (parts.size != 2) return PresetOverlayDismissHit(0f, 0f)
        val x = parts[0].toIntOrNull() ?: return PresetOverlayDismissHit(0f, 0f)
        val y = parts[1].toIntOrNull() ?: return PresetOverlayDismissHit(0f, 0f)
        return hit(x, y, screenBounds)
    }

    fun hit(x: Int, y: Int, screenBounds: Rect): PresetOverlayDismissHit {
        val targetCenterCssY = ((screenBounds.height() - statusBarHeight() - dp(24) - dp(28)) / density).toInt()
        val singleCenterCssX = (screenBounds.width() / density / 2f).toInt()
        val allCenterCssX = (dp(18 + 28) / density).toInt()
        val single = proximityForTarget(
            x = x,
            y = y,
            centerX = singleCenterCssX,
            centerY = targetCenterCssY,
            lastDistanceSq = lastSingleDistSq,
        )
        lastSingleDistSq = single.second
        val all = proximityForTarget(
            x = x,
            y = y,
            centerX = allCenterCssX,
            centerY = targetCenterCssY,
            lastDistanceSq = lastAllDistSq,
        )
        lastAllDistSq = all.second
        return PresetOverlayDismissHit(
            singleProximity = single.first,
            allProximity = all.first,
        )
    }

    private fun dismissCircleView(baseColor: Int): View {
        return View(context).apply {
            background = android.graphics.drawable.GradientDrawable().apply {
                shape = android.graphics.drawable.GradientDrawable.OVAL
                setColor(baseColor)
            }
            alpha = 0f
            scaleX = 0.4f
            scaleY = 0.4f
        }
    }

    private fun updateTargetVisual(
        circle: View,
        label: View,
        proximity: Float,
        baseColor: Int,
    ) {
        val scale = 1f + proximity * 0.35f
        circle.scaleX = scale
        circle.scaleY = scale
        label.scaleX = scale
        label.scaleY = scale
        val baseR = android.graphics.Color.red(baseColor)
        val baseG = android.graphics.Color.green(baseColor)
        val baseB = android.graphics.Color.blue(baseColor)
        val baseA = android.graphics.Color.alpha(baseColor)
        val r = (baseR + ((220 - baseR) * proximity)).toInt().coerceIn(0, 255)
        val g = (baseG + ((50 - baseG) * proximity)).toInt().coerceIn(0, 255)
        val b = (baseB + ((50 - baseB) * proximity)).toInt().coerceIn(0, 255)
        val a = (baseA + (24 * proximity)).toInt().coerceIn(0, 255)
        (circle.background as? android.graphics.drawable.GradientDrawable)
            ?.setColor(android.graphics.Color.argb(a, r, g, b))
    }

    private fun proximityForTarget(
        x: Int,
        y: Int,
        centerX: Int,
        centerY: Int,
        lastDistanceSq: Int,
    ): Pair<Float, Int> {
        val dx = x - centerX
        val dy = y - centerY
        val distSq = dx * dx + dy * dy
        val approaching = distSq < lastDistanceSq
        val hitRadius = 55f
        val outerRadius = if (approaching) 140f else 110f
        val dist = sqrt(distSq.toFloat())
        val proximity = if (dist <= hitRadius) {
            1f
        } else if (dist <= outerRadius) {
            1f - (dist - hitRadius) / (outerRadius - hitRadius)
        } else {
            0f
        }
        return proximity to distSq
    }

    private fun statusBarHeight(): Int {
        val resourceId = context.resources.getIdentifier("status_bar_height", "dimen", "android")
        return if (resourceId > 0) context.resources.getDimensionPixelSize(resourceId) else dp(24)
    }

    private fun allLabelText(): String = when (uiLanguage()) {
        "vi" -> "Tất cả"
        "ko" -> "전체"
        else -> "All"
    }

    private fun dp(value: Int): Int = (value * density).roundToInt()
}
