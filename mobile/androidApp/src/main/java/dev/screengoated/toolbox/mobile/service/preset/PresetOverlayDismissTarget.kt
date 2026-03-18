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

internal class PresetOverlayDismissTarget(
    private val context: Context,
    private val windowManager: WindowManager,
) {
    private val density = context.resources.displayMetrics.density
    private var dismissBubbleView: View? = null
    private var lastFingerDistSq = Int.MAX_VALUE

    fun ensureShown() {
        if (dismissBubbleView != null) return
        val bubbleSize = dp(56)
        val circle = View(context).apply {
            background = android.graphics.drawable.GradientDrawable().apply {
                shape = android.graphics.drawable.GradientDrawable.OVAL
                setColor(android.graphics.Color.argb(200, 60, 60, 60))
            }
            alpha = 0f
            scaleX = 0.4f
            scaleY = 0.4f
        }
        val icon = TextView(context).apply {
            text = "\u00D7"
            textSize = 24f
            setTextColor(android.graphics.Color.WHITE)
            gravity = Gravity.CENTER
        }
        val container = FrameLayout(context).apply {
            addView(
                circle,
                FrameLayout.LayoutParams(bubbleSize, bubbleSize).apply {
                    gravity = Gravity.CENTER
                },
            )
            addView(
                icon,
                FrameLayout.LayoutParams(bubbleSize, bubbleSize).apply {
                    gravity = Gravity.CENTER
                },
            )
        }
        val params = WindowManager.LayoutParams(
            bubbleSize * 2,
            bubbleSize * 2,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
            android.graphics.PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL
            y = dp(24)
        }
        dismissBubbleView = container
        runCatching { windowManager.addView(container, params) }
        circle.animate()
            .alpha(1f)
            .scaleX(1f)
            .scaleY(1f)
            .setDuration(250)
            .setInterpolator(android.view.animation.OvershootInterpolator(1.5f))
            .start()
    }

    fun update(proximity: Float) {
        ensureShown()
        val circle = (dismissBubbleView as? FrameLayout)?.getChildAt(0) ?: return
        val scale = 1f + proximity * 0.35f
        circle.scaleX = scale
        circle.scaleY = scale
        val r = (60 + (160 * proximity)).toInt().coerceIn(0, 255)
        val g = (60 - (10 * proximity)).toInt().coerceIn(0, 255)
        val b = (60 - (10 * proximity)).toInt().coerceIn(0, 255)
        val a = (200 + (20 * proximity)).toInt().coerceIn(0, 255)
        (circle.background as? android.graphics.drawable.GradientDrawable)
            ?.setColor(android.graphics.Color.argb(a, r, g, b))
    }

    fun hide() {
        val view = dismissBubbleView ?: return
        val circle = (view as? FrameLayout)?.getChildAt(0)
        if (circle != null) {
            circle.animate()
                .alpha(0f)
                .scaleX(0.3f)
                .scaleY(0.3f)
                .setDuration(200)
                .withEndAction {
                    runCatching { windowManager.removeView(view) }
                    dismissBubbleView = null
                }
                .start()
        } else {
            runCatching { windowManager.removeView(view) }
            dismissBubbleView = null
        }
    }

    fun resetTracking() {
        lastFingerDistSq = Int.MAX_VALUE
    }

    fun proximity(rawXY: String, screenBounds: Rect): Float {
        val parts = rawXY.split(",")
        if (parts.size != 2) return 0f
        val x = parts[0].toIntOrNull() ?: return 0f
        val y = parts[1].toIntOrNull() ?: return 0f
        return proximity(x, y, screenBounds)
    }

    fun proximity(x: Int, y: Int, screenBounds: Rect): Float {
        val bubbleCenterCssX = (screenBounds.width() / density / 2).toInt()
        val bubbleCenterCssY = ((screenBounds.height() - statusBarHeight() - dp(24) - dp(28)) / density).toInt()
        val dx = x - bubbleCenterCssX
        val dy = y - bubbleCenterCssY
        val distSq = dx * dx + dy * dy
        val approaching = distSq < lastFingerDistSq
        lastFingerDistSq = distSq
        val hitRadius = 55f
        val outerRadius = if (approaching) 140f else 110f
        val dist = sqrt(distSq.toFloat())
        return if (dist <= hitRadius) {
            1f
        } else if (dist <= outerRadius) {
            1f - (dist - hitRadius) / (outerRadius - hitRadius)
        } else {
            0f
        }
    }

    private fun statusBarHeight(): Int {
        val resourceId = context.resources.getIdentifier("status_bar_height", "dimen", "android")
        return if (resourceId > 0) context.resources.getDimensionPixelSize(resourceId) else dp(24)
    }

    private fun dp(value: Int): Int = (value * density).roundToInt()
}
