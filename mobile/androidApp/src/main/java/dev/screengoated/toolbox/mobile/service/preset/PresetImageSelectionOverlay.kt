package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.PointF
import android.graphics.Rect
import android.graphics.RectF
import android.os.Build
import android.text.TextUtils
import android.view.Gravity
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import java.io.ByteArrayOutputStream
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

internal class PresetImageSelectionOverlay(
    context: Context,
    private val windowManager: WindowManager,
    uiLanguage: String,
    title: String,
    private val trace: ImageCaptureTrace,
    screenshotBitmap: Bitmap,
    private val onSelectionConfirmed: (ByteArray) -> Unit,
    private val onColorPicked: (String) -> Unit,
    private val onCancelled: () -> Unit,
) {
    private val overlayBounds = fullOverlayBounds(context, windowManager)
    private val selectionView = PresetImageSelectionView(
        context = context,
        trace = trace,
        windowManager = windowManager,
        sourceBitmap = screenshotBitmap,
        onSelectionConfirmed = onSelectionConfirmed,
        onColorPicked = onColorPicked,
    )
    private val layoutParams = WindowManager.LayoutParams(
        overlayBounds.width(),
        overlayBounds.height(),
        WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
        WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
        android.graphics.PixelFormat.TRANSLUCENT,
    ).apply {
        gravity = Gravity.TOP or Gravity.START
        x = overlayBounds.left
        y = overlayBounds.top
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }
    }
    private val root = FrameLayout(context).apply {
        setBackgroundColor(Color.TRANSPARENT)
        alpha = 0f
        addView(selectionView, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        addView(buildChrome(context, uiLanguage, title), FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.TOP))
    }

    private fun buildChrome(
        context: Context,
        uiLanguage: String,
        title: String,
    ): View {
        val density = context.resources.displayMetrics.density
        val typeface = condensedRoundedTypeface(context)
        val estimatedInsets = estimatedSystemBarInsets(context)
        val container = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding((12 * density).roundToInt(), estimatedInsets.top + (12 * density).roundToInt(), (12 * density).roundToInt(), 0)
        }
        val label = TextView(context).apply {
            text = "$title • " + overlayLocalized(
                uiLanguage,
                "Drag to select, tap to pick color, pinch to zoom",
                "Kéo: chọn, chạm: lấy màu, chụm: zoom",
                "드래그로 선택, 탭으로 색상 추출, 핀치로 확대",
            )
            setTextColor(Color.WHITE)
            textSize = 11.5f
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
            typeface?.let { this.typeface = it }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                fontVariationSettings = "'wght' 615, 'wdth' 68, 'ROND' 100"
            }
            setPadding((6 * density).roundToInt(), (10 * density).roundToInt(), (6 * density).roundToInt(), (10 * density).roundToInt())
            setShadowLayer(14f, 0f, 3f, Color.argb(180, 0, 0, 0))
        }
        val cancel = TextView(context).apply {
            text = overlayLocalized(uiLanguage, "Cancel", "Hủy", "취소")
            setTextColor(Color.WHITE)
            textSize = 14f
            background = puffyBackground(
                fillColor = Color.argb(196, 26, 26, 32),
                strokeColor = Color.argb(112, 255, 255, 255),
                strokeWidthPx = density,
            )
            typeface?.let { this.typeface = it }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                fontVariationSettings = "'wght' 640, 'wdth' 82, 'ROND' 100"
            }
            setPadding((16 * density).roundToInt(), (11 * density).roundToInt(), (16 * density).roundToInt(), (11 * density).roundToInt())
            elevation = 4 * density
            setOnClickListener { onCancelled() }
        }
        container.addView(label, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { rightMargin = (10 * density).roundToInt() })
        container.addView(cancel)
        return container
    }

    fun show() {
        logImageCaptureTrace(trace, "overlay_show_requested")
        windowManager.addView(root, layoutParams)
        root.post { logImageCaptureTrace(trace, "overlay_first_frame_posted") }
        root.animate()
            .alpha(1f)
            .setDuration(FADE_IN_DURATION_MS)
            .withStartAction { logImageCaptureTrace(trace, "overlay_fade_in_started") }
            .withEndAction { logImageCaptureTrace(trace, "overlay_fade_in_completed") }
            .start()
    }

    fun destroy() {
        selectionView.destroy()
        runCatching { windowManager.removeViewImmediate(root) }
    }

    private companion object {
        private const val FADE_IN_DURATION_MS = 170L
    }
}

