package dev.screengoated.toolbox.mobile.service.overlay

import android.content.Context
import android.graphics.Color
import android.graphics.Outline
import android.graphics.Rect
import android.graphics.drawable.GradientDrawable
import android.view.ScaleGestureDetector
import android.view.View
import android.view.ViewOutlineProvider
import android.view.WindowManager
import android.webkit.WebView
import android.widget.FrameLayout
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import dev.screengoated.toolbox.mobile.service.OverlayPaneHolder
import dev.screengoated.toolbox.mobile.service.OverlayPaneId
import dev.screengoated.toolbox.mobile.service.OverlayPaneRuntimeSettings
import dev.screengoated.toolbox.mobile.service.buildOverlayWebView
import kotlin.math.abs
import kotlin.math.roundToInt

internal class OverlayPaneWindow(
    context: Context,
    private val windowManager: WindowManager,
    private val paneId: OverlayPaneId,
    initialBounds: OverlayBounds,
    private val minWidthPx: Int,
    private val minHeightPx: Int,
    private val screenBoundsProvider: () -> Rect,
    private val onBoundsChanged: (OverlayPaneId, OverlayBounds) -> Unit,
    onMessage: (OverlayPaneId, String) -> Unit,
) {
    private val cornerRadiusPx = context.resources.displayMetrics.density * 12f
    private val layoutParams = WindowManager.LayoutParams().apply {
        copyFrom(
            WindowManager.LayoutParams(
                initialBounds.width,
                initialBounds.height,
                WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                    WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                    WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
                android.graphics.PixelFormat.TRANSLUCENT,
            ),
        )
        gravity = android.view.Gravity.TOP or android.view.Gravity.START
        x = initialBounds.x
        y = initialBounds.y
    }

    private val rootView = FrameLayout(context).apply {
        setBackgroundColor(Color.TRANSPARENT)
        clipToOutline = true
        clipChildren = true
        background = GradientDrawable().apply {
            setColor(Color.TRANSPARENT)
            cornerRadius = cornerRadiusPx
        }
        outlineProvider = object : ViewOutlineProvider() {
            override fun getOutline(
                view: View,
                outline: Outline,
            ) {
                outline.setRoundRect(0, 0, view.width, view.height, cornerRadiusPx)
            }
        }
    }

    private val webView: WebView = buildOverlayWebView(context, paneId, onMessage).also { view ->
        rootView.addView(
            view,
            FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT,
            ),
        )
    }
    private val paneHolder = OverlayPaneHolder(paneId, rootView, webView)
    private val scaleDetector = ScaleGestureDetector(
        context,
        object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
            override fun onScale(detector: ScaleGestureDetector): Boolean {
                resizeByGesture(detector)
                return true
            }
        },
    )

    private var attached = false

    init {
        webView.setOnTouchListener { _, event ->
            scaleDetector.onTouchEvent(event)
            false
        }
    }

    fun show() {
        if (attached) {
            return
        }
        windowManager.addView(rootView, layoutParams)
        attached = true
    }

    fun hide() {
        if (!attached) {
            return
        }
        runCatching { windowManager.removeView(rootView) }
        attached = false
    }

    fun destroy() {
        hide()
        paneHolder.destroy()
    }

    fun moveBy(
        deltaX: Int,
        deltaY: Int,
    ) {
        updateBounds(
            bounds = currentBounds().let { bounds ->
                val screen = screenBoundsProvider()
                bounds.copy(
                    x = (bounds.x + deltaX).coerceIn(0, (screen.width() - bounds.width).coerceAtLeast(0)),
                    y = (bounds.y + deltaY).coerceIn(0, (screen.height() - bounds.height).coerceAtLeast(0)),
                )
            },
        )
    }

    fun currentBounds(): OverlayBounds {
        return OverlayBounds(
            x = layoutParams.x,
            y = layoutParams.y,
            width = layoutParams.width,
            height = layoutParams.height,
        )
    }

    fun render(
        html: String,
        settings: OverlayPaneRuntimeSettings,
        oldText: String,
        newText: String,
    ): Boolean {
        return paneHolder.render(html, settings, oldText, newText)
    }

    fun evaluate(script: String) {
        paneHolder.evaluate(script)
    }

    fun onReady() {
        paneHolder.onReady()
    }

    /**
     * Resize from a corner handle.
     * @param corner "bl" (bottom-left) or "br" (bottom-right)
     * @param dx horizontal drag delta in pixels
     * @param dy vertical drag delta in pixels
     */
    fun resizeFromCorner(corner: String, dx: Int, dy: Int) {
        val current = currentBounds()
        val screen = screenBoundsProvider()
        when (corner) {
            "br" -> {
                // Bottom-right: width grows with dx, height grows with dy, position unchanged
                val nextWidth = (current.width + dx).coerceIn(minWidthPx, screen.width() - current.x)
                val nextHeight = (current.height + dy).coerceIn(minHeightPx, screen.height() - current.y)
                updateBounds(current.copy(width = nextWidth, height = nextHeight))
            }
            "bl" -> {
                // Bottom-left: width shrinks with dx (x moves), height grows with dy
                val nextWidth = (current.width - dx).coerceIn(minWidthPx, current.x + current.width)
                val nextX = (current.x + current.width - nextWidth).coerceAtLeast(0)
                val nextHeight = (current.height + dy).coerceIn(minHeightPx, screen.height() - current.y)
                updateBounds(current.copy(x = nextX, width = nextWidth, height = nextHeight))
            }
        }
    }

    private fun resizeByGesture(detector: ScaleGestureDetector) {
        val scaleX = axisScale(detector.currentSpanX, detector.previousSpanX)
        val scaleY = axisScale(detector.currentSpanY, detector.previousSpanY)
        val dominantX = abs(detector.currentSpanX - detector.previousSpanX)
        val dominantY = abs(detector.currentSpanY - detector.previousSpanY)
        when {
            dominantX > dominantY * AXIS_DOMINANCE_THRESHOLD -> resizeByFactors(scaleX = scaleX, scaleY = 1f)
            dominantY > dominantX * AXIS_DOMINANCE_THRESHOLD -> resizeByFactors(scaleX = 1f, scaleY = scaleY)
            else -> resizeByFactors(scaleX = scaleX, scaleY = scaleY)
        }
    }

    private fun axisScale(
        current: Float,
        previous: Float,
    ): Float {
        if (current <= 0f || previous <= 0f) {
            return 1f
        }
        return (current / previous).coerceIn(0.96f, 1.04f)
    }

    private fun resizeByFactors(
        scaleX: Float,
        scaleY: Float,
    ) {
        val current = currentBounds()
        val screen = screenBoundsProvider()
        val nextWidth = (current.width * scaleX).roundToInt().coerceIn(minWidthPx, screen.width())
        val nextHeight = (current.height * scaleY).roundToInt().coerceIn(minHeightPx, screen.height())
        if (nextWidth == current.width && nextHeight == current.height) {
            return
        }
        val deltaWidth = nextWidth - current.width
        val deltaHeight = nextHeight - current.height
        updateBounds(
            current.copy(
                x = (current.x - (deltaWidth / 2)).coerceIn(0, (screen.width() - nextWidth).coerceAtLeast(0)),
                y = (current.y - (deltaHeight / 2)).coerceIn(0, (screen.height() - nextHeight).coerceAtLeast(0)),
                width = nextWidth,
                height = nextHeight,
            ),
        )
    }

    private fun updateBounds(bounds: OverlayBounds) {
        layoutParams.x = bounds.x
        layoutParams.y = bounds.y
        layoutParams.width = bounds.width
        layoutParams.height = bounds.height
        if (attached) {
            runCatching { windowManager.updateViewLayout(rootView, layoutParams) }
        }
        onBoundsChanged(paneId, currentBounds())
    }

    private companion object {
        private const val AXIS_DOMINANCE_THRESHOLD = 1.12f
    }
}