private class PresetImageSelectionView(
    context: Context,
    private val trace: ImageCaptureTrace,
    private val windowManager: WindowManager,
    private val sourceBitmap: Bitmap,
    private val onSelectionConfirmed: (ByteArray) -> Unit,
    private val onColorPicked: (String) -> Unit,
) : View(context) {
    private val dimPaint = Paint().apply { color = Color.argb(168, 0, 0, 0) }
    private val selectionBorderPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = context.resources.displayMetrics.density * 2f
        color = Color.WHITE
    }
    private val handlePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.FILL
        color = Color.WHITE
    }
    private val scaleDetector = ScaleGestureDetector(context, ScaleListener())
    private val baseRect = RectF()
    private val contentBounds = RectF()
    private var contentReadyLogged = false
    private var userScale = 1f
    private var panX = 0f
    private var panY = 0f
    private var selectionStartImage: PointF? = null
    private var selectionEndImage: PointF? = null
    private var lastMultiFocusX = 0f
    private var lastMultiFocusY = 0f
    private var selecting = false
    private var lastTapImagePoint: PointF? = null
    private val tapSlopPx = context.resources.displayMetrics.density * 14f

    init {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            requestApplyInsets()
        }
    }
    override fun onApplyWindowInsets(insets: WindowInsets): WindowInsets {
        updateViewport()
        return super.onApplyWindowInsets(insets)
    }
    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        updateViewport()
        updateGestureExclusion(w, h)
    }
    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        if (baseRect.isEmpty) {
            canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), dimPaint)
            return
        }
        val displayBitmap = sourceBitmap
        val displayRect = currentDisplayRect()
        canvas.save()
        canvas.clipRect(contentBounds)
        canvas.drawBitmap(displayBitmap, null, displayRect, null)
        canvas.restore()
        canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), dimPaint)

        val selectionRect = currentSelectionImageRect()?.let(::imageRectToViewRect)
        if (selectionRect != null) {
            canvas.save()
            canvas.clipRect(selectionRect)
            canvas.drawBitmap(displayBitmap, null, displayRect, null)
            canvas.restore()
            canvas.drawRect(selectionRect, selectionBorderPaint)
            drawHandle(canvas, selectionRect.left, selectionRect.top)
            drawHandle(canvas, selectionRect.right, selectionRect.top)
            drawHandle(canvas, selectionRect.left, selectionRect.bottom)
            drawHandle(canvas, selectionRect.right, selectionRect.bottom)
        }
    }
    override fun onTouchEvent(event: MotionEvent): Boolean {
        scaleDetector.onTouchEvent(event)
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (!canStartSelection(event.x, event.y)) {
                    clearSelection()
                    return true
                }
                selecting = true
                val point = mapViewToImage(event.x, event.y)
                selectionStartImage = point
                selectionEndImage = point
                lastTapImagePoint = point
                logImageCaptureTrace(
                    trace,
                    "selection_touch_down",
                    "x=${event.x.roundToInt()} y=${event.y.roundToInt()}",
                )
                invalidate()
            }

            MotionEvent.ACTION_POINTER_DOWN -> {
                selecting = false
                lastMultiFocusX = eventFocusX(event)
                lastMultiFocusY = eventFocusY(event)
            }

            MotionEvent.ACTION_MOVE -> {
                if (event.pointerCount >= 2) {
                    val focusX = eventFocusX(event)
                    val focusY = eventFocusY(event)
                    if (!scaleDetector.isInProgress && userScale > 1f) {
                        panX += focusX - lastMultiFocusX
                        panY += focusY - lastMultiFocusY
                        clampPan()
                        invalidate()
                    }
                    lastMultiFocusX = focusX
                    lastMultiFocusY = focusY
                } else if (selecting) {
                    selectionEndImage = mapViewToImage(event.x, event.y)
                    lastTapImagePoint = selectionEndImage
                    invalidate()
                }
            }

            MotionEvent.ACTION_UP -> {
                if (selecting) {
                    finishSelection()
                }
                selecting = false
            }

            MotionEvent.ACTION_CANCEL -> {
                clearSelection()
            }
        }
        return true
    }
    fun destroy() {
        if (!sourceBitmap.isRecycled) {
            sourceBitmap.recycle()
        }
    }
    private fun updateGestureExclusion(w: Int, h: Int) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q && w > 0 && h > 0) {
            val edgeWidth = (GESTURE_EXCLUSION_EDGE_DP * resources.displayMetrics.density).roundToInt()
            systemGestureExclusionRects = listOf(
                Rect(0, 0, edgeWidth, h),
                Rect(w - edgeWidth, 0, w, h),
            )
        }
    }

    private fun updateViewport() {
        if (width <= 0 || height <= 0 || sourceBitmap.isRecycled) return
        val viewportInsets = resolveViewportInsets()
        val debugInsets = viewportInsets.contentInsets
        contentBounds.set(0f, 0f, width.toFloat(), height.toFloat())

        val displayBitmap = sourceBitmap
        val scale = max(
            contentBounds.width() / displayBitmap.width.toFloat(),
            contentBounds.height() / displayBitmap.height.toFloat(),
        ) * CONTENT_OVERSCAN
        val drawW = displayBitmap.width * scale
        val drawH = displayBitmap.height * scale
        baseRect.set(
            contentBounds.left + (contentBounds.width() - drawW) / 2f,
            contentBounds.top + (contentBounds.height() - drawH) / 2f,
            contentBounds.left + (contentBounds.width() + drawW) / 2f,
            contentBounds.top + (contentBounds.height() + drawH) / 2f,
        )
        clampPan()
        if (!contentReadyLogged) {
            logImageCaptureTrace(
                trace,
                "content_surface_ready",
                "cropInsets=disabled debugInsets=${debugInsets.left},${debugInsets.top},${debugInsets.right},${debugInsets.bottom} source=${viewportInsets.source} metrics=${viewportInsets.metricsInsets.left},${viewportInsets.metricsInsets.top},${viewportInsets.metricsInsets.right},${viewportInsets.metricsInsets.bottom} root=${viewportInsets.runtimeInsets.left},${viewportInsets.runtimeInsets.top},${viewportInsets.runtimeInsets.right},${viewportInsets.runtimeInsets.bottom} estimate=${viewportInsets.estimatedInsets.left},${viewportInsets.estimatedInsets.top},${viewportInsets.estimatedInsets.right},${viewportInsets.estimatedInsets.bottom} fit=cover overscan=$CONTENT_OVERSCAN surface=${width}x${height} content=${displayBitmap.width}x${displayBitmap.height}",
            )
            contentReadyLogged = true
        }
        invalidate()
    }
    private fun resolveViewportInsets(): ViewportInsets {
        val estimated = sanitizedInsets(estimatedSystemBarInsets(context))
        val metrics = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            windowManager.currentWindowMetrics
                .windowInsets
                .getInsetsIgnoringVisibility(
                    WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout(),
                )
                .let { Rect(it.left, it.top, it.right, it.bottom) }
                .let(::sanitizedInsets)
        } else {
            null
        }
        val root = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            rootWindowInsets
                ?.getInsetsIgnoringVisibility(
                    WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout(),
                )
                ?.let { Rect(it.left, it.top, it.right, it.bottom) }
                ?.let(::sanitizedInsets)
        } else {
            null
        }
        val metricsLooksUsable = metrics?.let(::hasVisibleInsets) == true
        if (metricsLooksUsable) {
            return ViewportInsets(
                contentInsets = metrics ?: Rect(),
                runtimeInsets = root ?: Rect(),
                metricsInsets = metrics ?: Rect(),
                estimatedInsets = estimated,
                source = "window_metrics",
            )
        }
        val rootLooksUsable = root?.let(::hasVisibleInsets) == true
        if (rootLooksUsable) {
            return ViewportInsets(
                contentInsets = root ?: Rect(),
                runtimeInsets = root ?: Rect(),
                metricsInsets = metrics ?: Rect(),
                estimatedInsets = estimated,
                source = "root_insets",
            )
        }
        return ViewportInsets(
            contentInsets = estimated,
            runtimeInsets = root ?: Rect(),
            metricsInsets = metrics ?: Rect(),
            estimatedInsets = estimated,
            source = "estimated_fallback",
        )
    }

    private fun hasVisibleInsets(insets: Rect): Boolean {
        return insets.top > 0 || insets.bottom > 0 || insets.left > 0 || insets.right > 0
    }
    private fun sanitizedInsets(candidate: Rect): Rect {
        var left = candidate.left.coerceAtLeast(0)
        var top = candidate.top.coerceAtLeast(0)
        var right = candidate.right.coerceAtLeast(0)
        var bottom = candidate.bottom.coerceAtLeast(0)
        if (left + right >= sourceBitmap.width - MIN_CONTENT_EDGE_PX) {
            left = 0
            right = 0
        }
        if (top + bottom >= sourceBitmap.height - MIN_CONTENT_EDGE_PX) {
            top = 0
            bottom = 0
        }
        return Rect(left, top, right, bottom)
    }
    private fun canStartSelection(viewX: Float, viewY: Float): Boolean {
        return contentBounds.contains(viewX, viewY)
    }
    private fun finishSelection() {
        val viewRect = currentSelectionImageRect()?.let(::imageRectToViewRect)
        val displayBitmap = sourceBitmap
        if (viewRect == null || displayBitmap.isRecycled) {
            invalidate()
            return
        }
        if (viewRect.width() <= tapSlopPx && viewRect.height() <= tapSlopPx) {
            val point = lastTapImagePoint ?: return
            val x = point.x.toInt().coerceIn(0, displayBitmap.width - 1)
            val y = point.y.toInt().coerceIn(0, displayBitmap.height - 1)
            val color = displayBitmap.getPixel(x, y)
            logImageCaptureTrace(trace, "color_pick_sampled", "x=$x y=$y")
            onColorPicked(
                "#%02X%02X%02X".format(
                    Color.red(color),
                    Color.green(color),
                    Color.blue(color),
                ),
            )
        } else {
            cropSelection()?.let(onSelectionConfirmed)
        }
        clearSelection()
    }
    private fun cropSelection(): ByteArray? {
        val rect = currentSelectionImageRect() ?: return null
        val displayBitmap = sourceBitmap
        val left = rect.left.toInt().coerceIn(0, displayBitmap.width - 1)
        val top = rect.top.toInt().coerceIn(0, displayBitmap.height - 1)
        val right = rect.right.toInt().coerceIn(left + 1, displayBitmap.width)
        val bottom = rect.bottom.toInt().coerceIn(top + 1, displayBitmap.height)
        logImageCaptureTrace(trace, "selection_crop_started", "rect=$left,$top,$right,$bottom")
        val cropped = Bitmap.createBitmap(displayBitmap, left, top, right - left, bottom - top)
        return ByteArrayOutputStream().use { out ->
            cropped.compress(Bitmap.CompressFormat.PNG, 100, out)
            cropped.recycle()
            out.toByteArray()
        }
    }
    private fun currentSelectionImageRect(): RectF? {
        val start = selectionStartImage ?: return null
        val end = selectionEndImage ?: return null
        return RectF(
            min(start.x, end.x),
            min(start.y, end.y),
            max(start.x, end.x),
            max(start.y, end.y),
        )
    }
    private fun imageRectToViewRect(imageRect: RectF): RectF {
        val displayRect = currentDisplayRect()
        val displayBitmap = sourceBitmap
        val scaleX = displayRect.width() / displayBitmap.width
        val scaleY = displayRect.height() / displayBitmap.height
        return RectF(
            displayRect.left + (imageRect.left * scaleX),
            displayRect.top + (imageRect.top * scaleY),
            displayRect.left + (imageRect.right * scaleX),
            displayRect.top + (imageRect.bottom * scaleY),
        )
    }
    private fun mapViewToImage(viewX: Float, viewY: Float): PointF {
        val displayRect = currentDisplayRect()
        val displayBitmap = sourceBitmap
        val x = ((viewX - displayRect.left) / displayRect.width()) * displayBitmap.width
        val y = ((viewY - displayRect.top) / displayRect.height()) * displayBitmap.height
        return PointF(
            x.coerceIn(0f, displayBitmap.width.toFloat()),
            y.coerceIn(0f, displayBitmap.height.toFloat()),
        )
    }
    private fun currentDisplayRect(): RectF {
        if (baseRect.isEmpty) return RectF()
        val scaledWidth = baseRect.width() * userScale
        val scaledHeight = baseRect.height() * userScale
        val centeredLeft = contentBounds.left + (contentBounds.width() - scaledWidth) / 2f
        val centeredTop = contentBounds.top + (contentBounds.height() - scaledHeight) / 2f
        return RectF(
            centeredLeft + panX,
            centeredTop + panY,
            centeredLeft + scaledWidth + panX,
            centeredTop + scaledHeight + panY,
        )
    }
    private fun clampPan() {
        if (baseRect.isEmpty) {
            panX = 0f
            panY = 0f
            return
        }
        val scaledWidth = baseRect.width() * userScale
        val scaledHeight = baseRect.height() * userScale
        val maxPanX = max(0f, (scaledWidth - contentBounds.width()) / 2f)
        val maxPanY = max(0f, (scaledHeight - contentBounds.height()) / 2f)
        panX = panX.coerceIn(-maxPanX, maxPanX)
        panY = panY.coerceIn(-maxPanY, maxPanY)
    }
    private fun clearSelection() {
        selecting = false
        selectionStartImage = null
        selectionEndImage = null
        lastTapImagePoint = null
        invalidate()
    }
    private fun drawHandle(canvas: Canvas, x: Float, y: Float) {
        canvas.drawCircle(x, y, context.resources.displayMetrics.density * 4f, handlePaint)
    }
    private fun eventFocusX(event: MotionEvent): Float {
        var total = 0f
        for (index in 0 until event.pointerCount) {
            total += event.getX(index)
        }
        return total / event.pointerCount
    }
    private fun eventFocusY(event: MotionEvent): Float {
        var total = 0f
        for (index in 0 until event.pointerCount) {
            total += event.getY(index)
        }
        return total / event.pointerCount
    }
    private inner class ScaleListener : ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScale(detector: ScaleGestureDetector): Boolean {
            val displayRect = currentDisplayRect()
            if (!displayRect.contains(detector.focusX, detector.focusY)) {
                return false
            }
            val focusImage = mapViewToImage(detector.focusX, detector.focusY)
            userScale = (userScale * detector.scaleFactor).coerceIn(1f, MAX_SCALE)
            val displayBitmap = sourceBitmap
            val baseScaleX = baseRect.width() / displayBitmap.width
            val baseScaleY = baseRect.height() / displayBitmap.height
            val scaledLeft = detector.focusX - (focusImage.x * baseScaleX * userScale)
            val scaledTop = detector.focusY - (focusImage.y * baseScaleY * userScale)
            val centeredLeft = contentBounds.left + (contentBounds.width() - (baseRect.width() * userScale)) / 2f
            val centeredTop = contentBounds.top + (contentBounds.height() - (baseRect.height() * userScale)) / 2f
            panX = scaledLeft - centeredLeft
            panY = scaledTop - centeredTop
            clampPan()
            invalidate()
            return true
        }
    }
    private companion object {
        private const val MAX_SCALE = 4f
        private const val MIN_CONTENT_EDGE_PX = 48
        private const val CONTENT_OVERSCAN = 1f
        private const val GESTURE_EXCLUSION_EDGE_DP = 48f
    }
}
private data class ViewportInsets(
    val contentInsets: Rect,
    val runtimeInsets: Rect,
    val metricsInsets: Rect,
    val estimatedInsets: Rect,
    val source: String,
)
